import type { FormEvent, KeyboardEvent, ReactElement } from "react";
import { cloneElement, useId, useMemo, useState } from "react";
import type { Instance, UpdateInstanceInput, WindowConfig } from "../../../shared/types/instance";
import type { MinecraftVersion } from "../../../shared/types/version";
import type { LauncherCopy } from "../../i18n";
import {
  buildPatch,
  draftFromInstance,
  isDraftDirty,
  validateDraft,
  type DraftField,
  type InstanceDraft
} from "../../store/instancedraft";
import { Modal } from "../modal";
import { VersionPicker } from "./versionpicker";
import styles from "./instancesettingsmodal.module.css";

type TabId = "general" | "performance" | "display";

const tabs: readonly TabId[] = ["general", "performance", "display"];
const fieldTabs: Record<DraftField, TabId> = {
  name: "general",
  memoryMin: "performance",
  memoryMax: "performance",
  javaPath: "performance",
  width: "display",
  height: "display"
};

interface InstanceSettingsModalProps {
  open: boolean;
  instance: Instance | null;
  versions: MinecraftVersion[];
  globalWindow: WindowConfig;
  copy: LauncherCopy;
  error: string | null;
  onSave: (id: string, patch: UpdateInstanceInput) => Promise<boolean>;
  onClose: () => void;
}

export function InstanceSettingsModal({ open, instance, versions, globalWindow, copy, error, onSave, onClose }: InstanceSettingsModalProps) {
  return (
    <Modal title={copy.instances.settingsModal.title} closeLabel={copy.closeModal} open={open && instance !== null} onClose={onClose}>
      {instance ? (
        <SettingsForm instance={instance} versions={versions} globalWindow={globalWindow} copy={copy} error={error} onSave={onSave} onClose={onClose} />
      ) : null}
    </Modal>
  );
}

interface SettingsFormProps extends Omit<InstanceSettingsModalProps, "open" | "instance"> {
  instance: Instance;
}

