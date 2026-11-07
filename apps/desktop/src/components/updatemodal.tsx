import { useId } from "react";
import type { UpdateStatus } from "../../shared/types/updates";
import type { LauncherCopy, LauncherLanguage } from "../i18n";
import { describeUpdateState, formatReleaseDate, resolvePercent, resolveProgressSummary } from "../store/updatestate";
import { Modal } from "./modal";
import styles from "./updatemodal.module.css";

interface UpdateModalProps {
  open: boolean;
  status: UpdateStatus;
  gameRunning: boolean;
  language: LauncherLanguage;
  copy: LauncherCopy;
  onDownload: () => void;
  onInstall: () => void;
  onRetry: () => void;
  onClose: () => void;
}

export function UpdateModal({ open, status, gameRunning, language, copy, onDownload, onInstall, onRetry, onClose }: UpdateModalProps) {
  const text = copy.updates;
  const notesId = useId();
  const blockedId = useId();
  const releaseDate = formatReleaseDate(status.info?.releaseDate, language);
  const percent = resolvePercent(status.progress);
  const stateText = describeUpdateState(status, text.state);

  return (
    <Modal title={text.modal.title} closeLabel={copy.closeModal} open={open} onClose={onClose}>
      <div className={styles.content}>
        <dl className={styles.facts}>
          <div>
            <dt>{text.modal.installedVersion}</dt>
            <dd>{text.versionLabel(status.currentVersion)}</dd>
          </div>
          <div>
            <dt>{text.modal.newVersion}</dt>
            <dd>{status.info ? text.versionLabel(status.info.version) : "-"}</dd>
          </div>
          {releaseDate ? (
            <div>
              <dt>{text.modal.releaseDate}</dt>
              <dd>{releaseDate}</dd>
            </div>
          ) : null}
        </dl>

        {status.info ? (
          <section className={styles.notesSection}>
            <h3 id={notesId} className={styles.notesTitle}>
              {text.modal.notes}
            </h3>
            <div className={styles.notes} role="region" aria-labelledby={notesId} tabIndex={0}>
              {status.info.notes ?? text.modal.notesEmpty}
            </div>
          </section>
        ) : null}

        <p className={styles.state} aria-live="polite">
          {stateText}
        </p>

        {status.state === "downloading" ? (
          <div className={styles.progress}>
            <div
              className={styles.track}
              role="progressbar"
              aria-label={text.modal.progressLabel}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={percent}
              aria-valuetext={resolveProgressSummary(status.progress, language)}
            >
              <span className={styles.fill} style={{ width: `${percent}%` }} />
            </div>
            <p className={styles.summary}>{resolveProgressSummary(status.progress, language)}</p>
          </div>
        ) : null}

        {status.state === "error" && status.error ? (
          <p className={styles.error} role="alert">
            {status.error}
          </p>
        ) : null}

        <div className={styles.actions}>
          {status.state === "available" ? (
            <button type="button" className={styles.primary} onClick={onDownload}>
              {text.modal.download}
            </button>
          ) : null}
          {status.state === "downloaded" ? (
            <button
              type="button"
              className={styles.primary}
              aria-disabled={gameRunning}
              aria-describedby={gameRunning ? blockedId : undefined}
              onClick={gameRunning ? undefined : onInstall}
            >
              {text.modal.install}
            </button>
          ) : null}
          {status.state === "error" ? (
            <button type="button" className={styles.primary} onClick={onRetry}>
              {text.modal.retry}
            </button>
          ) : null}
        </div>

        {status.state === "downloaded" && gameRunning ? (
          <p id={blockedId} className={styles.hint}>
            {text.modal.blockedRunning}
          </p>
        ) : null}
      </div>
    </Modal>
  );
}
