import type { FormEvent } from "react";
import { useId, useState } from "react";
import type { CloneInstanceInput, CreateInstanceInput, Instance, LoaderType } from "../../../shared/types/instance";
import type { MinecraftVersion } from "../../../shared/types/version";
import { loaderLabel } from "../../format";
import type { LauncherCopy } from "../../i18n";
import { maxNameLength, validateInstanceName, validateNewInstance, type NewInstanceErrors } from "../../store/instancedraft";
import { VersionPicker } from "./versionpicker";
import styles from "./instanceform.module.css";

export type InstanceFormMode = { kind: "create" } | { kind: "clone"; source: Instance };

const loaderOptions: readonly { type: LoaderType; enabled: boolean }[] = [
  { type: "vanilla", enabled: true },
  { type: "fabric", enabled: false },
  { type: "forge", enabled: false },
  { type: "neoforge", enabled: false }
];

interface InstanceFormProps {
  mode: InstanceFormMode;
  versions: MinecraftVersion[];
  defaultVersionId: string;
  copy: LauncherCopy;
  error: string | null;
  onCreate: (input: CreateInstanceInput) => Promise<boolean>;
  onClone: (input: CloneInstanceInput) => Promise<boolean>;
  onDone: () => void;
  onBack: () => void;
}

export function InstanceForm({ mode, versions, defaultVersionId, copy, error, onCreate, onClone, onDone, onBack }: InstanceFormProps) {
  const text = copy.instances.drawer.form;
  const baseId = useId();
  const cloning = mode.kind === "clone";
  const [name, setName] = useState(mode.kind === "clone" ? `${mode.source.name}${text.cloneSuffix}`.slice(0, maxNameLength) : "");
  const [versionId, setVersionId] = useState(defaultVersionId);
  const [copyWorlds, setCopyWorlds] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [working, setWorking] = useState(false);

  const nameCode = validateInstanceName(name);
  const errors: NewInstanceErrors = cloning ? (nameCode ? { name: nameCode } : {}) : validateNewInstance({ name, versionId });
  const nameError = attempted && errors.name ? text.errors[errors.name] : null;
  const versionError = attempted && errors.versionId ? text.errors[errors.versionId] : null;

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    setAttempted(true);

    if (errors.name || errors.versionId) {
      return;
    }

    setWorking(true);
    const done =
      mode.kind === "clone"
        ? await onClone({ sourceId: mode.source.id, name: name.trim(), copyWorlds })
        : await onCreate({ name: name.trim(), versionId, loaderType: "vanilla" });
    setWorking(false);

    if (done) {
      onDone();
    }
  }

  return (
    <form className={styles.form} onSubmit={(event) => void handleSubmit(event)} noValidate>
      <button type="button" className={styles.back} onClick={onBack}>
        ← {text.back}
      </button>
      <h3 className={styles.title}>{cloning ? text.cloneTitle : text.createTitle}</h3>

      <div className={styles.field}>
        <label htmlFor={`${baseId}-name`}>
          {text.name}
          <input
            id={`${baseId}-name`}
            value={name}
            placeholder={text.namePlaceholder}
            aria-invalid={nameError ? true : undefined}
            aria-describedby={nameError ? `${baseId}-name-error` : undefined}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        {nameError ? (
          <p id={`${baseId}-name-error`} className={styles.error}>
            {nameError}
          </p>
        ) : null}
      </div>

      {cloning ? (
        <>
          <p className={styles.hint}>
            {mode.source.versionId || copy.instances.hero.noVersion} · {loaderLabel(mode.source.loaderType)}
          </p>
          <label className="checkbox">
            <input type="checkbox" checked={copyWorlds} onChange={(event) => setCopyWorlds(event.target.checked)} />
            {text.copyWorlds}
          </label>
          <p className={styles.hint}>{text.copyWorldsHint}</p>
        </>
      ) : (
        <>
          <div className={styles.field}>
            <VersionPicker
              versions={versions}
              value={versionId}
              label={text.version}
              placeholder={copy.instances.hero.noVersion}
              copy={copy}
              invalid={Boolean(versionError)}
              describedBy={versionError ? `${baseId}-version-error` : undefined}
              onChange={setVersionId}
            />
            {versionError ? (
              <p id={`${baseId}-version-error`} className={styles.error}>
                {versionError}
              </p>
            ) : null}
          </div>

          <fieldset className={styles.loaders}>
            <legend>{text.loader}</legend>
            {loaderOptions.map((option) => (
              <label key={option.type} className={styles.loader} data-disabled={!option.enabled || undefined}>
                <input type="radio" name={`${baseId}-loader`} value={option.type} checked={option.type === "vanilla"} disabled={!option.enabled} readOnly />
                <span>{loaderLabel(option.type)}</span>
                {option.enabled ? null : <span className={styles.soon}>{text.comingSoon}</span>}
              </label>
            ))}
          </fieldset>
        </>
      )}

      {error ? (
        <p className={styles.serverError} role="alert">
          {error}
        </p>
      ) : null}

      <button type="submit" className={styles.submit} disabled={working}>
        {working ? text.working : cloning ? text.clone : text.create}
      </button>
    </form>
  );
}
