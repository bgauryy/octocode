import { writeFileSync } from 'fs';
import { join } from 'path';

const MONITOR_MS = Number.parseInt(process.env.MONITOR_MS ?? '30000', 10);
const SLOW_MS = Number.parseInt(process.env.SLOW_MS ?? '1000', 10);
const MAX_STDOUT_ITEMS = Number.parseInt(process.env.MAX_STDOUT_ITEMS ?? '10', 10);

function nowIso() {
  return new Date().toISOString();
}

function safeUrl(raw) {
  try {
    const url = new URL(raw);
    url.username = '';
    url.password = '';
    for (const key of [...url.searchParams.keys()]) {
      if (/token|key|secret|session|auth|password/i.test(key)) url.searchParams.set(key, '[REDACTED]');
    }
    return url.href;
  } catch {
    return String(raw ?? '');
  }
}

function headerPairs(headers = {}) {
  return Object.entries(headers)
    .filter(([name]) => !/cookie|authorization|proxy-authorization|x-api-key|token|secret/i.test(name))
    .map(([name, value]) => ({ name, value: String(value) }));
}

function mimeToContent(mimeType = '') {
  return { size: 0, mimeType: mimeType || 'application/octet-stream', text: '' };
}

function toHarEntry(record) {
  const startedDateTime = new Date(record.startWallTime ?? record.start).toISOString();
  const duration = Math.max(0, (record.end ?? Date.now()) - record.start);
  const response = record.response ?? {};
  const request = record.request ?? {};
  const url = safeUrl(request.url ?? record.url);
  return {
    startedDateTime,
    time: duration,
    request: {
      method: request.method ?? record.method ?? 'GET',
      url,
      httpVersion: 'HTTP/2',
      cookies: [],
      headers: headerPairs(request.headers),
      queryString: (() => {
        try { return [...new URL(url).searchParams.entries()].map(([name, value]) => ({ name, value })); }
        catch { return []; }
      })(),
      headersSize: -1,
      bodySize: request.postData ? String(request.postData).length : 0,
    },
    response: {
      status: response.status ?? (record.failed ? 0 : -1),
      statusText: response.statusText ?? record.errorText ?? '',
      httpVersion: response.protocol ?? 'HTTP/2',
      cookies: [],
      headers: headerPairs(response.headers),
      content: mimeToContent(response.mimeType),
      redirectURL: response.headers?.location ?? '',
      headersSize: -1,
      bodySize: response.encodedDataLength ?? record.encodedDataLength ?? -1,
    },
    cache: {},
    timings: {
      blocked: -1,
      dns: -1,
      connect: -1,
      send: 0,
      wait: duration,
      receive: 0,
      ssl: -1,
    },
    pageref: 'live-page',
    _resourceType: record.type ?? 'Other',
    _requestId: record.requestId,
    _failed: Boolean(record.failed),
    _errorText: record.errorText ?? null,
    _blockedReason: record.blockedReason ?? null,
    _initiator: record.initiator?.type ?? null,
  };
}

function summarize(records, events, resourceEntries, performanceSnapshot) {
  const completed = [...records.values()].filter(r => r.response || r.failed);
  const failed = completed.filter(r => r.failed || (r.response?.status ?? 0) >= 400);
  const slow = completed
    .map(r => ({
      url: safeUrl(r.request?.url ?? r.url),
      method: r.request?.method ?? r.method,
      status: r.response?.status ?? (r.failed ? 0 : -1),
      ms: Math.max(0, (r.end ?? Date.now()) - r.start),
      type: r.type ?? 'Other',
    }))
    .filter(r => r.ms >= SLOW_MS)
    .sort((a, b) => b.ms - a.ms)
    .slice(0, 25);

  const exceptions = events.filter(e => e.kind === 'exception').length;
  const consoleErrors = events.filter(e => e.kind === 'console' && e.type === 'error').length;

  return {
    capturedAt: nowIso(),
    monitorMs: MONITOR_MS,
    target: performanceSnapshot?.location ?? null,
    counts: {
      requests: records.size,
      completed: completed.length,
      failed: failed.length,
      slow: slow.length,
      exceptions,
      consoleErrors,
      resourceEntries: resourceEntries.length,
      longTasks: performanceSnapshot?.longTasks?.length ?? 0,
    },
    pageTiming: performanceSnapshot?.navigationTiming ?? null,
    vitalsApprox: performanceSnapshot?.vitalsApprox ?? null,
    failures: failed.slice(0, 50).map(r => ({
      url: safeUrl(r.request?.url ?? r.url),
      method: r.request?.method ?? r.method,
      status: r.response?.status ?? 0,
      errorText: r.errorText ?? null,
      blockedReason: r.blockedReason ?? null,
    })),
    slow,
  };
}

