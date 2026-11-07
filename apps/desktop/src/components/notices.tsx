import { useState } from "react";
import type { AppBootstrap } from "../../shared/types/api";
import type { LauncherCopy } from "../i18n";

interface NoticesProps {
  bootstrap: AppBootstrap | null;
  copy: LauncherCopy;
}

/** One-off startup explanations (forced re-login, migrated data). Dismissed per session. */
export function Notices({ bootstrap, copy }: NoticesProps) {
  const [dismissed, setDismissed] = useState<string[]>([]);
  const notices = bootstrap?.notices;
  const messages: [string, string][] = [];

  if (notices?.account) {
    messages.push([notices.account, copy.notices[notices.account]]);
  }

  if (notices?.migratedDefaultInstance) {
    messages.push(["migratedDefaultInstance", copy.notices.migratedDefaultInstance]);
  }

  if (notices?.onedriveDetected) {
    messages.push(["onedriveDetected", copy.notices.onedriveDetected]);
  }

  const visible = messages.filter(([key]) => !dismissed.includes(key));

  if (visible.length === 0) {
    return null;
  }

  return (
    <div className="notices" role="status">
      {visible.map(([key, message]) => (
        <p key={key} className="notice">
          <span>{message}</span>
          <button type="button" aria-label={copy.notices.dismiss} onClick={() => setDismissed((current) => [...current, key])}>
            ×
          </button>
        </p>
      ))}
    </div>
  );
}
