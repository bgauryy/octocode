import { readFileSync } from 'fs';
import { join } from 'path';
import type { RunnerSession } from '../cdp-runner-session.mjs';
import { createSourceMapResolver } from '../sourcemap-resolver.mjs';
import { run as monitor } from './live-har-monitor.mjs';
import { run as screenshot } from './page-screenshot.mjs';
import type { MonitorEvent } from './live-har-format.mjs';

// dev-check: load DEV_URL in the attached (fresh) tab with console, network and
// source-map listeners already on, wait for load or network idle up to
// DEV_TIMEOUT_MS, then save dev-check.json and a screenshot (SHOT_* apply).
//   DEV_WAIT_UNTIL  load (default) | networkIdle
//   DEV_IDLE_MS     quiet window for networkIdle (default 500)
//   DEV_TIMEOUT_MS  wait ceiling (default 30000); evidence so far is kept

type CallFrame = {
  functionName?: string;
  scriptId?: string;
  url?: string;
  lineNumber: number;
  columnNumber: number;
};
type Maps = Awaited<ReturnType<typeof createSourceMapResolver>>;

const whole = (name: string, fallback: number) => {
  const value = Number(process.env[name] ?? fallback);
  if (!Number.isSafeInteger(value) || value < 0)
    throw new Error(`Invalid ${name}`);
  return value;
};

/** CDP frames are 0-based; reported positions, generated and original, are 1-based. */
const stack = (maps: Maps, frames: unknown) =>
  Array.isArray(frames)
    ? frames.map((frame: CallFrame) => {
        const original = frame.scriptId
          ? maps.resolve(frame.scriptId, frame.lineNumber, frame.columnNumber)
          : null;
        return {
          function: frame.functionName || null,
          url: frame.url ?? null,
          line: frame.lineNumber + 1,
          column: frame.columnNumber + 1,
          ...(original
            ? {
                original: {
                  source: original.source,
                  line: original.line,
                  column: original.col + 1,
                  name: original.name,
                },
              }
            : {}),
        };
      })
    : [];

/** One console, browser-log or exception event as a report row. */
const message = (maps: Maps, event: MonitorEvent) => ({
  at: event.at,
  kind: event.kind,
  level: event.kind === 'exception' ? 'error' : event.type,
  ...(event.source ? { source: event.source } : {}),
  message:
    typeof event.message === 'string'
      ? event.message
      : JSON.stringify(event.message ?? ''),
  ...(event.url ? { url: event.url } : {}),
  stack: stack(maps, event.stack),
});

/** Tile files page-screenshot recorded in its manifest. */
function screenshotFiles(cdp: RunnerSession): string[] {
  const manifest = JSON.parse(
    readFileSync(join(cdp.outputDir, 'screenshot-manifest.json'), 'utf8')
  );
  return manifest.tiles.map((tile: { file: string }) => tile.file);
}

export async function run(cdp: RunnerSession) {
  const url = process.env.DEV_URL ?? '';
  if (!/^https?:\/\/\S+$/.test(url)) throw new Error('DEV_URL needs http(s)');
  const waitUntil =
    process.env.DEV_WAIT_UNTIL === 'networkIdle' ? 'networkIdle' : 'load';
  const timeoutMs = whole('DEV_TIMEOUT_MS', 30000);
  // Debugger.scriptParsed must be observed from the first script on.
  const maps = await createSourceMapResolver(cdp);
  const started = Date.now();
  const observed = await monitor(cdp, {
    url,
    monitorMs: timeoutMs,
    waitUntil,
    idleMs: whole('DEV_IDLE_MS', 500),
  });
  await maps.settle(Math.max(1000, timeoutMs - (Date.now() - started)));
  const messages = observed.events
    .filter(e => ['console', 'log', 'exception'].includes(e.kind))
    .map(event => message(maps, event));
  await screenshot(cdp);
  const tiles = screenshotFiles(cdp);
  const page = await cdp.send('Runtime.evaluate', {
    expression: '({ url: location.href, title: document.title })',
    returnByValue: true,
  });
  const errors = messages.filter(m => m.level === 'error');
  const warnings = messages.filter(m => m.level === 'warning');
  const failedRequests = observed.summary.failures;
  const saved = cdp.saveArtifact('dev-check.json', {
    url,
    finalUrl: page.result?.value?.url ?? null,
    title: page.result?.value?.title ?? null,
    targetId: cdp.targetInfo.id,
    waitUntil,
    settled: observed.settled,
    loaded: observed.loaded,
    pendingRequests: observed.pending,
    elapsedMs: Date.now() - started,
    clean: !errors.length && !failedRequests.length,
    counts: {
      errors: errors.length,
      warnings: warnings.length,
      failedRequests: failedRequests.length,
      requests: observed.summary.counts.requests,
    },
    errors,
    warnings,
    failedRequests,
    screenshot: tiles[0] ?? null,
    artifacts: {
      screenshots: tiles,
      har: observed.harPath,
      events: observed.eventsPath,
      networkSummary: observed.summaryPath,
    },
  });
  process.stdout.write(`[ARTIFACT] DEV_CHECK ${saved.file}\n`);
}
