import type { Instance, UpdateInstanceInput, WindowConfig } from "../../shared/types/instance";

export interface InstanceDraft {
  name: string;
  versionId: string;
  memoryMin: string;
  memoryMax: string;
  javaPath: string;
  useGlobalWindow: boolean;
  width: string;
  height: string;
  fullscreen: boolean;
}

export type DraftErrorCode =
  | "nameRequired"
  | "nameTooLong"
  | "memoryMinFormat"
  | "memoryMaxFormat"
  | "memoryOrder"
  | "javaPathInvalid"
  | "widthRange"
  | "heightRange";

export type DraftField = "name" | "memoryMin" | "memoryMax" | "javaPath" | "width" | "height";
export type DraftErrors = Partial<Record<DraftField, DraftErrorCode>>;

export type NewInstanceErrorCode = "nameRequired" | "nameTooLong" | "versionRequired";
export type NewInstanceErrors = Partial<Record<"name" | "versionId", NewInstanceErrorCode>>;

export const windowLimits = { minWidth: 320, maxWidth: 7680, minHeight: 240, maxHeight: 4320 } as const;
export const maxNameLength = 64;

const memoryPattern = /^([1-9]\d*)([MG])$/;
const maxJavaPathLength = 1024;
const integerPattern = /^\d+$/;

export function draftFromInstance(instance: Instance, globalWindow: WindowConfig): InstanceDraft {
  const window = instance.window ?? globalWindow;

  return {
    name: instance.name,
    versionId: instance.versionId,
    memoryMin: instance.memoryConfig.min,
    memoryMax: instance.memoryConfig.max,
    javaPath: instance.javaPath,
    useGlobalWindow: instance.window === undefined,
    width: String(window.width),
    height: String(window.height),
    fullscreen: window.fullscreen
  };
}

export function validateDraft(draft: InstanceDraft): DraftErrors {
  const errors: DraftErrors = {};
  const name = draft.name.trim();

  if (!name) {
    errors.name = "nameRequired";
  } else if (name.length > maxNameLength) {
    errors.name = "nameTooLong";
  }

  const min = toMegabytes(draft.memoryMin);
  const max = toMegabytes(draft.memoryMax);

  if (min === null) {
    errors.memoryMin = "memoryMinFormat";
  }

  if (max === null) {
    errors.memoryMax = "memoryMaxFormat";
  } else if (min !== null && min > max) {
    errors.memoryMax = "memoryOrder";
  }

  if (draft.javaPath.length > maxJavaPathLength || draft.javaPath.includes("\0")) {
    errors.javaPath = "javaPathInvalid";
  }

  if (!draft.useGlobalWindow) {
    if (!isInRange(draft.width, windowLimits.minWidth, windowLimits.maxWidth)) {
      errors.width = "widthRange";
    }

    if (!isInRange(draft.height, windowLimits.minHeight, windowLimits.maxHeight)) {
      errors.height = "heightRange";
    }
  }

  return errors;
}

export function buildPatch(draft: InstanceDraft): UpdateInstanceInput {
  return {
    name: draft.name.trim(),
    versionId: draft.versionId,
    memoryConfig: { min: normalizeMemory(draft.memoryMin), max: normalizeMemory(draft.memoryMax) },
    javaPath: draft.javaPath.trim(),
    window: draft.useGlobalWindow ? null : { width: Number(draft.width), height: Number(draft.height), fullscreen: draft.fullscreen }
  };
}

export function isDraftDirty(draft: InstanceDraft, base: InstanceDraft): boolean {
  return JSON.stringify(comparable(draft)) !== JSON.stringify(comparable(base));
}

export function validateInstanceName(value: string): "nameRequired" | "nameTooLong" | null {
  const name = value.trim();

  if (!name) {
    return "nameRequired";
  }

  return name.length > maxNameLength ? "nameTooLong" : null;
}

export function validateNewInstance(input: { name: string; versionId: string }): NewInstanceErrors {
  const errors: NewInstanceErrors = {};
  const nameError = validateInstanceName(input.name);

  if (nameError) {
    errors.name = nameError;
  }

  if (!input.versionId) {
    errors.versionId = "versionRequired";
  }

  return errors;
}

function comparable(draft: InstanceDraft): unknown {
  return {
    name: draft.name.trim(),
    versionId: draft.versionId,
    memoryMin: normalizeMemory(draft.memoryMin),
    memoryMax: normalizeMemory(draft.memoryMax),
    javaPath: draft.javaPath.trim(),
    window: draft.useGlobalWindow ? null : { width: draft.width.trim(), height: draft.height.trim(), fullscreen: draft.fullscreen }
  };
}

function normalizeMemory(value: string): string {
  return value.trim().toUpperCase();
}

function toMegabytes(value: string): number | null {
  const match = memoryPattern.exec(normalizeMemory(value));
  return match ? Number(match[1]) * (match[2] === "G" ? 1024 : 1) : null;
}

function isInRange(value: string, min: number, max: number): boolean {
  const trimmed = value.trim();
  return integerPattern.test(trimmed) && Number(trimmed) >= min && Number(trimmed) <= max;
}