async function collectPerformanceSnapshot(cdp) {
  const result = await cdp.send('Runtime.evaluate', {
    returnByValue: true,
    expression: `(() => {
      const nav = performance.getEntriesByType('navigation')[0];
      const resources = performance.getEntriesByType('resource')
        .map(r => ({
          name: r.name,
          initiatorType: r.initiatorType,
          startTime: Math.round(r.startTime),
          duration: Math.round(r.duration),
          transferSize: r.transferSize || 0,
          encodedBodySize: r.encodedBodySize || 0,
          decodedBodySize: r.decodedBodySize || 0,
        }))
        .sort((a, b) => b.duration - a.duration)
        .slice(0, 200);
      const longTasks = (globalThis.__octocodeLongTasks || []).slice(-100);
      return {
        location: location.href,
        title: document.title,
        navigationTiming: nav ? {
          domContentLoaded: Math.round(nav.domContentLoadedEventEnd),
          load: Math.round(nav.loadEventEnd),
          responseStart: Math.round(nav.responseStart),
          responseEnd: Math.round(nav.responseEnd),
          transferSize: nav.transferSize || 0,
          encodedBodySize: nav.encodedBodySize || 0,
        } : null,
        vitalsApprox: {
          now: Math.round(performance.now()),
          memoryUsed: performance.memory?.usedJSHeapSize ?? null,
          memoryLimit: performance.memory?.jsHeapSizeLimit ?? null,
        },
        resources,
        longTasks,
      };
    })()`,
  });
  return result.result?.value ?? { resources: [], longTasks: [] };
}

