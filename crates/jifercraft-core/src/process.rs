// Process supervisor: spawns Minecraft, streams stdout/stderr without dropping frames,
// rotates game log files, and terminates the entire process tree when asked.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use serde::Serialize;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::error::{CoreError, CoreResult};
use crate::launch::LaunchCommand;

const KEEP_LOG_FILES: usize = 10;
const MAX_LINE_BYTES: usize = 16 * 1024;
const PUMP_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameLogLine {
    pub level: LogLevel,
    pub stream: LogStream,
    pub text: String,
}

pub type LineSink = Arc<dyn Fn(GameLogLine) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitSummary {
    pub code: Option<i32>,
    pub killed: bool,
}

#[derive(Debug)]
pub struct RunningGame {
    pub pid: Option<u32>,
    pub log_file: PathBuf,
    done: JoinHandle<CoreResult<ExitSummary>>,
}

impl RunningGame {
    pub async fn wait(self) -> CoreResult<ExitSummary> {
        self.done
            .await
            .map_err(|error| CoreError::Background(error.to_string()))?
    }
}

/// Starts the game. Cancelling `kill` terminates it; the returned handle resolves once the process and both
/// output pumps have finished.
pub fn spawn(
    command: &LaunchCommand,
    log_dir: &Path,
    on_line: LineSink,
    kill: CancellationToken,
) -> CoreResult<RunningGame> {
    std::fs::create_dir_all(log_dir).map_err(|error| CoreError::io(log_dir, error))?;
    prune_logs(log_dir, KEEP_LOG_FILES - 1);
    let log_file = log_dir.join(format!("game-{}.log", crate::timestamp::now_iso().replace(':', "-")));

    let mut process = Command::new(&command.program);
    process
        .args(&command.args)
        .current_dir(&command.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: no console flashes up behind a javaw-less runtime.
        process.creation_flags(0x0800_0000);
    }
    let mut child = process
        .spawn()
        .map_err(|error| CoreError::LaunchFailed(format!("{}: {error}", command.program.display())))?;
    let pid = child.id();

    let (lines, mut received) = mpsc::channel::<GameLogLine>(1024);
    let stdout = child
        .stdout
        .take()
        .map(|out| tokio::spawn(pump(BufReader::new(out), LogStream::Stdout, lines.clone())));
    let stderr = child
        .stderr
        .take()
        .map(|err| tokio::spawn(pump(BufReader::new(err), LogStream::Stderr, lines.clone())));
    drop(lines);

    let file_path = log_file.clone();
    let writer = tokio::spawn(async move {
        let mut file = match tokio::fs::File::create(&file_path).await {
            Ok(file) => Some(BufWriter::new(file)),
            Err(error) => {
                tracing::warn!(%error, path = %file_path.display(), "game log file unavailable");
                None
            }
        };
        while let Some(line) = received.recv().await {
            if let Some(out) = file.as_mut()
                && (out.write_all(line.text.as_bytes()).await.is_err() || out.write_all(b"\n").await.is_err())
            {
                file = None;
            }
            on_line(line);
            // Flush whenever the burst is over, so the file is useful even if the launcher dies mid-game.
            if received.is_empty()
                && let Some(out) = file.as_mut()
                && out.flush().await.is_err()
            {
                file = None;
            }
        }
        if let Some(mut out) = file {
            let _ = out.flush().await;
        }
    });

    let done = tokio::spawn(async move {
        let (status, killed) = tokio::select! {
            status = child.wait() => (status, false),
            () = kill.cancelled() => {
                kill_tree(pid).await;
                let _ = child.start_kill();
                (child.wait().await, true)
            }
        };
        // Grandchildren can inherit the pipes and keep them open after the game exits; drain what is
        // buffered, then stop waiting for an EOF that may never come.
        for mut pump in [stdout, stderr].into_iter().flatten() {
            if tokio::time::timeout(PUMP_GRACE, &mut pump).await.is_err() {
                pump.abort();
            }
        }
        let _ = writer.await;
        let status = status.map_err(|error| CoreError::LaunchFailed(error.to_string()))?;
        Ok(ExitSummary {
            code: status.code(),
            killed,
        })
    });

    Ok(RunningGame { pid, log_file, done })
}

