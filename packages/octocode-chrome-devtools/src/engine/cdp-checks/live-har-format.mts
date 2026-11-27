// What live-har-monitor writes: redacted HAR 1.2 entries, the network summary, and the page's
// performance snapshot (navigation timing, resource timing and long tasks).
import type { Session } from '../cdp-connection.mjs';
import {
  CREDENTIAL_NAME,
  cookieRows,
  harCookies,
  headerCookieRows,
  mergeHeaders,
  type CookieRow,
} from './net-headers.mjs';

type Headers = Record<string, string | number>;

/** What requestWillBeSentExtraInfo adds to one request hop: the wire headers and the cookies considered. */
export type RequestExtra = { headers?: Headers; associatedCookies?: unknown };
/** What responseReceivedExtraInfo adds to one response hop: the wire headers (Set-Cookie) and blocked cookies. */
export type ResponseExtra = {
  headers?: Headers;
  statusCode?: number;
  blockedCookies?: unknown;
};
/** The extra-info events of one requestId, in arrival order: one entry per redirect hop. */
export type ExtraInfo = { request: RequestExtra[]; response: ResponseExtra[] };

/** One request live-har-monitor observed, built from CDP Network events. */
export type NetworkRecord = {
  requestId: string;
  frameId?: string;
  sessionId?: string | null;
  complete?: boolean;
  failed?: boolean;
  request?: {
    method?: string;
    url?: string;
    headers?: Headers;
    postData?: string;
  };
  /** Extra-info events of this requestId (shared by every redirect hop) and this record's hop number. */
  extras?: ExtraInfo;
  hop?: number;
  method: string;
  url: string;
  start: number;
  end?: number;
  startWallTime?: number;
  timestamp?: number;
  initiator?: { type?: string };
  type?: string;
  response?: {
    status?: number;
    statusText?: string;
    protocol?: string;
    headers?: Headers;
    mimeType?: string;
    encodedDataLength?: number;
  };
  encodedDataLength?: number;
  errorText?: string;
  blockedReason?: string;
};

/** A console or exception event the monitor recorded. */
export type MonitorEvent = {
  kind: string;
  type?: string;
  [field: string]: unknown;
};

/** What PERFORMANCE_SNAPSHOT_JS returns from the page. */
export type PerformanceSnapshot = {
  location?: string;
  title?: string;
  navigationTiming?: { domContentLoaded?: number; load?: number } | null;
  vitalsApprox?: unknown;
  resources?: unknown[];
  longTasks?: unknown[];
};

/** The URL without credentials; secret-looking query values are redacted. */
export function safeUrl(raw: string | undefined) {
  try {
    const url = new URL(raw ?? '');
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
    return raw ?? '';
  }
}

// CDP Network.Headers: header name to value, stringified for HAR. Credential headers keep their
// name; the value is replaced by its length.
function headerPairs(headers: Headers = {}) {
  return Object.entries(headers).map(([name, raw]) => {
    const value = String(raw);
    return {
      name,
      value: CREDENTIAL_NAME.test(name)
        ? `[redacted ${value.length} chars]`
        : value,
    };
  });
}

/** The extra-info of this record's hop; a response extra whose status disagrees belongs to another hop. */
function hopExtras(record: NetworkRecord) {
  const hop = record.hop ?? 0;
  const request = record.extras?.request[hop];
  const response = record.extras?.response[hop];
  const status = record.response?.status;
  return {
    request,
    response:
      response &&
      (status === undefined ||
        response.statusCode === undefined ||
        response.statusCode === status)
        ? response
        : undefined,
  };
}

/** Cookie names this hop sent and set, with domain, path and block reasons; never values. */
function cookieEvidence(record: NetworkRecord): CookieRow[] {
  const { request, response } = hopExtras(record);
  const sent = request?.associatedCookies
    ? cookieRows('request', request.associatedCookies)
    : headerCookieRows(
        'request',
        mergeHeaders(record.request?.headers, request?.headers)
      );
  const blocked = cookieRows('response', response?.blockedCookies);
  const set = headerCookieRows(
    'response',
    mergeHeaders(record.response?.headers, response?.headers)
  ).filter(row => !blocked.some(other => other.name === row.name));
  return [...sent, ...set, ...blocked];
}

