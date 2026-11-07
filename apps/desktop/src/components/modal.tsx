import type { ReactNode } from "react";
import { useId, useRef } from "react";
import { useDialog } from "../hooks/usedialog";

interface ModalProps {
  title: string;
  closeLabel: string;
  open: boolean;
  onClose: () => void;
  children: ReactNode;
}

export function Modal({ open, ...props }: ModalProps) {
  return open ? <ModalDialog {...props} /> : null;
}

function ModalDialog({ title, closeLabel, onClose, children }: Omit<ModalProps, "open">) {
  const titleId = useId();
  const dialogRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  useDialog(dialogRef, onClose, closeRef);

  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        ref={dialogRef}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="modal-header">
          <h2 id={titleId}>{title}</h2>
          <button ref={closeRef} type="button" onClick={onClose} aria-label={closeLabel}>
            x
          </button>
        </header>
        {children}
      </section>
    </div>
  );
}
