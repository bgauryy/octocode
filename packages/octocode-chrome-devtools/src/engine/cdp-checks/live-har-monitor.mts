import { writeFileSync, appendFileSync } from 'fs';
import { join } from 'path';
import type { Session } from '../cdp-connection.mjs';
import type { RunnerSession } from '../cdp-runner-session.mjs';
import { errorMessage } from '../error-message.mjs';
import { observeFrameEvents } from '../frame-events.mjs';
import type { MonitorEvent, NetworkRecord } from './live-har-format.mjs';
import {
  LONG_TASK_OBSERVER_JS,
  collectPerformanceSnapshot,
  harDocument,
  safeUrl,
  summarize,
} from './live-har-format.mjs';

const MONITOR_MS = Number.parseInt(process.env.MONITOR_MS ?? '30000', 10);
const SLOW_MS = Number.parseInt(process.env.SLOW_MS ?? '1000', 10);
const MAX_STDOUT_ITEMS = Number.parseInt(
  process.env.MAX_STDOUT_ITEMS ?? '10',
  10
);
const MONITOR_URL = process.env.MONITOR_URL || '';

/** The runner adds its run-folder helpers; a bare session runs the monitor without them. */
type MonitorSession = Session &
  Partial<Pick<RunnerSession, 'upsertResourceMap' | 'writeSessionMetadata'>>;

/** Prints and records page exceptions and console errors/warnings. */
function watchPageErrors(
  cdp: MonitorSession,
  pushEvent: (event: MonitorEvent) => void
) {
  cdp.on('Runtime.exceptionThrown', ({ exceptionDetails }) => {
    const message =
      exceptionDetails.exception?.description ?? exceptionDetails.text;
    const frame = exceptionDetails.stackTrace?.callFrames?.[0];
    console.log(`[EXCEPTION] ${String(message).split('\n')[0]}`);
    if (frame)
      console.log(
        `[EXCEPTION_LOCATION] ${frame.url}:${frame.lineNumber}:${frame.columnNumber}`
      );
    pushEvent({ kind: 'exception', message, frame });
  });

  cdp.on('Runtime.consoleAPICalled', ({ type, args }) => {
    if (!['error', 'warning'].includes(type)) return;
    const message = args
      .map(
        (a: { value?: unknown; description?: string }) =>
          a.value ?? a.description ?? '[object]'
      )
      .join(' ');
    console.log(`[CONSOLE:${type.toUpperCase()}] ${message}`);
    pushEvent({ kind: 'console', type, message });
  });
}

