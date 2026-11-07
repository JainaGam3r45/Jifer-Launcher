import { useId, useRef, useState } from "react";
import type { CloneInstanceInput, CreateInstanceInput, Instance } from "../../../shared/types/instance";
import type { MinecraftVersion } from "../../../shared/types/version";
import { useDialog } from "../../hooks/usedialog";
import type { LauncherCopy } from "../../i18n";
import { Modal } from "../modal";
import { InstanceCard } from "./instancecard";
import { InstanceForm, type InstanceFormMode } from "./instanceform";
import styles from "./instancedrawer.module.css";

type DrawerView = { kind: "list" } | InstanceFormMode;

interface InstanceDrawerProps {
  open: boolean;
  instances: Instance[];
  activeId: string | null;
  runningId: string | null;
  versions: MinecraftVersion[];
  copy: LauncherCopy;
  error: string | null;
  onSelect: (id: string) => void;
  onOpenSettings: (id: string) => void;
  onCreate: (input: CreateInstanceInput) => Promise<boolean>;
  onClone: (input: CloneInstanceInput) => Promise<boolean>;
  onRemove: (id: string) => Promise<boolean>;
  onDismissError: () => void;
  onClose: () => void;
}

export function InstanceDrawer({ open, ...props }: InstanceDrawerProps) {
  return open ? <DrawerPanel {...props} /> : null;
}

function DrawerPanel({
  instances,
  activeId,
  runningId,
  versions,
  copy,
  error,
  onSelect,
  onOpenSettings,
  onCreate,
  onClone,
  onRemove,
  onDismissError,
  onClose
}: Omit<InstanceDrawerProps, "open">) {
  const text = copy.instances.drawer;
  const titleId = useId();
  const panelRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const [view, setView] = useState<DrawerView>({ kind: "list" });
  const [pendingRemoval, setPendingRemoval] = useState<Instance | null>(null);
  const [removing, setRemoving] = useState(false);

  const showList = () => setView({ kind: "list" });
  useDialog(panelRef, view.kind === "list" ? onClose : showList, closeRef);

  const defaultVersionId = versions.find((version) => version.type === "release")?.id ?? "";

  function openForm(next: InstanceFormMode) {
    onDismissError();
    setView(next);
  }

  function askRemoval(instance: Instance) {
    onDismissError();
    setPendingRemoval(instance);
  }

  async function confirmRemoval() {
    if (!pendingRemoval) {
      return;
    }

    setRemoving(true);
    const removed = await onRemove(pendingRemoval.id);
    setRemoving(false);

    if (removed) {
      setPendingRemoval(null);
    }
  }

  function removeBlockedReason(instance: Instance): string | null {
    if (instance.id === activeId) {
      return text.removeBlockedActive;
    }

    return instance.id === runningId ? text.removeBlockedRunning : null;
  }

  return (
    <>
      <div className={styles.backdrop} role="presentation" onMouseDown={onClose}>
        <aside ref={panelRef} className={styles.panel} role="dialog" aria-modal="true" aria-labelledby={titleId} onMouseDown={(event) => event.stopPropagation()}>
          <header className={styles.header}>
            <h2 id={titleId}>{text.title}</h2>
            <button ref={closeRef} type="button" className={styles.close} onClick={onClose} aria-label={text.closeLabel}>
              x
            </button>
          </header>

          <div className={styles.body}>
            {view.kind === "list" ? (
              <>
                <button type="button" className={styles.create} onClick={() => openForm({ kind: "create" })}>
                  + {text.newInstance}
                </button>
                {error && !pendingRemoval ? (
                  <p className={styles.error} role="alert">
                    {error}
                  </p>
                ) : null}
                {instances.length === 0 ? <p className={styles.empty}>{text.empty}</p> : null}
                <ul className={styles.list}>
                  {instances.map((instance) => (
                    <li key={instance.id}>
                      <InstanceCard
                        instance={instance}
                        active={instance.id === activeId}
                        running={instance.id === runningId}
                        copy={copy.instances}
                        removeBlockedReason={removeBlockedReason(instance)}
                        onSelect={() => onSelect(instance.id)}
                        onSettings={() => onOpenSettings(instance.id)}
                        onClone={() => openForm({ kind: "clone", source: instance })}
                        onRemove={() => askRemoval(instance)}
                      />
                    </li>
                  ))}
                </ul>
              </>
            ) : (
              <InstanceForm
                mode={view}
                versions={versions}
                defaultVersionId={defaultVersionId}
                copy={copy}
                error={error}
                onCreate={onCreate}
                onClone={onClone}
                onDone={showList}
                onBack={showList}
              />
            )}
          </div>
        </aside>
      </div>

      <Modal title={text.confirm.title} closeLabel={copy.closeModal} open={pendingRemoval !== null} onClose={() => setPendingRemoval(null)}>
        <div className={styles.confirm}>
          <p>{pendingRemoval ? text.confirm.message(pendingRemoval.name) : null}</p>
          {error ? (
            <p className={styles.error} role="alert">
              {error}
            </p>
          ) : null}
          <div className={styles.confirmActions}>
            <button type="button" className={styles.cancel} onClick={() => setPendingRemoval(null)}>
              {text.confirm.cancel}
            </button>
            <button type="button" className={styles.destructive} disabled={removing} onClick={() => void confirmRemoval()}>
              {removing ? text.confirm.removing : text.confirm.confirm}
            </button>
          </div>
        </div>
      </Modal>
    </>
  );
}
