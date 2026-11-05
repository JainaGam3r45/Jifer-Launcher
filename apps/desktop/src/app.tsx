import type { CSSProperties, ReactNode } from "react";
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import coverOne from "../assets/coverone.png";
import coverThree from "../assets/coverthree.png";
import coverTwo from "../assets/covertwo.png";
import logo from "../assets/logo.png";
import { appInfo } from "../shared/constants/app";
import { DiagnosticsConsole } from "./components/console/diagnosticsconsole";
import { FolderIcon } from "./components/icons";
import { InstanceDrawer } from "./components/instances/instancedrawer";
import { InstanceHero } from "./components/instances/instancehero";
import { InstanceSettingsModal } from "./components/instances/instancesettingsmodal";
import { LoginPanel } from "./components/loginpanel";
import { Notices } from "./components/notices";
import { SettingsPanel } from "./components/settingspanel";
import { SideRail, type ViewId } from "./components/shell/siderail";
import { TitleBar } from "./components/shell/titlebar";
import { LaunchTelemetry } from "./components/telemetry/launchtelemetry";
import { UpdateIndicator } from "./components/updateindicator";
import { UpdateModal } from "./components/updatemodal";
import { launcherCopy, type LauncherCopy } from "./i18n";
import { AboutPage } from "./pages/aboutpage";
import { resolveInstanceLaunchView } from "./store/instancestate";
import { useAppState } from "./store/useappstate";
import { withViewTransition } from "./store/viewtransition";

const shellStyle = {
  "--cover-one": `url(${coverOne})`,
  "--cover-two": `url(${coverTwo})`,
  "--cover-three": `url(${coverThree})`
} as CSSProperties;

