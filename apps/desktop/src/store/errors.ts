import type { IpcError } from "../../shared/types/api";

export function isIpcError(error: unknown): error is IpcError {
  return (
    typeof error === "object" &&
    error !== null &&
    typeof (error as Partial<IpcError>).code === "string" &&
    typeof (error as Partial<IpcError>).message === "string"
  );
}

export function errorCode(error: unknown): string | null {
  return isIpcError(error) ? error.code : null;
}

/** User-facing text: the translated message for a known backend code, otherwise the backend's own text. */
export function formatError(error: unknown, translations: Readonly<Record<string, string>> = {}): string {
  if (isIpcError(error)) {
    return translations[error.code] ?? error.message;
  }

  return error instanceof Error ? error.message : String(error);
}
