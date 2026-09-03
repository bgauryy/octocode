/**
 * ui-ticker — the extension's single 1-second UI clock.
 *
 * Every periodically-refreshing surface (footer metrics, agent ledger)
 * subscribes here instead of owning its own setInterval, so at most ONE timer
 * exists process-wide and all per-second refreshes fire inside the same tick —
 * pi then coalesces the resulting requestRender calls into one repaint instead
 * of two out-of-phase ones.
 *
 * The timer starts lazily with the first subscriber, stops with the last, and
 * is unref()'d so it never holds the process open.
 */

export const UI_TICK_MS = 1000;

const subscribers = new Map<string, () => void>();
let timer: ReturnType<typeof setInterval> | undefined;

/** Subscribe (fn) or unsubscribe (undefined) a per-second refresh under `key`. */
export function setUiTickSubscriber(key: string, fn: (() => void) | undefined): void {
  if (fn) subscribers.set(key, fn);
  else subscribers.delete(key);

  if (subscribers.size > 0 && !timer) {
    timer = setInterval(() => {
      // Snapshot: a subscriber may (un)subscribe itself mid-tick.
      for (const tick of [...subscribers.values()]) {
        try {
          tick();
        } catch {
          // Each surface's refresh is best-effort UI work; one failing
          // subscriber must not starve the others.
        }
      }
    }, UI_TICK_MS);
    (timer as { unref?: () => void }).unref?.();
  } else if (subscribers.size === 0 && timer) {
    clearInterval(timer);
    timer = undefined;
  }
}

/** Whether `key` currently has a live subscription (test/introspection hook). */
export function hasUiTickSubscriber(key: string): boolean {
  return subscribers.has(key);
}