/// Terminates the whole process tree (the JVM plus anything it spawned). Best effort: the direct child is
/// killed through its handle regardless.
async fn kill_tree(pid: Option<u32>) {
    #[cfg(windows)]
    if let Some(pid) = pid {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status()
            .await;
    }
    #[cfg(not(windows))]
    let _ = pid;
}

async fn pump(mut reader: impl AsyncBufRead + Unpin, stream: LogStream, lines: mpsc::Sender<GameLogLine>) {
    let mut buffer = Vec::with_capacity(256);
    let mut assembler = EventAssembler::default();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        buffer.truncate(MAX_LINE_BYTES);
        // Windows consoles emit the active code page, not UTF-8; lossy decoding keeps lines readable.
        let text = String::from_utf8_lossy(&buffer)
            .trim_end_matches(['\r', '\n'])
            .to_owned();
        if let Some(line) = assembler.push(&text, stream)
            && lines.send(line).await.is_err()
        {
            return;
        }
    }
    if let Some(line) = assembler.finish(stream) {
        let _ = lines.send(line).await;
    }
}

// Mojang's default log4j setup decided stdout should spit out raw serialized XML.
// Yes, raw `<log4j:Event>` chunks spewed across multiple lines with CDATA stack traces.
// We stitch the XML shards back together so the UI console displays clean lines
// instead of looking like an enterprise SOAP service had an aneurysm.
#[derive(Debug, Default)]
pub struct EventAssembler {
    pending: Option<String>,
}

const MAX_EVENT_BYTES: usize = 256 * 1024;

impl EventAssembler {
    pub fn push(&mut self, text: &str, stream: LogStream) -> Option<GameLogLine> {
        if let Some(pending) = self.pending.as_mut() {
            pending.push('\n');
            pending.push_str(text);
            if text.contains("</log4j:Event>") || pending.len() > MAX_EVENT_BYTES {
                return self.pending.take().map(|xml| event_line(&xml, stream));
            }
            return None;
        }
        if text.trim_start().starts_with("<log4j:Event") {
            if text.contains("</log4j:Event>") {
                return Some(event_line(text, stream));
            }
            self.pending = Some(text.to_owned());
            return None;
        }
        if text.trim().is_empty() {
            return None;
        }
        Some(GameLogLine {
            level: classify(text, stream),
            stream,
            text: text.to_owned(),
        })
    }

    /// Flushes an event cut short by the process exiting.
    pub fn finish(&mut self, stream: LogStream) -> Option<GameLogLine> {
        self.pending.take().map(|xml| event_line(&xml, stream))
    }
}

fn event_line(xml: &str, stream: LogStream) -> GameLogLine {
    let Some(message) = cdata_in(xml, "log4j:Message") else {
        return GameLogLine {
            level: classify(xml, stream),
            stream,
            text: xml.to_owned(),
        };
    };
    let level = match attribute(xml, "level").as_deref() {
        Some("FATAL" | "ERROR") => LogLevel::Error,
        Some("WARN") => LogLevel::Warn,
        Some("DEBUG" | "TRACE") => LogLevel::Debug,
        _ => LogLevel::Info,
    };
    let thread = attribute(xml, "thread").unwrap_or_default();
    let label = attribute(xml, "level").unwrap_or_else(|| "INFO".to_owned());
    let mut text = format!("[{thread}/{label}] {}", message.trim_end());
    if let Some(throwable) = cdata_in(xml, "log4j:Throwable") {
        text.push('\n');
        text.push_str(throwable.trim_end());
    }
    GameLogLine { level, stream, text }
}

fn attribute(xml: &str, name: &str) -> Option<String> {
    let head = &xml[..xml.find('>')?];
    let start = head.find(&format!(" {name}=\""))? + name.len() + 3;
    let len = head[start..].find('"')?;
    Some(unescape(&head[start..start + len]))
}