export function App() {
  const launcher = useAppState();
  const [view, setView] = useState<ViewId>("play");
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [updateOpen, setUpdateOpen] = useState(false);
  const [instanceSettingsId, setInstanceSettingsId] = useState<string | null>(null);
  const language = launcher.settings?.ui.language ?? "es";
  const copy = launcherCopy[language];

  // Keep the window hidden until first paint so we don't flash an ugly unstyled frame.
  // Show ourselves now that the DOM is ready and styled.
  useEffect(() => {
    void getCurrentWindow().show();
  }, []);

  const titleBar = (
    <TitleBar
      instanceName={launcher.session ? (launcher.activeInstance?.name ?? null) : null}
      versionId={launcher.session ? (launcher.activeInstance?.versionId || null) : null}
      status={launcher.status}
      copy={copy}
      actions={<UpdateIndicator status={launcher.updateStatus} copy={copy.updates} onOpen={() => setUpdateOpen(true)} />}
    />
  );

  if (launcher.loading) {
    return (
      <Frame titleBar={titleBar}>
        <main className="loading-screen">
          <div className="loader-card">
            <img className="brand-mark large" src={logo} alt="" />
            <p className="eyebrow">JiferCraft Studios</p>
            <h1>{appInfo.name}</h1>
            <div className="loader-line" />
          </div>
        </main>
      </Frame>
    );
  }

  if (!launcher.settings) {
    return (
      <Frame titleBar={titleBar}>
        <main className="loading-screen">
          <div className="loader-card">
            <img className="brand-mark large" src={logo} alt="" />
            <p className="eyebrow">JiferCraft Studios</p>
            <h1>{copy.startupFailedTitle}</h1>
            <p style={{ marginTop: "0.5rem", color: "var(--color-text-secondary, #9ca3af)", fontSize: "0.875rem" }}>
              {launcher.error || copy.startupFailedMessage}
            </p>
            <div style={{ display: "flex", gap: "0.75rem", justifyContent: "center", marginTop: "1.25rem" }}>
              <button type="button" className="action-button primary" onClick={() => void launcher.refresh()}>
                {copy.retry}
              </button>
              <button type="button" className="action-button secondary" onClick={() => void launcher.openDataFolder()}>
                {copy.console.openFolder}
              </button>
            </div>
          </div>
        </main>
      </Frame>
    );
  }

  if (!launcher.session) {
    return (
      <Frame titleBar={titleBar}>
        <main className="auth-shell">
          <section className="auth-brand">
            <div className="auth-message">
              <p className="eyebrow">{copy.ready}</p>
              <h2>{copy.login.offlineTitle}</h2>
              <p>{copy.login.offlineHelp}</p>
            </div>
          </section>
          <section className="auth-panel-wrap">
            <Notices bootstrap={launcher.bootstrap} copy={copy} />
            {launcher.error ? (
              <aside className="toast-error">
                <strong>{copy.signInFailed}</strong>
                <span>{launcher.error}</span>
              </aside>
            ) : null}
            <LoginPanel
              session={launcher.session}
              microsoftCode={launcher.microsoftCode}
              authLoading={launcher.authLoading}
              microsoftEnabled={launcher.bootstrap?.microsoftAvailable ?? false}
              lastOfflineUsername={launcher.settings.user.lastOfflineUsername}
              copy={copy.login}
              onMicrosoftLogin={launcher.startMicrosoftLogin}
              onCompleteMicrosoftLogin={launcher.completeMicrosoftLogin}
              onLogin={launcher.login}
              onLogout={launcher.logout}
            />
            <p className="release-label">{copy.releaseLabel(launcher.bootstrap?.version ?? appInfo.version)}</p>
          </section>
        </main>
      </Frame>
    );
  }

  const launchView = resolveInstanceLaunchView(launcher.status, launcher.activeInstance?.id ?? null);
  const settingsInstance = launcher.instances.find((instance) => instance.id === instanceSettingsId) ?? null;
  const navigate = (next: ViewId) => {
    if (next !== view) {
      withViewTransition(() => setView(next));
    }
  };
  const openInstanceSettings = (id: string) => {
    launcher.dismissError();
    setInstanceSettingsId(id);
  };

  return (
    <Frame titleBar={titleBar}>
      <div className="workspace">
        <SideRail
          view={view}
          session={launcher.session}
          offlineAccounts={launcher.settings.user.offlineAccounts}
          language={language}
          consoleAlert={launcher.status.state === "error"}
          copy={copy}
          onNavigate={navigate}
          onOpenInstances={() => {
            launcher.dismissError();
            setDrawerOpen(true);
          }}
          onSwitchAccount={(username) => void launcher.switchOfflineAccount(username)}
          onForgetAccount={(username) => void launcher.forgetOfflineAccount(username)}
          onLogout={() => void launcher.logout()}
          onLanguage={(next) => void launcher.updateSettings({ ui: { language: next } })}
          onOpenLink={(url) => void launcher.openExternal(url)}
        />

        <main className="stage" data-view={view}>
          {view === "play" ? (
            <div className="view view-play">
              <Notices bootstrap={launcher.bootstrap} copy={copy} />
              {launcher.error ? (
                <aside className="toast-error inline" role="alert">
                  <span>{launcher.error}</span>
                  <button type="button" aria-label={copy.notices.dismiss} onClick={launcher.dismissError}>
                    ×
                  </button>
                </aside>
              ) : null}
              <div className="hero-slot" key={launcher.activeInstance?.id ?? "none"}>
                <InstanceHero
                  instance={launcher.activeInstance}
                  instances={launcher.instances}
                  runningInstanceId={launchView.runningInstanceId}
                  launchState={launchView.state}
                  blocked={launchView.blocked}
                  canPlay={Boolean(launcher.session)}
                  language={language}
                  copy={copy.instances}
                  onPlay={() => void launcher.launch()}
                  onStop={() => void launcher.stop()}
                  onOpenSettings={() => launcher.activeInstance && openInstanceSettings(launcher.activeInstance.id)}
                  onSelectInstance={(id) => void launcher.setActiveInstance(id)}
                  onManageInstances={() => setDrawerOpen(true)}
                />
              </div>
              <LaunchTelemetry status={launcher.status} progress={launcher.progress} language={language} copy={copy} />
              <Spotlights copy={copy} />
            </div>
          ) : null}

          {view === "console" ? (
            <div className="view view-console">
              <DiagnosticsConsole language={language} copy={copy} onOpenDataFolder={() => void launcher.openDataFolder()} />
            </div>
          ) : null}

          {view === "settings" ? (
            <div className="view view-narrow">
              <SettingsPanel settings={launcher.settings} onUpdate={launcher.updateSettings} copy={copy.settingsPanel} />
              <button type="button" className="ghost-button" onClick={() => void launcher.openDataFolder()}>
                <FolderIcon />
                {copy.console.openFolder}
              </button>
            </div>
          ) : null}

          {view === "about" ? (
            <div className="view view-narrow">
              <AboutPage copy={copy.aboutPanel} updates={copy.updates} updateStatus={launcher.updateStatus} onCheckUpdates={() => void launcher.checkForUpdates()} />
            </div>
          ) : null}
        </main>
      </div>

      <InstanceDrawer
        open={drawerOpen}
        instances={launcher.instances}
        activeId={launcher.activeInstance?.id ?? null}
        runningId={launchView.runningInstanceId}
        versions={launcher.versions}
        copy={copy}
        error={launcher.error}
        onSelect={(id) => {
          setDrawerOpen(false);
          void launcher.setActiveInstance(id);
        }}
        onOpenSettings={(id) => {
          setDrawerOpen(false);
          openInstanceSettings(id);
        }}
        onCreate={async (input) => (await launcher.createInstance(input)) !== null}
        onClone={async (input) => (await launcher.cloneInstance(input)) !== null}
        onRemove={async (id) => (await launcher.removeInstance(id)) !== null}
        onDismissError={launcher.dismissError}
        onClose={() => setDrawerOpen(false)}
      />
      <InstanceSettingsModal
        open={settingsInstance !== null}
        instance={settingsInstance}
        versions={launcher.versions}
        globalWindow={launcher.settings.window}
        copy={copy}
        error={launcher.error}
        onSave={async (id, patch) => (await launcher.updateInstance(id, patch)) !== null}
        onClose={() => setInstanceSettingsId(null)}
      />
      <UpdateModal
        open={updateOpen}
        status={launcher.updateStatus}
        gameRunning={launcher.status.state === "running"}
        language={language}
        copy={copy}
        onDownload={() => void launcher.downloadUpdate()}
        onInstall={() => void launcher.installUpdate()}
        onRetry={() => void launcher.checkForUpdates()}
        onClose={() => setUpdateOpen(false)}
      />
    </Frame>
  );
}

function Frame({ titleBar, children }: { titleBar: ReactNode; children: ReactNode }) {
  return (
    <div className="app-frame" style={shellStyle}>
      {titleBar}
      {children}
    </div>
  );
}

function Spotlights({ copy }: { copy: LauncherCopy }) {
  return (
    <section className="content-grid">
      <article className="spotlight-card large">
        <img src={coverTwo} alt="" />
        <div>
          <p className="eyebrow">{copy.hosting}</p>
          <h2>{copy.worldsTitle}</h2>
          <p>{copy.worldsText}</p>
        </div>
      </article>
      <article className="spotlight-card">
        <img src={coverThree} alt="" />
        <div>
          <p className="eyebrow">{copy.discord}</p>
          <h2>{copy.discordTitle}</h2>
          <p>{copy.discordText}</p>
        </div>
      </article>
    </section>
  );
}
