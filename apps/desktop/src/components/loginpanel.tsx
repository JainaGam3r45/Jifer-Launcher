import { FormEvent } from "react";
import type { LauncherSession, MicrosoftDeviceCode } from "../../shared/types/auth";
import type { LauncherCopy } from "../i18n";
import { useFormField } from "../hooks/useformfield";

type LoginCopy = LauncherCopy["login"];

interface LoginPanelProps {
  session: LauncherSession | null;
  microsoftCode: MicrosoftDeviceCode | null;
  authLoading: boolean;
  microsoftEnabled: boolean;
  lastOfflineUsername: string;
  copy: LoginCopy;
  onMicrosoftLogin: () => Promise<void>;
  onCompleteMicrosoftLogin: () => Promise<void>;
  onLogin: (username: string) => Promise<void>;
  onLogout: () => Promise<void>;
}

export function LoginPanel({
  session,
  microsoftCode,
  authLoading,
  microsoftEnabled,
  lastOfflineUsername,
  copy,
  onMicrosoftLogin,
  onCompleteMicrosoftLogin,
  onLogin,
  onLogout
}: LoginPanelProps) {
  const username = useFormField(session?.username ?? lastOfflineUsername);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await onLogin(username.value);
  }

  if (session) {
    return (
      <section className="panel">
        <div className="panel-heading">
          <p className="eyebrow">{copy.session}</p>
          <h2>{session.username}</h2>
        </div>
        <p className="muted">{session.mode === "microsoft" ? copy.premiumSession : copy.offlineSession}</p>
        <button className="secondary" type="button" onClick={() => void onLogout()}>
          {copy.signOut}
        </button>
      </section>
    );
  }

  return (
    <section className="auth-card">
      {microsoftEnabled ? (
        <>
          <button className="signin-button" type="button" onClick={() => void onMicrosoftLogin()} disabled={authLoading}>
            {copy.microsoftSignIn}
            <span aria-hidden="true">-&gt;</span>
          </button>
          {microsoftCode ? (
            <div className="device-code">
              <p>{copy.deviceCodePrompt}</p>
              <strong>{microsoftCode.userCode}</strong>
              <button className="secondary" type="button" onClick={() => void onCompleteMicrosoftLogin()} disabled={authLoading}>
                {copy.finishedSignIn}
              </button>
            </div>
          ) : (
            <p className="auth-copy">{copy.microsoftHelp}</p>
          )}
        </>
      ) : (
        <div className="panel-heading">
          <p className="eyebrow">{copy.session}</p>
          <h2>{copy.offlineTitle}</h2>
          <p className="auth-copy">{copy.offlineHelp}</p>
        </div>
      )}

      <form className="offline-form" onSubmit={handleSubmit}>
        <label>
          {copy.offlineUsername}
          <input {...username.bind} placeholder={copy.playerNamePlaceholder} autoComplete="username" />
        </label>
        <button className="secondary" type="submit" disabled={authLoading}>
          {copy.continueOffline}
        </button>
      </form>
    </section>
  );
}