fn cdata_in<'a>(xml: &'a str, element: &str) -> Option<&'a str> {
    let open = xml.find(&format!("<{element}>"))?;
    let rest = &xml[open..];
    let start = rest.find("<![CDATA[")? + "<![CDATA[".len();
    let len = rest[start..].find("]]>")?;
    Some(&rest[start..start + len])
}

fn unescape(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Reads the level from log4j's `[thread/LEVEL]` prefix, falling back to the stream.
pub fn classify(text: &str, stream: LogStream) -> LogLevel {
    if text.contains("/ERROR]") || text.contains("/FATAL]") || text.contains("Exception") || text.starts_with("\tat ") {
        LogLevel::Error
    } else if text.contains("/WARN]") {
        LogLevel::Warn
    } else if text.contains("/DEBUG]") || text.contains("/TRACE]") {
        LogLevel::Debug
    } else if stream == LogStream::Stderr {
        LogLevel::Warn
    } else {
        LogLevel::Info
    }
}

/// Keeps the newest `keep` `game-*.log` files in `dir`.
fn prune_logs(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut logs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("game-"))
                && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
        })
        .collect();
    // Names embed an ISO timestamp, so lexical order is chronological.
    logs.sort();
    let excess = logs.len().saturating_sub(keep);
    for old in logs.into_iter().take(excess) {
        let _ = std::fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    fn shell(script: &str, cwd: &Path) -> LaunchCommand {
        if cfg!(windows) {
            LaunchCommand::new("cmd".into(), vec!["/C".into(), script.into()], cwd.to_path_buf())
        } else {
            LaunchCommand::new("sh".into(), vec!["-c".into(), script.into()], cwd.to_path_buf())
        }
    }

    fn collector() -> (LineSink, Arc<Mutex<Vec<GameLogLine>>>) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&lines);
        (Arc::new(move |line| sink.lock().unwrap().push(line)), lines)
    }

    const XML_SAMPLE: &str = r#"<log4j:Event logger="net.minecraft.client.Minecraft" timestamp="1762768800000" level="INFO" thread="Render thread">
  <log4j:Message><![CDATA[Setting user: Steve]]></log4j:Message>
</log4j:Event>

<log4j:Event logger="net.minecraft.client.renderer.texture.TextureAtlas" timestamp="1762768800001" level="WARN" thread="Worker-Main-2">
  <log4j:Message><![CDATA[Texture minecraft:block/missing with size 15x16 limits mip level]]></log4j:Message>
</log4j:Event>