function SettingsForm({ instance, versions, globalWindow, copy, error, onSave, onClose }: SettingsFormProps) {
  const text = copy.instances.settingsModal;
  const baseId = useId();
  const base = useMemo(() => draftFromInstance(instance, globalWindow), [instance, globalWindow]);
  const [draft, setDraft] = useState<InstanceDraft>(base);
  const [tab, setTab] = useState<TabId>("general");
  const [attempted, setAttempted] = useState(false);
  const [saving, setSaving] = useState(false);

  const errors = validateDraft(draft);
  const visibleErrors = attempted ? errors : {};
  const dirty = isDraftDirty(draft, base);
  const tabHasError = (id: TabId) => (Object.keys(visibleErrors) as DraftField[]).some((field) => fieldTabs[field] === id);

  function update(changes: Partial<InstanceDraft>) {
    setDraft((current) => ({ ...current, ...changes }));
  }

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    setAttempted(true);

    const firstInvalid = (Object.keys(errors) as DraftField[])[0];

    if (firstInvalid) {
      setTab(fieldTabs[firstInvalid]);
      return;
    }

    setSaving(true);
    const saved = await onSave(instance.id, buildPatch(draft));
    setSaving(false);

    if (saved) {
      onClose();
    }
  }

  function handleTabKeyDown(event: KeyboardEvent<HTMLButtonElement>, index: number) {
    const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;

    if (step === 0) {
      return;
    }

    event.preventDefault();
    const next = tabs[(index + step + tabs.length) % tabs.length] ?? "general";
    setTab(next);
    document.getElementById(`${baseId}-tab-${next}`)?.focus();
  }

  const fieldError = (field: DraftField) => {
    const code = visibleErrors[field];
    return code ? text.errors[code] : null;
  };

  return (
    <form className={styles.form} onSubmit={(event) => void handleSubmit(event)} noValidate>
      <div className={styles.tabs} role="tablist" aria-label={text.tabsLabel}>
        {tabs.map((id, index) => (
          <button
            key={id}
            id={`${baseId}-tab-${id}`}
            type="button"
            role="tab"
            className={styles.tab}
            aria-selected={tab === id}
            aria-controls={`${baseId}-panel-${id}`}
            tabIndex={tab === id ? 0 : -1}
            data-invalid={tabHasError(id) || undefined}
            onClick={() => setTab(id)}
            onKeyDown={(event) => handleTabKeyDown(event, index)}
          >
            {text.tabs[id]}
          </button>
        ))}
      </div>

      <div className={styles.body}>
        <div id={`${baseId}-panel-general`} role="tabpanel" aria-labelledby={`${baseId}-tab-general`} hidden={tab !== "general"} className={styles.panel}>
          <Field id={`${baseId}-name`} label={text.fields.name} error={fieldError("name")}>
            <input value={draft.name} onChange={(event) => update({ name: event.target.value })} />
          </Field>
          <VersionPicker
            versions={versions}
            value={draft.versionId}
            label={text.fields.version}
            placeholder={copy.instances.hero.noVersion}
            copy={copy}
            onChange={(versionId) => update({ versionId })}
          />
        </div>

        <div id={`${baseId}-panel-performance`} role="tabpanel" aria-labelledby={`${baseId}-tab-performance`} hidden={tab !== "performance"} className={styles.panel}>
          <div className={styles.row}>
            <Field id={`${baseId}-memory-min`} label={text.fields.memoryMin} error={fieldError("memoryMin")}>
              <input value={draft.memoryMin} placeholder="2G" onChange={(event) => update({ memoryMin: event.target.value })} />
            </Field>
            <Field id={`${baseId}-memory-max`} label={text.fields.memoryMax} error={fieldError("memoryMax")}>
              <input value={draft.memoryMax} placeholder="4G" onChange={(event) => update({ memoryMax: event.target.value })} />
            </Field>
          </div>
          <p className={styles.hint}>{text.fields.memoryHint}</p>
          <Field id={`${baseId}-java`} label={text.fields.javaPath} error={fieldError("javaPath")}>
            <input value={draft.javaPath} placeholder={text.fields.javaPathPlaceholder} onChange={(event) => update({ javaPath: event.target.value })} />
          </Field>
        </div>

        <div id={`${baseId}-panel-display`} role="tabpanel" aria-labelledby={`${baseId}-tab-display`} hidden={tab !== "display"} className={styles.panel}>
          <label className="checkbox">
            <input type="checkbox" checked={draft.useGlobalWindow} onChange={(event) => update({ useGlobalWindow: event.target.checked })} />
            {text.fields.useGlobalWindow}
          </label>
          {draft.useGlobalWindow ? (
            <p className={styles.hint}>{text.fields.globalWindowValue(globalWindow.width, globalWindow.height, globalWindow.fullscreen)}</p>
          ) : (
            <>
              <div className={styles.row}>
                <Field id={`${baseId}-width`} label={text.fields.width} error={fieldError("width")}>
                  <input inputMode="numeric" value={draft.width} onChange={(event) => update({ width: event.target.value })} />
                </Field>
                <Field id={`${baseId}-height`} label={text.fields.height} error={fieldError("height")}>
                  <input inputMode="numeric" value={draft.height} onChange={(event) => update({ height: event.target.value })} />
                </Field>
              </div>
              <label className="checkbox">
                <input type="checkbox" checked={draft.fullscreen} onChange={(event) => update({ fullscreen: event.target.checked })} />
                {text.fields.fullscreen}
              </label>
            </>
          )}
        </div>
      </div>

      <footer className={styles.footer}>
        {error ? (
          <p className={styles.serverError} role="alert">
            {error}
          </p>
        ) : null}
        <div className={styles.actions}>
          <button type="button" className={styles.secondary} onClick={onClose}>
            {text.cancel}
          </button>
          <button type="submit" className={styles.primary} disabled={!dirty || saving}>
            {saving ? text.saving : text.save}
          </button>
        </div>
      </footer>
    </form>
  );
}

interface FieldProps {
  id: string;
  label: string;
  error: string | null;
  children: ReactElement<{ id?: string; "aria-invalid"?: boolean; "aria-describedby"?: string }>;
}

function Field({ id, label, error, children }: FieldProps) {
  const errorId = `${id}-error`;
  const control = cloneElement(children, {
    id,
    "aria-invalid": error ? true : undefined,
    "aria-describedby": error ? errorId : undefined
  });

  return (
    <div className={styles.field}>
      <label htmlFor={id}>
        {label}
        {control}
      </label>
      {error ? (
        <p id={errorId} className={styles.error}>
          {error}
        </p>
      ) : null}
    </div>
  );
}
