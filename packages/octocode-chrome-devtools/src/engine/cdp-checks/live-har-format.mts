// What live-har-monitor writes: redacted HAR 1.2 entries, the network summary, and the page's
// performance snapshot (navigation timing, resource timing and long tasks).

/** The URL without credentials; secret-looking query values are redacted. */
export function safeUrl(raw) {
  try {
    const url = new URL(raw);
    url.username = '';
    url.password = '';
    // A snapshot of the keys: set() rewrites the list being redacted.
    const keys = Array.from(url.searchParams.keys());
    for (const key of keys) {
      if (/token|key|secret|session|auth|password/i.test(key))
        url.searchParams.set(key, '[REDACTED]');
    }
    return url.href;
  } catch {
    return String(raw ?? '');
  }
}

// CDP Network.Headers: header name to value, stringified for HAR.
function headerPairs(headers: Record<string, string | number> = {}) {
  return Object.entries(headers)
    .filter(
      ([name]) =>
        !/cookie|authorization|proxy-authorization|x-api-key|token|secret/i.test(
          name
        )
    )
    .map(([name, value]) => ({ name, value: String(value) }));
}

function mimeToContent(mimeType = '') {
  return {
    size: -1,
    mimeType: mimeType || 'application/octet-stream',
    text: '',
    comment: 'response body not captured',
  };
}

const durationOf = record =>
  Math.max(0, (record.complete ? record.end : Date.now()) - record.start);

function queryString(url) {
  try {
    return [...new URL(url).searchParams.entries()].map(([name, value]) => ({
      name,
      value,
    }));
  } catch {
    return [];
  }
}

function harRequest(record, url) {
  const request = record.request ?? {};
  return {
    method: request.method ?? record.method ?? 'GET',
    url,
    httpVersion: 'HTTP/2',
    cookies: [],
    headers: headerPairs(request.headers),
    queryString: queryString(url),
    headersSize: -1,
    bodySize: request.postData ? String(request.postData).length : 0,
  };
}

function harResponse(record) {
  const response = record.response ?? {};
  return {
    status: response.status ?? (record.failed ? 0 : -1),
    statusText: response.statusText ?? record.errorText ?? '',
    httpVersion: response.protocol ?? 'HTTP/2',
    cookies: [],
    headers: headerPairs(response.headers),
    content: mimeToContent(response.mimeType),
    redirectURL: response.headers?.location ?? '',
    headersSize: -1,
    bodySize: response.encodedDataLength ?? record.encodedDataLength ?? -1,
  };
}

// HAR custom fields: CDP identity and failure detail the standard fields cannot hold.
function harExtensions(record) {
  return {
    _resourceType: record.type ?? 'Other',
    _encodedDataLength: record.encodedDataLength ?? null,
    _requestId: record.requestId,
    _frameId: record.frameId ?? null,
    _sessionId: record.sessionId ?? null,
    _pending: !record.complete,
    _failed: Boolean(record.failed),
    _errorText: record.errorText ?? null,
    _blockedReason: record.blockedReason ?? null,
    _initiator: record.initiator?.type ?? null,
  };
}

function toHarEntry(record) {
  const duration = durationOf(record);
  const url = safeUrl(record.request?.url ?? record.url);
  return {
    startedDateTime: new Date(
      record.startWallTime ?? record.start
    ).toISOString(),
    time: duration,
    request: harRequest(record, url),
    response: harResponse(record),
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
    _timingsEstimated: true,
    ...harExtensions(record),
  };
}

/** The HAR 1.2 log of every observed request, with one page for the monitored window. */
export function harDocument(
  records,
  { performanceSnapshot, monitorMs, title }
) {
  return {
    log: {
      version: '1.2',
      creator: {
        name: 'octocode-chrome-devtools live-har-monitor',
        version: '1.0.0',
      },
      browser: { name: 'Chrome', version: 'CDP' },
      pages: [
        {
          startedDateTime: new Date(Date.now() - monitorMs).toISOString(),
          id: 'live-page',
          title,
          pageTimings: {
            onContentLoad:
              performanceSnapshot.navigationTiming?.domContentLoaded ?? -1,
            onLoad: performanceSnapshot.navigationTiming?.load ?? -1,
          },
        },
      ],
      entries: [...records.values()].map(toHarEntry),
    },
  };
}

const slowRow = r => ({
  url: safeUrl(r.request?.url ?? r.url),
  method: r.request?.method ?? r.method,
  status: r.response?.status ?? (r.failed ? 0 : -1),
  ms: durationOf(r),
  type: r.type ?? 'Other',
});

const failureRow = r => ({
  url: safeUrl(r.request?.url ?? r.url),
  method: r.request?.method ?? r.method,
  status: r.response?.status ?? 0,
  errorText: r.errorText ?? null,
  blockedReason: r.blockedReason ?? null,
});

function eventCounts(events) {
  return {
    exceptions: events.filter(e => e.kind === 'exception').length,
    consoleErrors: events.filter(
      e => e.kind === 'console' && e.type === 'error'
    ).length,
  };
}

/** Counts, failures and slow requests (slowest first) over the monitored window. */
export function summarize(
  records,
  { events, resourceEntries, performanceSnapshot, slowMs, monitorMs }
) {
  const completed = [...records.values()].filter(r => r.complete);
  const failed = [...records.values()].filter(
    r => r.failed || (r.response?.status ?? 0) >= 400
  );
  const slow = completed
    .map(slowRow)
    .filter(r => r.ms >= slowMs)
    .toSorted((a, b) => b.ms - a.ms);
  return {
    capturedAt: new Date().toISOString(),
    monitorMs,
    target: performanceSnapshot?.location ?? null,
    counts: {
      requests: records.size,
      completed: completed.length,
      pending: records.size - completed.length,
      failed: failed.length,
      slow: slow.length,
      ...eventCounts(events),
      resourceEntries: resourceEntries.length,
      longTasks: performanceSnapshot?.longTasks?.length ?? 0,
    },
    pageTiming: performanceSnapshot?.navigationTiming ?? null,
    vitalsApprox: performanceSnapshot?.vitalsApprox ?? null,
    failures: failed.map(failureRow),
    slow,
  };
}

/** Starts collecting long tasks in the page, once per document. */
export const LONG_TASK_OBSERVER_JS = `(() => {
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
          });
          observer.observe({ type: 'longtask', buffered: true });
        } catch {}
      }
    })()`;

const PERFORMANCE_SNAPSHOT_JS = `(() => {
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
        .sort((a, b) => b.duration - a.duration);
      const longTasks = (globalThis.__octocodeLongTasks || []);
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
    })()`;

/** The page's navigation and resource timing (slowest first) and collected long tasks. */
export async function collectPerformanceSnapshot(cdp) {
  const result = await cdp.send('Runtime.evaluate', {
    returnByValue: true,
    expression: PERFORMANCE_SNAPSHOT_JS,
  });
  return result.result?.value ?? { resources: [], longTasks: [] };
}
