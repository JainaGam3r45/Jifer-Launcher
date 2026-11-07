import { useEffect, useState } from "react";
import type { LauncherSettings, LauncherSettingsUpdate } from "../../shared/types/settings";
import type { LauncherCopy } from "../i18n";

type SettingsCopy = LauncherCopy["settingsPanel"];

interface SettingsPanelProps {
  settings: LauncherSettings;
  onUpdate: (update: LauncherSettingsUpdate) => Promise<boolean>;
  copy: SettingsCopy;
}

/** Text fields are committed on blur: the backend validates whole values, not keystrokes. */
export function SettingsPanel({ settings, onUpdate, copy }: SettingsPanelProps) {
  const [root, setRoot] = useState(settings.minecraftRoot);
  const [width, setWidth] = useState(String(settings.window.width));
  const [height, setHeight] = useState(String(settings.window.height));

  useEffect(() => setRoot(settings.minecraftRoot), [settings.minecraftRoot]);
  useEffect(() => setWidth(String(settings.window.width)), [settings.window.width]);
  useEffect(() => setHeight(String(settings.window.height)), [settings.window.height]);

  function commitWindow(next: Partial<LauncherSettings["window"]>) {
    void onUpdate({ window: { ...settings.window, ...next } }).then((saved) => {
      if (!saved) {
        setWidth(String(settings.window.width));
        setHeight(String(settings.window.height));
      }
    });
  }

  return (
    <section className="panel settings-panel">
      <div className="panel-heading">
        <p className="eyebrow">{copy.eyebrow}</p>
        <h2>{copy.title}</h2>
      </div>
      <label>
        {copy.language}
        <select value={settings.ui.language} onChange={(event) => void onUpdate({ ui: { language: event.target.value as LauncherSettings["ui"]["language"] } })}>
          <option value="es">Español</option>
          <option value="en">English</option>
        </select>
      </label>
      <label>
        {copy.minecraftDirectory}
        <input
          value={root}
          spellCheck={false}
          onChange={(event) => setRoot(event.target.value)}
          onBlur={() => {
            if (root !== settings.minecraftRoot) {
              void onUpdate({ minecraftRoot: root }).then((saved) => saved || setRoot(settings.minecraftRoot));
            }
          }}
        />
      </label>
      <div className="grid three">
        <label>
          {copy.width}
          <input type="number" value={width} onChange={(event) => setWidth(event.target.value)} onBlur={() => commitWindow({ width: Number(width) })} />
        </label>
        <label>
          {copy.height}
          <input type="number" value={height} onChange={(event) => setHeight(event.target.value)} onBlur={() => commitWindow({ height: Number(height) })} />
        </label>
        <label className="checkbox">
          <input type="checkbox" checked={settings.window.fullscreen} onChange={(event) => commitWindow({ fullscreen: event.target.checked })} />
          {copy.fullscreen}
        </label>
      </div>
    </section>
  );
}
