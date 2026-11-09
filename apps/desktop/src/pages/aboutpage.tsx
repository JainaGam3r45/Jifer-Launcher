import { appInfo } from "../../shared/constants/app";
import logo from "../../assets/logo.png";
import type { UpdateStatus } from "../../shared/types/updates";
import type { LauncherCopy } from "../i18n";
import { describeUpdateState } from "../store/updatestate";
import styles from "./aboutpage.module.css";

type AboutCopy = LauncherCopy["aboutPanel"];

interface AboutPageProps {
  copy: AboutCopy;
  updates: LauncherCopy["updates"];
  updateStatus: UpdateStatus;
  onCheckUpdates: () => void;
}

export function AboutPage({ copy, updates, updateStatus, onCheckUpdates }: AboutPageProps) {
  const checking = updateStatus.state === "checking" || updateStatus.state === "downloading";

  return (
    <section className="panel about-panel">
      <div className="panel-heading">
        <div className="about-title">
          <img className="about-logo" src={logo} alt="" />
          <div>
            <p className="eyebrow">{copy.eyebrow}</p>
            <h2>{appInfo.name}</h2>
          </div>
        </div>
      </div>
      <dl>
        <div>
          <dt>{copy.version}</dt>
          <dd>{appInfo.version}</dd>
        </div>
        <div>
          <dt>{copy.author}</dt>
          <dd>{appInfo.author}</dd>
        </div>
        <div>
          <dt>{copy.organization}</dt>
          <dd>{appInfo.organization}</dd>
        </div>
        <div>
          <dt>{copy.website}</dt>
          <dd>
            <a href={appInfo.website} target="_blank" rel="noreferrer">
              {appInfo.website}
            </a>
          </dd>
        </div>
        <div>
          <dt>{copy.license}</dt>
          <dd>{appInfo.license}</dd>
        </div>
      </dl>
      {updateStatus.supported ? (
        <div className={styles.update}>
          <button type="button" className={styles.check} disabled={checking} onClick={onCheckUpdates}>
            {updates.about.check}
          </button>
          <p className={styles.status} aria-live="polite">
            {describeUpdateState(updateStatus, updates.state)}
          </p>
        </div>
      ) : null}
      <p className="muted">{appInfo.copyright}</p>
    </section>
  );
}