<log4j:Event logger="net.minecraft.server.MinecraftServer" timestamp="1762768800002" level="ERROR" thread="Server thread">
  <log4j:Message><![CDATA[Encountered an unexpected exception]]></log4j:Message>
  <log4j:Throwable><![CDATA[java.lang.IllegalStateException: boom
	at net.minecraft.server.MinecraftServer.tick(MinecraftServer.java:42)
]]></log4j:Throwable>
</log4j:Event>
Plain stdout from a mod
<log4j:Event logger="x" timestamp="1" level="INFO" thread="cut">
  <log4j:Message><![CDATA[never closed"#;

    #[test]
    fn folds_mojang_xml_events_into_readable_lines() {
        let mut assembler = EventAssembler::default();
        let mut lines: Vec<GameLogLine> = XML_SAMPLE
            .lines()
            .filter_map(|line| assembler.push(line, LogStream::Stdout))
            .collect();
        lines.extend(assembler.finish(LogStream::Stdout));

        let texts: Vec<(&str, LogLevel)> = lines.iter().map(|line| (line.text.as_str(), line.level)).collect();
        assert_eq!(texts[0], ("[Render thread/INFO] Setting user: Steve", LogLevel::Info));
        assert_eq!(texts[1].1, LogLevel::Warn);
        assert!(
            texts[1]
                .0
                .starts_with("[Worker-Main-2/WARN] Texture minecraft:block/missing")
        );
        assert_eq!(texts[2].1, LogLevel::Error);
        assert!(texts[2].0.starts_with(
            "[Server thread/ERROR] Encountered an unexpected exception\njava.lang.IllegalStateException: boom\n\tat "
        ));
        assert_eq!(texts[3], ("Plain stdout from a mod", LogLevel::Info));
        assert_eq!(lines.len(), 5, "a truncated event is still flushed");
        assert!(lines[4].text.contains("never closed"));
    }

    #[test]
    fn classifies_log4j_levels() {
        assert_eq!(
            classify("[12:00:00] [Render thread/ERROR]: boom", LogStream::Stdout),
            LogLevel::Error
        );
        assert_eq!(
            classify("[12:00:00] [main/WARN]: careful", LogStream::Stdout),
            LogLevel::Warn
        );
        assert_eq!(
            classify("[12:00:00] [main/INFO]: hello", LogStream::Stdout),
            LogLevel::Info
        );
        assert_eq!(
            classify("java.lang.IllegalStateException: x", LogStream::Stdout),
            LogLevel::Error
        );
        assert_eq!(classify("\tat net.minecraft.Main", LogStream::Stdout), LogLevel::Error);
        assert_eq!(classify("plain stderr", LogStream::Stderr), LogLevel::Warn);
    }

    #[tokio::test]
    async fn streams_lines_to_the_sink_and_the_log_file() {
        let dir = tempfile::tempdir().unwrap();
        let (sink, lines) = collector();
        let script = "echo [main/INFO]: hello&& echo [main/ERROR]: boom 1>&2&& exit 3";
        let script = if cfg!(windows) {
            script.to_owned()
        } else {
            script.replace("&&", " &&")
        };
        let game = spawn(
            &shell(&script, dir.path()),
            &dir.path().join("logs"),
            sink,
            CancellationToken::new(),
        )
        .unwrap();
        let log_file = game.log_file.clone();
        let exit = game.wait().await.unwrap();

        assert_eq!(
            exit,
            ExitSummary {
                code: Some(3),
                killed: false
            }
        );
        let lines = lines.lock().unwrap().clone();
        assert!(
            lines
                .iter()
                .any(|l| l.text.contains("hello") && l.level == LogLevel::Info && l.stream == LogStream::Stdout)
        );
        assert!(
            lines
                .iter()
                .any(|l| l.text.contains("boom") && l.level == LogLevel::Error && l.stream == LogStream::Stderr)
        );
        let written = std::fs::read_to_string(log_file).unwrap();
        assert!(written.contains("hello") && written.contains("boom"));
    }

    #[tokio::test]
    async fn kill_terminates_a_running_game() {
        let dir = tempfile::tempdir().unwrap();
        let (sink, _) = collector();
        let kill = CancellationToken::new();
        let script = if cfg!(windows) {
            "ping -n 30 127.0.0.1 >NUL"
        } else {
            "sleep 30"
        };
        let game = spawn(&shell(script, dir.path()), &dir.path().join("logs"), sink, kill.clone()).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        kill.cancel();
        let exit = tokio::time::timeout(Duration::from_secs(10), game.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(exit.killed);
    }

    #[tokio::test]
    async fn reports_a_missing_executable() {
        let dir = tempfile::tempdir().unwrap();
        let (sink, _) = collector();
        let command = LaunchCommand::new(dir.path().join("no-java.exe"), Vec::new(), dir.path().to_path_buf());
        let error = spawn(&command, &dir.path().join("logs"), sink, CancellationToken::new()).unwrap_err();
        assert_eq!(error.code(), "launch_failed");
    }

    #[test]
    fn prunes_old_logs() {
        let dir = tempfile::tempdir().unwrap();
        for day in 1..=5 {
            std::fs::write(dir.path().join(format!("game-2026-01-0{day}T00-00-00.000Z.log")), "").unwrap();
        }
        std::fs::write(dir.path().join("notes.txt"), "").unwrap();
        prune_logs(dir.path(), 2);
        let mut names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "game-2026-01-04T00-00-00.000Z.log",
                "game-2026-01-05T00-00-00.000Z.log",
                "notes.txt"
            ]
        );
    }
}