export async function run(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  await cdp.send('Log.enable');
  await cdp.send('Page.enable');

  await cdp.send('Runtime.evaluate', {
    expression: `(() => {
      if (!globalThis.__octocodeLongTasks && 'PerformanceObserver' in globalThis) {
        globalThis.__octocodeLongTasks = [];
        try {
          const observer = new PerformanceObserver(list => {
            for (const entry of list.getEntries()) {
              globalThis.__octocodeLongTasks.push({
                startTime: Math.round(entry.startTime),
                duration: Math.round(entry.duration),
                name: entry.name,
              });
            }
            if (globalThis.__octocodeLongTasks.length > 200) {
              globalThis.__octocodeLongTasks = globalThis.__octocodeLongTasks.slice(-200);
            }
          });
          observer.observe({ type: 'longtask', buffered: true });
        } catch {}
      }
    })()`,
  });

  const records = new Map();
  const events = [];
  const eventLines = [];
  const pushEvent = (event) => {
    const safeEvent = { at: nowIso(), ...event };
    events.push(safeEvent);
    eventLines.push(JSON.stringify(safeEvent));
  };

  cdp.on('Network.requestWillBeSent', ({ requestId, request, wallTime, timestamp, initiator, type }) => {
    records.set(requestId, {
      requestId,
      request,
      method: request.method,
      url: request.url,
      start: Date.now(),
      startWallTime: wallTime ? wallTime * 1000 : Date.now(),
      timestamp,
      initiator,
      type,
    });
  });

  cdp.on('Network.responseReceived', ({ requestId, response, type }) => {
    const record = records.get(requestId);
    if (!record) return;
    record.response = response;
    record.type = type ?? record.type;
    record.end = Date.now();
    const ms = record.end - record.start;
    if (response.status >= 400) {
      console.log(`[NETWORK_ERROR] ${response.status} ${record.method} ${safeUrl(record.url)} ${ms}ms`);
    } else if (ms >= SLOW_MS && events.filter(e => e.kind === 'slow-request').length < MAX_STDOUT_ITEMS) {
      console.log(`[METRIC] slow-request status=${response.status} method=${record.method} ms=${ms} url=${safeUrl(record.url)}`);
    }
    pushEvent({ kind: 'response', requestId, status: response.status, method: record.method, url: safeUrl(record.url), ms, type: record.type });
  });

  cdp.on('Network.loadingFinished', ({ requestId, encodedDataLength }) => {
    const record = records.get(requestId);
    if (!record) return;
    record.encodedDataLength = encodedDataLength;
    record.end = Date.now();
  });

  cdp.on('Network.loadingFailed', ({ requestId, errorText, blockedReason }) => {
    const record = records.get(requestId) ?? { requestId, start: Date.now(), url: 'unknown', method: 'GET' };
    record.failed = true;
    record.errorText = errorText;
    record.blockedReason = blockedReason;
    record.end = Date.now();
    records.set(requestId, record);
    console.log(`[NETWORK_FAILED] ${safeUrl(record.url)} ${blockedReason ? `blocked=${blockedReason}` : errorText}`);
    pushEvent({ kind: 'network-failed', requestId, url: safeUrl(record.url), errorText, blockedReason });
  });

  cdp.on('Runtime.exceptionThrown', ({ exceptionDetails }) => {
    const message = exceptionDetails.exception?.description ?? exceptionDetails.text;
    const frame = exceptionDetails.stackTrace?.callFrames?.[0];
    console.log(`[EXCEPTION] ${String(message).split('\n')[0]}`);
    if (frame) console.log(`[EXCEPTION_LOCATION] ${frame.url}:${frame.lineNumber}:${frame.columnNumber}`);
    pushEvent({ kind: 'exception', message, frame });
  });

  cdp.on('Runtime.consoleAPICalled', ({ type, args }) => {
    if (!['error', 'warning'].includes(type)) return;
    const message = args.map(a => a.value ?? a.description ?? '[object]').join(' ').slice(0, 500);
    console.log(`[CONSOLE:${type.toUpperCase()}] ${message}`);
    pushEvent({ kind: 'console', type, message });
  });

  console.log(`[METRIC] live-monitor target="${cdp.targetInfo.url}" durationMs=${MONITOR_MS}`);
  await new Promise(resolve => setTimeout(resolve, MONITOR_MS));

  const performanceSnapshot = await collectPerformanceSnapshot(cdp);
  const resourceEntries = performanceSnapshot.resources ?? [];
  const summary = summarize(records, events, resourceEntries, performanceSnapshot);
  const har = {
    log: {
      version: '1.2',
      creator: { name: 'octocode-chrome-devtools live-har-monitor', version: '1.0.0' },
      browser: { name: 'Chrome', version: 'CDP' },
      pages: [{
        startedDateTime: new Date(Date.now() - MONITOR_MS).toISOString(),
        id: 'live-page',
        title: performanceSnapshot.title ?? cdp.targetInfo.title ?? '',
        pageTimings: {
          onContentLoad: performanceSnapshot.navigationTiming?.domContentLoaded ?? -1,
          onLoad: performanceSnapshot.navigationTiming?.load ?? -1,
        },
      }],
      entries: [...records.values()].filter(r => r.response || r.failed).map(toHarEntry),
    },
  };

  const harPath = join(cdp.outputDir, 'live-network.har');
  const eventsPath = join(cdp.outputDir, 'events.ndjson');
  const summaryPath = join(cdp.outputDir, 'network-summary.json');
  const resourcesPath = join(cdp.outputDir, 'resource-timing.json');

  writeFileSync(harPath, `${JSON.stringify(har, null, 2)}\n`, { mode: 0o600 });
  writeFileSync(eventsPath, `${eventLines.join('\n')}\n`, { mode: 0o600 });
  writeFileSync(summaryPath, `${JSON.stringify(summary, null, 2)}\n`, { mode: 0o600 });
  writeFileSync(resourcesPath, `${JSON.stringify(resourceEntries, null, 2)}\n`, { mode: 0o600 });

  cdp.upsertResourceMap?.('live-har-monitor', {
    type: 'browser-monitor-artifacts',
    harPath,
    eventsPath,
    summaryPath,
    resourcesPath,
    targetUrl: cdp.targetInfo.url,
  });
  cdp.writeSessionMetadata?.({ lastHarPath: harPath, lastNetworkSummaryPath: summaryPath });

  console.log(`[METRIC] requests=${summary.counts.requests} failed=${summary.counts.failed} slow=${summary.counts.slow} exceptions=${summary.counts.exceptions} consoleErrors=${summary.counts.consoleErrors} longTasks=${summary.counts.longTasks}`);
  for (const failure of summary.failures.slice(0, MAX_STDOUT_ITEMS)) {
    console.log(`[NETWORK_ERROR] status=${failure.status} method=${failure.method} url=${failure.url}`);
  }
  for (const slow of summary.slow.slice(0, MAX_STDOUT_ITEMS)) {
    console.log(`[METRIC] slow status=${slow.status} method=${slow.method} ms=${slow.ms} type=${slow.type} url=${slow.url}`);
  }
  console.log(`[ARTIFACT] HAR ${harPath}`);
  console.log(`[ARTIFACT] EVENTS ${eventsPath}`);
  console.log(`[ARTIFACT] SUMMARY ${summaryPath}`);
  console.log(`[ARTIFACT] RESOURCE_TIMING ${resourcesPath}`);
}