function mimeToContent(mimeType = '') {
  return {
    size: -1,
    mimeType: mimeType || 'application/octet-stream',
    text: '',
    comment: 'response body not captured',
  };
}

// A complete record always carries its end time; an open one runs until now.
const durationOf = (record: NetworkRecord) =>
  Math.max(
    0,
    ((record.complete ? record.end : undefined) ?? Date.now()) - record.start
  );

function queryString(url: string) {
  try {
    return [...new URL(url).searchParams.entries()].map(([name, value]) => ({
      name,
      value,
    }));
  } catch {
    return [];
  }
}

function harRequest(record: NetworkRecord, url: string) {
  const request = record.request ?? {};
  const headers = mergeHeaders(
    request.headers,
    hopExtras(record).request?.headers
  );
  return {
    method: request.method ?? record.method ?? 'GET',
    url,
    httpVersion: 'HTTP/2',
    cookies: harCookies('request', headers),
    headers: headerPairs(headers),
    queryString: queryString(url),
    headersSize: -1,
    bodySize: request.postData ? request.postData.length : 0,
  };
}

function harResponse(record: NetworkRecord) {
  const response = record.response ?? {};
  const headers = mergeHeaders(
    response.headers,
    hopExtras(record).response?.headers
  );
  return {
    status: response.status ?? (record.failed ? 0 : -1),
    statusText: response.statusText ?? record.errorText ?? '',
    httpVersion: response.protocol ?? 'HTTP/2',
    cookies: harCookies('response', headers),
    headers: headerPairs(headers),
    content: mimeToContent(response.mimeType),
    redirectURL: response.headers?.location ?? '',
    headersSize: -1,
    bodySize: response.encodedDataLength ?? record.encodedDataLength ?? -1,
  };
}

// HAR custom fields: CDP identity and failure detail the standard fields cannot hold.
function harExtensions(record: NetworkRecord) {
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
    _cookies: cookieEvidence(record),
  };
}

function toHarEntry(record: NetworkRecord) {
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
  records: Map<string, NetworkRecord>,
  {
    performanceSnapshot,
    monitorMs,
    title,
  }: {
    performanceSnapshot: PerformanceSnapshot;
    monitorMs: number;
    title: string;
  }
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

const slowRow = (r: NetworkRecord) => ({
  url: safeUrl(r.request?.url ?? r.url),
  method: r.request?.method ?? r.method,
  status: r.response?.status ?? (r.failed ? 0 : -1),
  ms: durationOf(r),
  type: r.type ?? 'Other',
});

const failureRow = (r: NetworkRecord) => ({
  url: safeUrl(r.request?.url ?? r.url),
  method: r.request?.method ?? r.method,
  status: r.response?.status ?? 0,
  errorText: r.errorText ?? null,
  blockedReason: r.blockedReason ?? null,
});

function eventCounts(events: MonitorEvent[]) {
  return {
    exceptions: events.filter(e => e.kind === 'exception').length,
    consoleErrors: events.filter(
      e => e.kind === 'console' && e.type === 'error'
    ).length,
  };
}

/** Counts, failures and slow requests (slowest first) over the monitored window. */
export function summarize(
  records: Map<string, NetworkRecord>,
  {
    events,
    resourceEntries,
    performanceSnapshot,
    slowMs,
    monitorMs,
  }: {
    events: MonitorEvent[];
    resourceEntries: unknown[];
    performanceSnapshot: PerformanceSnapshot;
    slowMs: number;
    monitorMs: number;
  }
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
export async function collectPerformanceSnapshot(
  cdp: Session
): Promise<PerformanceSnapshot> {
  const result = await cdp.send('Runtime.evaluate', {
    returnByValue: true,
    expression: PERFORMANCE_SNAPSHOT_JS,
  });
  return result.result?.value ?? { resources: [], longTasks: [] };
}
