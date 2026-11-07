import type { KeyboardEvent } from "react";
import { useEffect, useId, useRef, useState } from "react";
import type { Instance } from "../../../shared/types/instance";
import type { LauncherCopy } from "../../i18n";
import { pickQuickInstances } from "../../store/instancestate";
import { ChevronIcon } from "../icons";
import styles from "./instanceselector.module.css";

type SelectorCopy = LauncherCopy["instances"]["selector"];

interface InstanceSelectorProps {
  instances: Instance[];
  activeId: string | null;
  runningId: string | null;
  copy: SelectorCopy;
  runningLabel: string;
  onSelect: (id: string) => void;
  onManage: () => void;
}

export function InstanceSelector({ instances, activeId, runningId, copy, runningLabel, onSelect, onManage }: InstanceSelectorProps) {
  const baseId = useId();
  const [open, setOpen] = useState(false);
  const [highlighted, setHighlighted] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const visible = pickQuickInstances(instances, activeId);
  const manageIndex = visible.length;
  const optionCount = visible.length + 1;
  const active = instances.find((instance) => instance.id === activeId) ?? null;
  const optionId = (index: number) => `${baseId}-option-${index}`;

  useEffect(() => {
    if (open) {
      listRef.current?.focus();
    }
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    function handlePointerDown(event: MouseEvent) {
      if (!containerRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    }

    document.addEventListener("mousedown", handlePointerDown);
    return () => document.removeEventListener("mousedown", handlePointerDown);
  }, [open]);

  function openMenu() {
    const activeIndex = visible.findIndex((instance) => instance.id === activeId);
    setHighlighted(Math.max(activeIndex, 0));
    setOpen(true);
  }

  function closeMenu() {
    setOpen(false);
    buttonRef.current?.focus();
  }

  function choose(index: number) {
    const instance = visible[index];
    setOpen(false);
    buttonRef.current?.focus();

    if (index === manageIndex) {
      onManage();
    } else if (instance && instance.id !== activeId) {
      onSelect(instance.id);
    }
  }

  function handleButtonKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      openMenu();
    }
  }

  function handleListKeyDown(event: KeyboardEvent<HTMLUListElement>) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        setHighlighted((current) => (current + 1) % optionCount);
        break;
      case "ArrowUp":
        event.preventDefault();
        setHighlighted((current) => (current - 1 + optionCount) % optionCount);
        break;
      case "Home":
        event.preventDefault();
        setHighlighted(0);
        break;
      case "End":
        event.preventDefault();
        setHighlighted(optionCount - 1);
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        choose(highlighted);
        break;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        closeMenu();
        break;
      case "Tab":
        setOpen(false);
        break;
    }
  }

  return (
    <div ref={containerRef} className={styles.selector}>
      <button
        ref={buttonRef}
        type="button"
        className={styles.trigger}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={`${baseId}-popup`}
        onClick={() => (open ? closeMenu() : openMenu())}
        onKeyDown={handleButtonKeyDown}
      >
        <span className={styles.caption}>{copy.active}</span>
        <span className={styles.name}>{active?.name ?? "-"}</span>
        <ChevronIcon />
      </button>
      {open ? (
        <div id={`${baseId}-popup`} className={styles.popup}>
          {visible.length === 0 ? <p className={styles.empty}>{copy.empty}</p> : null}
          <ul
            ref={listRef}
            role="listbox"
            tabIndex={-1}
            className={styles.list}
            aria-label={copy.label}
            aria-activedescendant={optionId(highlighted)}
            onKeyDown={handleListKeyDown}
          >
            {visible.map((instance, index) => (
              <li
                key={instance.id}
                id={optionId(index)}
                role="option"
                aria-selected={instance.id === activeId}
                className={styles.option}
                data-highlighted={highlighted === index}
                onMouseEnter={() => setHighlighted(index)}
                onClick={() => choose(index)}
              >
                <span className={styles.optionName}>{instance.name}</span>
                <span className={styles.optionMeta}>
                  {instance.id === runningId ? <span className={styles.running}>{runningLabel}</span> : null}
                  {instance.versionId || "-"}
                </span>
              </li>
            ))}
            <li
              id={optionId(manageIndex)}
              role="option"
              aria-selected={false}
              className={`${styles.option} ${styles.manage}`}
              data-highlighted={highlighted === manageIndex}
              onMouseEnter={() => setHighlighted(manageIndex)}
              onClick={() => choose(manageIndex)}
            >
              {copy.manage}
            </li>
          </ul>
        </div>
      ) : null}
    </div>
  );
}
