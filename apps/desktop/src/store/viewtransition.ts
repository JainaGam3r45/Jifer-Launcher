import { flushSync } from "react-dom";

type TransitionDocument = Document & { startViewTransition?: (update: () => void) => unknown };

/**
 * Runs a state update inside a View Transition when the platform supports it and the user has not asked
 * for reduced motion; otherwise applies it immediately.
 */
export function withViewTransition(update: () => void): void {
  const doc = document as TransitionDocument;
  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  if (!doc.startViewTransition || reduced) {
    update();
    return;
  }

  doc.startViewTransition(() => flushSync(update));
}