export async function run(
  cdp: MonitorSession,
  {
    onReady,
    monitorMs = MONITOR_MS,
    url = MONITOR_URL,
  }: { onReady?: () => Promise<void>; monitorMs?: number; url?: string } = {}
) {
  if (!Number.isSafeInteger(monitorMs) || monitorMs < 0)
    throw new Error('Invalid monitorMs');
  const captureStartedAt = Date.now();
  let actionError: string | null = null;
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  await cdp.send('Log.enable');
  await cdp.send('Page.enable');

  await cdp.send('Runtime.evaluate', { expression: LONG_TASK_OBSERVER_JS });

  const records = new Map<string, NetworkRecord>();
  const requestKey = (requestId: string, meta: { sessionId?: string }) =>
    JSON.stringify([meta.sessionId || null, requestId]);
  let hop = 0;
  const events: MonitorEvent[] = [];
  let slowPrinted = 0;
  const eventsPath = join(cdp.outputDir, 'events.ndjson');
  writeFileSync(eventsPath, '', { mode: 0o600, flag: 'wx' });
  console.log(`[ARTIFACT] EVENTS ${eventsPath}`);
  const pushEvent = (event: MonitorEvent) => {
    const safeEvent = { at: new Date().toISOString(), ...event };
    events.push(safeEvent);
    appendFileSync(eventsPath, JSON.stringify(safeEvent) + '\n');
  };

  cdp.on(
    'Network.requestWillBeSent',
    (
      {
        requestId,
        request,
        wallTime,
        timestamp,
        initiator,
        type,
        frameId,
        redirectResponse,
      },
      meta: { sessionId?: string } = {}
    ) => {
      const prior = records.get(requestKey(requestId, meta));
      if (prior && redirectResponse) {
        prior.response = redirectResponse;
        prior.end = Date.now();
        prior.complete = true;
        records.set(`${requestKey(requestId, meta)}:redirect:${hop++}`, prior);
      }
      records.set(requestKey(requestId, meta), {
        requestId,
        frameId,
        sessionId: meta.sessionId ?? null,
        complete: false,
        request,
        method: request.method,
        url: request.url,
        start: Date.now(),
        startWallTime: wallTime ? wallTime * 1000 : Date.now(),
        timestamp,
        initiator,
        type,
      });
    }
  );

  cdp.on(
    'Network.responseReceived',
    ({ requestId, response, type }, meta = {}) => {
      const record = records.get(requestKey(requestId, meta));
      if (!record) return;
      record.response = response;
      record.type = type ?? record.type;
      record.end = Date.now();
      const ms = record.end - record.start;
      if (response.status >= 400) {
        console.log(
          `[NETWORK_ERROR] ${response.status} ${record.method} ${safeUrl(record.url)} ${ms}ms`
        );
      } else if (ms >= SLOW_MS && slowPrinted++ < MAX_STDOUT_ITEMS) {
        console.log(
          `[METRIC] slow-request status=${response.status} method=${record.method} ms=${ms} url=${safeUrl(record.url)}`
        );
      }
      pushEvent({
        kind: 'response',
        requestId,
        sessionId: record.sessionId,
        status: response.status,
        method: record.method,
        url: safeUrl(record.url),
        ms,
        type: record.type,
      });
    }
  );

  cdp.on(
    'Network.loadingFinished',
    ({ requestId, encodedDataLength }, meta = {}) => {
      const record = records.get(requestKey(requestId, meta));
      if (!record) return;
      record.complete = true;
      record.encodedDataLength = encodedDataLength;
      record.end = Date.now();
    }
  );

  cdp.on(
    'Network.loadingFailed',
    (
      { requestId, errorText, blockedReason },
      meta: { sessionId?: string } = {}
    ) => {
      const record: NetworkRecord = records.get(
        requestKey(requestId, meta)
      ) ?? {
        requestId,
        start: Date.now(),
        url: 'unknown',
        method: 'GET',
      };
      record.complete = true;
      record.failed = true;
      record.errorText = errorText;
      record.blockedReason = blockedReason;
      record.end = Date.now();
      records.set(requestKey(requestId, meta), record);
      console.log(
        `[NETWORK_FAILED] ${safeUrl(record.url)} ${blockedReason ? `blocked=${blockedReason}` : errorText}`
      );
      pushEvent({
        kind: 'network-failed',
        requestId,
        sessionId: meta.sessionId || null,
        url: safeUrl(record.url),
        errorText,
        blockedReason,
      });
    }
  );

  watchPageErrors(cdp, pushEvent);

  const frameEvents = await observeFrameEvents(cdp, [
    'Network',
    'Runtime',
    'Log',
  ]);
  if (url) await cdp.send('Page.navigate', { url: url });
  console.log(
    `[METRIC] live-monitor target="${url || cdp.targetInfo.url}" durationMs=${monitorMs}`
  );
  if (onReady) {
    try {
      await onReady();
    } catch (error) {
      actionError = errorMessage(error);
      process.exitCode = 1;
      console.log(`[FINDING] MONITORED_ACTION_ERROR ${actionError}`);
    }
  }
  const progress = setInterval(
    () =>
      console.log(
        `[PROGRESS] NETWORK observed=${records.size} pending=${[...records.values()].filter(r => !r.complete).length} frames=${frameEvents.coverage().frames.length}`
      ),
    1000
  );
  try {
    await new Promise(resolve => setTimeout(resolve, monitorMs));
  } finally {
    clearInterval(progress);
  }

  await frameEvents.stop();
  const performanceSnapshot = await collectPerformanceSnapshot(cdp);
  const resourceEntries = performanceSnapshot.resources ?? [];
  const summary: ReturnType<typeof summarize> & {
    actualMonitorMs?: number;
    actionError?: unknown;
    iframeEvents?: unknown;
  } = summarize(records, {
    events,
    resourceEntries,
    performanceSnapshot,
    slowMs: SLOW_MS,
    monitorMs,
  });
  summary.actualMonitorMs = Date.now() - captureStartedAt;
  summary.actionError = actionError;
  summary.iframeEvents = frameEvents.coverage();
  const har = harDocument(records, {
    performanceSnapshot,
    monitorMs,
    title: performanceSnapshot.title ?? cdp.targetInfo.title ?? '',
  });

  const harPath = join(cdp.outputDir, 'live-network.har');
  const summaryPath = join(cdp.outputDir, 'network-summary.json');
  const resourcesPath = join(cdp.outputDir, 'resource-timing.json');

  writeFileSync(harPath, `${JSON.stringify(har, null, 2)}\n`, { mode: 0o600 });
  writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, {
    mode: 0o600,
  });
  writeFileSync(
    resourcesPath,
    `${JSON.stringify(resourceEntries, null, 2)}\n`,
    { mode: 0o600 }
  );

  cdp.upsertResourceMap?.('live-har-monitor', {
    type: 'browser-monitor-artifacts',
    harPath,
    eventsPath,
    summaryPath,
    resourcesPath,
    targetUrl: cdp.targetInfo.url,
  });
  cdp.writeSessionMetadata?.({
    lastHarPath: harPath,
    lastNetworkSummaryPath: summaryPath,
  });

  console.log(
    `[METRIC] requests=${summary.counts.requests} failed=${summary.counts.failed} slow=${summary.counts.slow} exceptions=${summary.counts.exceptions} consoleErrors=${summary.counts.consoleErrors} longTasks=${summary.counts.longTasks}`
  );
  for (const failure of summary.failures.slice(0, MAX_STDOUT_ITEMS)) {
    console.log(
      `[NETWORK_ERROR] status=${failure.status} method=${failure.method} url=${failure.url}`
    );
  }
  for (const slow of summary.slow.slice(0, MAX_STDOUT_ITEMS)) {
    console.log(
      `[METRIC] slow status=${slow.status} method=${slow.method} ms=${slow.ms} type=${slow.type} url=${slow.url}`
    );
  }
  console.log(`[ARTIFACT] HAR ${harPath}`);
  console.log(`[ARTIFACT] EVENTS ${eventsPath}`);
  console.log(`[ARTIFACT] SUMMARY ${summaryPath}`);
  console.log(`[ARTIFACT] RESOURCE_TIMING ${resourcesPath}`);
}
