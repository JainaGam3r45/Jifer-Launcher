import type { LaunchProgress, LaunchStateEvent } from "../../../shared/types/events";
import type { LauncherCopy, LauncherLanguage } from "../../i18n";
import { downloadPercent, formatDuration, launchPhases, phaseStates, remainingSeconds } from "../../store/telemetry";
import { formatBytes, formatSpeed } from "../../store/updatestate";
import styles from "./launchtelemetry.module.css";

interface LaunchTelemetryProps {
  status: LaunchStateEvent;
  progress: LaunchProgress | null;
  language: LauncherLanguage;
  copy: LauncherCopy;
}

/** Live view of a launch being prepared: phase stepper plus the verified downloader's counters. */
export function LaunchTelemetry({ status, progress, language, copy }: LaunchTelemetryProps) {
  const text = copy.telemetry;
  const visible = status.state === "installing" || status.state === "launching";
  if (!visible) {
    return null;
  }

  // Progress events stop once the backend switches to "launching"; that state is the final phase.
  const phase = status.state === "launching" ? "starting" : (progress?.phase ?? null);
  const states = phaseStates(phase);
  const download = progress?.download;
  const percent = downloadPercent(download);
  const eta = remainingSeconds(download);
  const number = (value: number) => value.toLocaleString(language);

  return (
    <section className={styles.panel} aria-label={text.title} aria-busy="true">
      <header className={styles.header}>
        <div>
          <p className="eyebrow">{text.title}</p>
          <h3>{text.messages[phase ?? "metadata"]}</h3>
        </div>
        {percent !== null ? <strong className={styles.percent}>{Math.floor(percent)}%</strong> : null}
      </header>

      <ol className={styles.steps}>
        {launchPhases.map((step) => (
          <li key={step} data-state={states[step]}>
            <span className={styles.node} aria-hidden="true" />
            <span>{text.phases[step]}</span>
          </li>
        ))}
      </ol>

      <div
        className={styles.track}
        role="progressbar"
        aria-label={text.title}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent === null ? undefined : Math.round(percent)}
      >
        <span className={percent === null ? styles.indeterminate : styles.fill} style={percent === null ? undefined : { width: `${percent}%` }} />
      </div>

      {download ? (
        <dl className={styles.metrics}>
          <Metric label={text.speed} value={formatSpeed(download.bytesPerSecond, language)} hint={eta !== null && eta > 0 ? text.remaining(formatDuration(eta, language)) : undefined} />
          <Metric
            label={text.files}
            value={text.of(number(download.filesDone), number(download.filesTotal))}
            hint={download.bytesTotal > 0 ? `${formatBytes(download.bytesDone, language)} / ${formatBytes(download.bytesTotal, language)}` : undefined}
          />
          <Metric label={text.active} value={number(download.filesActive)} live={download.filesActive > 0} />
          <Metric label={text.verified} value={number(download.filesVerified)} hint={download.filesVerifying > 0 ? `+${number(download.filesVerifying)}` : undefined} accent />
          <Metric label={text.reused} value={number(download.filesReused)} />
          <Metric label={text.retries} value={number(download.retries)} warn={download.retries > 0} />
        </dl>
      ) : null}
    </section>
  );
}

interface MetricProps {
  label: string;
  value: string;
  hint?: string;
  live?: boolean;
  accent?: boolean;
  warn?: boolean;
}

function Metric({ label, value, hint, live, accent, warn }: MetricProps) {
  return (
    <div className={styles.metric} data-live={live || undefined} data-accent={accent || undefined} data-warn={warn || undefined}>
      <dt>{label}</dt>
      <dd>
        <span className={styles.value}>{value}</span>
        {hint ? <span className={styles.hint}>{hint}</span> : null}
      </dd>
    </div>
  );
}
