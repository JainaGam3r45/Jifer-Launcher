import { useId } from "react";
import type { Instance } from "../../../shared/types/instance";
import { loaderLabel } from "../../format";
import type { LauncherCopy } from "../../i18n";
import styles from "./instancecard.module.css";

interface InstanceCardProps {
  instance: Instance;
  active: boolean;
  running: boolean;
  copy: LauncherCopy["instances"];
  removeBlockedReason: string | null;
  onSelect: () => void;
  onSettings: () => void;
  onClone: () => void;
  onRemove: () => void;
}

export function InstanceCard({ instance, active, running, copy, removeBlockedReason, onSelect, onSettings, onClone, onRemove }: InstanceCardProps) {
  const reasonId = useId();
  const text = copy.drawer;

  return (
    <article className={styles.card} data-active={active || undefined}>
      <header className={styles.header}>
        <h3 className={styles.name}>{instance.name}</h3>
        <div className={styles.badges}>
          {running ? <span className={styles.running}>{copy.state.running}</span> : null}
          {active ? <span className={styles.active}>{text.active}</span> : null}
        </div>
      </header>
      <p className={styles.meta}>
        <span>{instance.versionId || copy.hero.noVersion}</span>
        <span className={styles.loader}>{loaderLabel(instance.loaderType)}</span>
      </p>
      <div className={styles.actions}>
        {active ? null : (
          <button type="button" className={styles.primary} onClick={onSelect}>
            {text.select}
          </button>
        )}
        <button type="button" onClick={onSettings}>
          {text.settings}
        </button>
        <button type="button" onClick={onClone}>
          {text.clone}
        </button>
        <button
          type="button"
          className={styles.danger}
          aria-disabled={removeBlockedReason !== null}
          aria-describedby={removeBlockedReason ? reasonId : undefined}
          title={removeBlockedReason ?? undefined}
          onClick={removeBlockedReason ? undefined : onRemove}
        >
          {text.remove}
        </button>
      </div>
      {removeBlockedReason ? (
        <span id={reasonId} className={styles.srOnly}>
          {removeBlockedReason}
        </span>
      ) : null}
    </article>
  );
}
