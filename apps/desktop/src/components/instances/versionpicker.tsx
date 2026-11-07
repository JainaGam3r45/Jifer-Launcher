import { useMemo, useState } from "react";
import type { MinecraftVersion } from "../../../shared/types/version";
import type { LauncherCopy } from "../../i18n";
import { filterVersions, type VersionFilter } from "../../store/versionfilter";
import styles from "./versionpicker.module.css";

type PickerCopy = Pick<LauncherCopy, "version" | "releases" | "snapshots" | "allVersions" | "searchVersion">;

interface VersionPickerProps {
  versions: MinecraftVersion[];
  value: string;
  label: string;
  placeholder: string;
  copy: PickerCopy;
  invalid?: boolean;
  describedBy?: string;
  onChange: (versionId: string) => void;
}

export function VersionPicker({ versions, value, label, placeholder, copy, invalid, describedBy, onChange }: VersionPickerProps) {
  const [filter, setFilter] = useState<VersionFilter>("release");
  const [query, setQuery] = useState("");
  const options = useMemo(() => filterVersions(versions, filter, query, value), [filter, query, value, versions]);
  const knownValue = options.some((version) => version.id === value);

  return (
    <div className={styles.picker}>
      <div className={styles.controls}>
        <select value={filter} onChange={(event) => setFilter(event.target.value as VersionFilter)} aria-label={copy.version}>
          <option value="release">{copy.releases}</option>
          <option value="snapshot">{copy.snapshots}</option>
          <option value="all">{copy.allVersions}</option>
        </select>
        <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={copy.searchVersion} aria-label={copy.searchVersion} />
      </div>
      <label>
        {label}
        <select value={value} onChange={(event) => onChange(event.target.value)} aria-invalid={invalid || undefined} aria-describedby={describedBy}>
          {value === "" ? <option value="">{placeholder}</option> : null}
          {value !== "" && !knownValue ? <option value={value}>{value}</option> : null}
          {options.map((version) => (
            <option key={version.id} value={version.id}>
              {version.id} · {version.type}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}
