import { useCallback, useDeferredValue, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { LauncherLogEntry } from "../../../shared/types/events";
import { launcher } from "../../bridge";
import type { LauncherCopy, LauncherLanguage } from "../../i18n";
import { countByLevel, filterEntries, formatEntry, formatForExport, visibleRange, type LevelFilter, type SourceFilter } from "../../store/consolefilter";
import { logStore, useLogEntries } from "../../store/logstore";
import { CopyIcon, ExportIcon, FolderIcon, TrashIcon } from "../icons";
import styles from "./diagnosticsconsole.module.css";

const ROW_HEIGHT = 22;
const levels: readonly LevelFilter[] = ["all", "debug", "info", "warn", "error"];

interface DiagnosticsConsoleProps {
  language: LauncherLanguage;
  copy: LauncherCopy;
  onOpenDataFolder: () => void;
}

export function DiagnosticsConsole({ language, copy, onOpenDataFolder }: DiagnosticsConsoleProps) {
  const text = copy.console;
  const entries = useLogEntries();
  const [level, setLevel] = useState<LevelFilter>("all");
  const [source, setSource] = useState<SourceFilter>("all");
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query);
  const [follow, setFollow] = useState(true);
  const [selected, setSelected] = useState<LauncherLogEntry | null>(null);
  const [copied, setCopied] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);

  const visible = useMemo(() => filterEntries(entries, { level, source, query: deferredQuery }), [entries, level, source, deferredQuery]);
  const counts = useMemo(() => countByLevel(entries), [entries]);

  const copyVisible = useCallback(async () => {
    await navigator.clipboard.writeText(formatForExport(visible));
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }, [visible]);

  const exportVisible = useCallback(async () => {
    setExportError(null);
    try {
      await launcher.logs.export(formatForExport(visible));
    } catch (error) {
      setExportError(typeof error === "object" && error && "message" in error ? String(error.message) : String(error));
    }
  }, [visible]);

  return (
    <section className={styles.console} aria-label={text.title}>
      <header className={styles.header}>
        <div>
          <p className="eyebrow">{text.eyebrow}</p>
          <h2>{text.title}</h2>
        </div>
        <div className={styles.tools}>
          <button type="button" className={styles.tool} onClick={() => void copyVisible()} disabled={visible.length === 0}>
            <CopyIcon />
            {copied ? text.copied : text.copy}
          </button>
          <button type="button" className={styles.tool} onClick={() => void exportVisible()} disabled={visible.length === 0}>
            <ExportIcon />
            {text.export}
          </button>
          <button type="button" className={styles.tool} onClick={onOpenDataFolder}>
            <FolderIcon />
            {text.openFolder}
          </button>
          <button
            type="button"
            className={`${styles.tool} ${styles.danger}`}
            onClick={() => {
              logStore.clear();
              setSelected(null);
            }}
            disabled={entries.length === 0}
          >
            <TrashIcon />
            {text.clear}
          </button>
        </div>
      </header>

      <div className={styles.filters}>
        <div className={styles.levels} role="radiogroup" aria-label={text.levels.all}>
          {levels.map((option) => (
            <button
              key={option}
              type="button"
              role="radio"
              aria-checked={level === option}
              data-level={option}
              className={styles.chip}
              onClick={() => setLevel(option)}
            >
              {text.levels[option]}
              {option !== "all" ? <span className={styles.count}>{counts[option].toLocaleString(language)}</span> : null}
            </button>
          ))}
        </div>
        <select className={styles.select} value={source} aria-label={text.sources.all} onChange={(event) => setSource(event.target.value as SourceFilter)}>
          <option value="all">{text.sources.all}</option>
          <option value="game">{text.sources.game}</option>
          <option value="launcher">{text.sources.launcher}</option>
        </select>
        <input className={styles.search} type="search" value={query} placeholder={text.search} aria-label={text.search} spellCheck={false} onChange={(event) => setQuery(event.target.value)} />
        <label className={styles.follow}>
          <input type="checkbox" checked={follow} onChange={(event) => setFollow(event.target.checked)} />
          {text.follow}
        </label>
      </div>

      <VirtualLog
        entries={visible}
        follow={follow}
        selectedId={selected?.id ?? null}
        emptyText={entries.length === 0 ? text.empty : text.noMatches}
        onUserScroll={(atBottom) => setFollow(atBottom)}
        onSelect={setSelected}
      />

      <footer className={styles.footer}>
        <span>{text.count(visible.length, entries.length)}</span>
        {exportError ? <span className={styles.error} role="alert">{exportError}</span> : null}
      </footer>

      {selected ? (
        <pre className={styles.detail} data-selectable aria-live="polite">
          {formatEntry(selected)}
        </pre>
      ) : null}
    </section>
  );
}

interface VirtualLogProps {
  entries: readonly LauncherLogEntry[];
  follow: boolean;
  selectedId: number | null;
  emptyText: string;
  onUserScroll: (atBottom: boolean) => void;
  onSelect: (entry: LauncherLogEntry) => void;
}

/** Fixed-row virtual list: only the rows in view (plus overscan) exist in the DOM. */
function VirtualLog({ entries, follow, selectedId, emptyText, onUserScroll, onSelect }: VirtualLogProps) {
  const viewport = useRef<HTMLDivElement>(null);
  const programmaticScroll = useRef(false);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(0);

  useEffect(() => {
    const element = viewport.current;
    if (!element) {
      return;
    }
    const observer = new ResizeObserver(() => setHeight(element.clientHeight));
    observer.observe(element);
    setHeight(element.clientHeight);
    return () => observer.disconnect();
  }, []);

  useLayoutEffect(() => {
    const element = viewport.current;
    if (follow && element) {
      programmaticScroll.current = true;
      element.scrollTop = element.scrollHeight;
      setScrollTop(element.scrollTop);
    }
  }, [entries, follow, height]);

  const { first, last } = visibleRange(scrollTop, height, ROW_HEIGHT, entries.length);
  const rows = entries.slice(first, last);

  return (
    <div
      ref={viewport}
      className={styles.viewport}
      tabIndex={0}
      role="log"
      aria-live="off"
      onScroll={(event) => {
        const element = event.currentTarget;
        setScrollTop(element.scrollTop);
        if (programmaticScroll.current) {
          programmaticScroll.current = false;
          return;
        }
        onUserScroll(element.scrollHeight - element.scrollTop - element.clientHeight < ROW_HEIGHT);
      }}
    >
      {entries.length === 0 ? (
        <p className={styles.empty}>{emptyText}</p>
      ) : (
        <div className={styles.canvas} style={{ height: entries.length * ROW_HEIGHT }}>
          {rows.map((entry, index) => (
            <button
              key={entry.id}
              type="button"
              className={styles.row}
              data-level={entry.level}
              aria-pressed={entry.id === selectedId}
              style={{ transform: `translateY(${(first + index) * ROW_HEIGHT}px)` }}
              title={entry.message}
              onClick={() => onSelect(entry)}
            >
              <time>{entry.timestamp.slice(11, 23)}</time>
              <span className={styles.level}>{entry.level}</span>
              <span className={styles.source} data-source={entry.source}>
                {entry.source === "game" ? "MC" : "JC"}
              </span>
              <span className={styles.message}>{entry.message}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
