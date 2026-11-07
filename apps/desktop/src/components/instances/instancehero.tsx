import { useId } from "react";
import type { LauncherState } from "../../../shared/types/events";
import type { Instance } from "../../../shared/types/instance";
import { formatRelativeTime, loaderLabel } from "../../format";
import type { LauncherCopy, LauncherLanguage } from "../../i18n";
import { SettingsIcon } from "../icons";
import { InstanceSelector } from "./instanceselector";
import styles from "./instancehero.module.css";

type InstancesCopy = LauncherCopy["instances"];

interface InstanceHeroProps {
  instance: Instance | null;
  instances: Instance[];
  runningInstanceId: string | null;
  launchState: LauncherState;
  blocked: boolean;
  canPlay: boolean;
  language: LauncherLanguage;
  copy: InstancesCopy;
  now?: number;
  onPlay: () => void;
  onStop: () => void;
  onOpenSettings: () => void;
  onSelectInstance: (id: string) => void;
  onManageInstances: () => void;
}

export function InstanceHero({
  instance,
  instances,
  runningInstanceId,
  launchState,
  blocked,
  canPlay,
  language,
  copy,
  now = Date.now(),
  onPlay,
  onStop,
  onOpenSettings,
  onSelectInstance,
  onManageInstances
}: InstanceHeroProps) {
  const titleId = useId();
  const hintId = useId();

  if (!instance) {
    return (
      <section className={styles.hero} aria-labelledby={titleId}>
        <div className={styles.copy}>
          <h2 id={titleId} className={styles.title}>
            {copy.hero.emptyTitle}
          </h2>
          <p className={styles.meta}>{copy.hero.emptyText}</p>
        </div>
        <aside className={styles.actions}>
          <button type="button" className={styles.play} onClick={onManageInstances}>
            {copy.hero.emptyAction}
          </button>
        </aside>
      </section>
    );
  }

  const hasVersion = instance.versionId.trim() !== "";
  const running = launchState === "launching" || launchState === "running" || launchState === "installing";
  const stopping = launchState === "stopping";
  const lastPlayed = instance.lastPlayedAt ? formatRelativeTime(instance.lastPlayedAt, now, language) : "";
  const statusLabel = resolveStatusLabel(launchState, blocked, copy);
  const hint = !hasVersion ? copy.hero.noVersionHint : blocked ? copy.state.blocked : null;
  const playDisabled = running ? false : stopping || blocked || !canPlay || !hasVersion;

  return (
    <section className={styles.hero} aria-labelledby={titleId}>
      <div className={styles.copy}>
        <InstanceSelector
          instances={instances}
          activeId={instance.id}
          runningId={runningInstanceId}
          copy={copy.selector}
          runningLabel={copy.state.running}
          onSelect={onSelectInstance}
          onManage={onManageInstances}
        />
        <h2 id={titleId} className={styles.title}>
          {instance.name}
        </h2>
        <div className={styles.badges}>
          <span className={hasVersion ? styles.badgeAccent : styles.badgeWarning}>{hasVersion ? instance.versionId : copy.hero.noVersion}</span>
          <span className={styles.badge}>{loaderLabel(instance.loaderType)}</span>
        </div>
        <p className={styles.meta}>
          {lastPlayed ? copy.hero.lastPlayed(lastPlayed) : copy.hero.neverPlayed}
        </p>
      </div>
      <aside className={styles.actions}>
        {statusLabel ? (
          <span className={styles.status} data-tone={blocked ? "warning" : launchState}>
            {statusLabel}
          </span>
        ) : null}
        <div className={styles.buttons}>
          <button
            type="button"
            className={styles.play}
            data-mode={running ? "stop" : "play"}
            disabled={playDisabled}
            aria-describedby={hint ? hintId : undefined}
            onClick={running ? onStop : onPlay}
          >
            {running ? copy.hero.stop : copy.hero.play}
          </button>
          <button type="button" className={styles.settings} aria-label={copy.hero.openSettings} title={copy.hero.openSettings} onClick={onOpenSettings}>
            <SettingsIcon />
          </button>
        </div>
        {hint ? (
          <p id={hintId} className={styles.hint}>
            {hint}
          </p>
        ) : null}
      </aside>
    </section>
  );
}

function resolveStatusLabel(state: LauncherState, blocked: boolean, copy: InstancesCopy): string | null {
  if (blocked) {
    return copy.state.blocked;
  }

  switch (state) {
    case "running":
      return copy.state.playingNow;
    case "launching":
      return copy.state.launching;
    case "installing":
      return copy.state.installing;
    case "stopping":
      return copy.state.stopping;
    default:
      return null;
  }
}
