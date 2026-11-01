import type { summarize } from './live-har-format.mjs';

/**
 * Waits the whole window, or with waitUntil until the page loaded (load) or loaded and
 * kept no request pending for idleMs (networkIdle), never past monitorMs from start.
 * Returns whether the condition was met.
 */
export async function waitWindow(
  {
    waitUntil,
    monitorMs,
    idleMs,
    startedAt,
  }: {
    waitUntil?: 'load' | 'networkIdle';
    monitorMs: number;
    idleMs: number;
    startedAt: number;
  },
  loaded: () => boolean,
  pending: () => number
) {
  if (!waitUntil) {
    await new Promise(resolve => setTimeout(resolve, monitorMs));
    return false;
  }
  for (let idleSince: number | undefined; ;) {
    const now = Date.now();
    idleSince = loaded() && !pending() ? (idleSince ?? now) : undefined;
    const settled =
      waitUntil === 'load'
        ? loaded()
        : idleSince !== undefined && now - idleSince >= idleMs;
    if (settled) return true;
    if (now - startedAt >= monitorMs) return false;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
}

/** Prints the first failed and slow requests of a monitor summary. */
export function printFindings(
  summary: ReturnType<typeof summarize>,
  limit: number
) {
  for (const failure of summary.failures.slice(0, limit))
    process.stdout.write(
      `[NETWORK_ERROR] status=${failure.status} method=${failure.method} url=${failure.url}\n`
    );
  for (const slow of summary.slow.slice(0, limit))
    process.stdout.write(
      `[METRIC] slow status=${slow.status} method=${slow.method} ms=${slow.ms} type=${slow.type} url=${slow.url}\n`
    );
}
