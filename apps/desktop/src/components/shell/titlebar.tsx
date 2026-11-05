import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState, type ReactNode } from "react";
import type { LaunchStateEvent } from "../../../shared/types/events";
import logo from "../../../assets/logo.png";
import type { LauncherCopy } from "../../i18n";
import { CloseIcon, MaximizeIcon, MinimizeIcon, RestoreIcon } from "../icons";
import styles from "./titlebar.module.css";

interface TitleBarProps {
  instanceName: string | null;
  versionId: string | null;
  status: LaunchStateEvent;
  copy: LauncherCopy;
  /** Right-side slot for contextual actions such as the update indicator. */
  actions?: ReactNode;
}

/**
 * Frameless window chrome. The whole bar is a drag region (double-click toggles maximize); interactive
 * children opt out automatically because they are buttons.
 */
export function TitleBar({ instanceName, versionId, status, copy, actions }: TitleBarProps) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    let stop: (() => void) | null = null;
    let disposed = false;
    const sync = () => void appWindow.isMaximized().then(setMaximized).catch(() => undefined);

    sync();
    void appWindow.onResized(sync).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        stop = unlisten;
      }
    });

    return () => {
      disposed = true;
      stop?.();
    };
  }, []);

  const appWindow = () => getCurrentWindow();
  const busy = status.state !== "idle";

  return (
    <header className={styles.bar} data-tauri-drag-region>
      <div className={styles.brand} data-tauri-drag-region>
        <img className={styles.mark} src={logo} alt="" />
        <span className={styles.wordmark} data-tauri-drag-region>
          Jifer<strong>Craft</strong>
        </span>
      </div>

      <nav className={styles.crumbs} aria-label="breadcrumb" data-tauri-drag-region>
        <span data-tauri-drag-region>{instanceName ?? copy.shell.noInstance}</span>
        {versionId ? (
          <span className={styles.version} data-tauri-drag-region>
            {versionId}
          </span>
        ) : null}
      </nav>

      <div className={styles.status} data-state={status.state} role="status" aria-live="polite" data-tauri-drag-region>
        <span className={styles.dot} aria-hidden="true" />
        <span data-tauri-drag-region>{copy.status.label[status.state]}</span>
        {busy ? <span className="visually-hidden">{status.message}</span> : null}
      </div>

      <div className={styles.actions}>{actions}</div>

      <div className={styles.controls}>
        <button type="button" className={styles.control} aria-label={copy.shell.minimize} title={copy.shell.minimize} onClick={() => void appWindow().minimize()}>
          <MinimizeIcon />
        </button>
        <button
          type="button"
          className={styles.control}
          aria-label={maximized ? copy.shell.restore : copy.shell.maximize}
          title={maximized ? copy.shell.restore : copy.shell.maximize}
          onClick={() => void appWindow().toggleMaximize()}
        >
          {maximized ? <RestoreIcon /> : <MaximizeIcon />}
        </button>
        <button type="button" className={`${styles.control} ${styles.close}`} aria-label={copy.shell.close} title={copy.shell.close} onClick={() => void appWindow().close()}>
          <CloseIcon />
        </button>
      </div>
    </header>
  );
}
