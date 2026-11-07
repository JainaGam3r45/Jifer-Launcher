import type { LauncherCopy } from "../i18n";
import { resolvePercent, shouldShowUpdateIndicator } from "../store/updatestate";
import type { UpdateStatus } from "../../shared/types/updates";
import { DownloadIcon } from "./icons";
import styles from "./updateindicator.module.css";

interface UpdateIndicatorProps {
  status: UpdateStatus;
  copy: LauncherCopy["updates"];
  onOpen: () => void;
}

export function UpdateIndicator({ status, copy, onOpen }: UpdateIndicatorProps) {
  if (!shouldShowUpdateIndicator(status)) {
    return null;
  }

  const percent = resolvePercent(status.progress);
  const version = status.info?.version ?? "";
  const label = resolveLabel(status, copy, percent, version);

  return (
    <button type="button" className={styles.indicator} data-state={status.state} aria-label={label} aria-haspopup="dialog" title={label} onClick={onOpen}>
      <DownloadIcon />
      {status.state === "downloading" ? <span className={styles.pill}>{percent}%</span> : null}
      {status.state === "downloaded" ? <span className={styles.pill}>{copy.indicator.ready}</span> : null}
      {status.state === "available" || status.state === "error" ? <span className={styles.dot} aria-hidden="true" /> : null}
    </button>
  );
}

function resolveLabel(status: UpdateStatus, copy: LauncherCopy["updates"], percent: number, version: string): string {
  switch (status.state) {
    case "downloading":
      return copy.indicator.downloading(percent);
    case "downloaded":
      return copy.indicator.downloaded(version);
    case "error":
      return copy.indicator.error;
    default:
      return copy.indicator.available(version);
  }
}
