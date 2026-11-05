import { useEffect, useRef, useState, type ReactElement } from "react";
import type { LauncherSession } from "../../../shared/types/auth";
import type { LauncherCopy, LauncherLanguage } from "../../i18n";
import { CommunityIcon, DonateIcon, InfoIcon, LanguageIcon, LayersIcon, PlayIcon, SettingsIcon, TerminalIcon } from "../icons";
import styles from "./siderail.module.css";

export type ViewId = "play" | "console" | "settings" | "about";

interface SideRailProps {
  view: ViewId;
  session: LauncherSession;
  offlineAccounts: string[];
  language: LauncherLanguage;
  consoleAlert: boolean;
  copy: LauncherCopy;
  onNavigate: (view: ViewId) => void;
  onOpenInstances: () => void;
  onSwitchAccount: (username: string) => void;
  onForgetAccount: (username: string) => void;
  onLogout: () => void;
  onLanguage: (language: LauncherLanguage) => void;
  onOpenLink: (url: string) => void;
}

interface NavItem {
  id: ViewId | "instances";
  label: string;
  icon: ReactElement;
}

export function SideRail(props: SideRailProps) {
  const { view, session, copy, consoleAlert, onNavigate, onOpenInstances } = props;
  const items: NavItem[] = [
    { id: "play", label: copy.shell.play, icon: <PlayIcon /> },
    { id: "instances", label: copy.shell.instances, icon: <LayersIcon /> },
    { id: "console", label: copy.shell.console, icon: <TerminalIcon /> },
    { id: "settings", label: copy.settings, icon: <SettingsIcon /> },
    { id: "about", label: copy.aboutTitle, icon: <InfoIcon /> }
  ];

  return (
    <nav className={styles.rail} aria-label={copy.shell.navigation}>
      <ul className={styles.items}>
        {items.map((item) => {
          const current = item.id === view;
          return (
            <li key={item.id}>
              <button
                type="button"
                className={styles.item}
                aria-current={current ? "page" : undefined}
                aria-label={item.label}
                data-tooltip={item.label}
                onClick={() => (item.id === "instances" ? onOpenInstances() : onNavigate(item.id))}
              >
                {item.icon}
                {item.id === "console" && consoleAlert && view !== "console" ? <span className={styles.alert} aria-hidden="true" /> : null}
              </button>
            </li>
          );
        })}
      </ul>

      <div className={styles.footer}>
        <RailLinks {...props} />
        <AccountButton {...props} session={session} />
      </div>
    </nav>
  );
}

function RailLinks({ language, copy, onLanguage, onOpenLink }: SideRailProps) {
  return (
    <>
      <button
        type="button"
        className={styles.item}
        aria-label={`${copy.shell.language}: ${language.toUpperCase()}`}
        data-tooltip={copy.shell.language}
        onClick={() => onLanguage(language === "es" ? "en" : "es")}
      >
        <LanguageIcon />
        <span className={styles.badge}>{language}</span>
      </button>
      <button type="button" className={styles.item} aria-label={copy.shell.community} data-tooltip={copy.shell.community} onClick={() => onOpenLink("https://discord.jifercraft.com")}>
        <CommunityIcon />
      </button>
      <button type="button" className={styles.item} aria-label={copy.shell.donate} data-tooltip={copy.shell.donate} onClick={() => onOpenLink("https://paypal.me/jifercloud")}>
        <DonateIcon />
      </button>
    </>
  );
}

function AccountButton({ session, offlineAccounts, copy, onSwitchAccount, onForgetAccount, onLogout }: SideRailProps) {
  const [open, setOpen] = useState(false);
  const container = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }

    const close = (event: MouseEvent | KeyboardEvent) => {
      if (event instanceof KeyboardEvent ? event.key === "Escape" : !container.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", close);
    };
  }, [open]);

  const others = offlineAccounts.filter((account) => account !== session.username);

  return (
    <div className={styles.account} ref={container}>
      <button type="button" className={styles.avatar} aria-expanded={open} aria-haspopup="menu" aria-label={session.username} onClick={() => setOpen((value) => !value)}>
        <img src={avatarUrl(session.username, 64)} alt="" />
        <span className={styles.mode} data-mode={session.mode} aria-hidden="true" />
      </button>
      {open ? (
        <section className={styles.popover} role="menu">
          <header>
            <img src={avatarUrl(session.username, 64)} alt="" />
            <div>
              <strong>{session.username}</strong>
              <span>{session.mode === "microsoft" ? copy.accountStatusPremium : copy.accountStatusOffline}</span>
            </div>
          </header>
          {others.length > 0 ? <p className={styles.label}>{copy.accounts}</p> : null}
          {others.map((account) => (
            <div key={account} className={styles.row}>
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setOpen(false);
                  onSwitchAccount(account);
                }}
              >
                <img src={avatarUrl(account, 32)} alt="" />
                {account}
              </button>
              <button type="button" className={styles.forget} aria-label={copy.forgetAccount(account)} onClick={() => onForgetAccount(account)}>
                ×
              </button>
            </div>
          ))}
          <button
            type="button"
            role="menuitem"
            className={styles.signOut}
            onClick={() => {
              setOpen(false);
              onLogout();
            }}
          >
            {copy.signOut}
          </button>
        </section>
      ) : null}
    </div>
  );
}

export function avatarUrl(username: string, size: number): string {
  return `https://mc-heads.net/avatar/${encodeURIComponent(username)}/${size}`;
}
