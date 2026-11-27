/**
 * Network conditions: loads NETCOND_URL under offline/latency/throughput limits, blocked URL
 * patterns, extra request headers and mock responses, and reports what happened per request.
 * The conditions belong to this CDP session and end when it closes.
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Payload, Session } from '../cdp-connection.mjs';
import { errorMessage } from '../error-message.mjs';
import {
  kbpsToBytes,
  parseConditions,
  urlMatches,
  type Conditions,
} from './net-conditions-options.mjs';

/** Request rows printed inline; the artifact keeps every row and `query` pages the rest. */
const PAGE_ROWS = 10;
/** How long a quiet network counts as settled once the load event has fired. */
const QUIET_MS = 400;

type Row = {
  url: string;
  method: string;
  outcome:
    'ok' | 'blocked' | 'mocked' | 'failed' | 'failed-offline' | 'pending';
  status: number | null;
  bytes: number | null;
  error?: string;
};

const out = (line: string) => {
  // oxlint-disable-next-line eslint/no-console -- stdout is the check's output protocol
  console.log(line);
};

export async function run(cdp: Session) {
  let c: Conditions;
  try {
    c = parseConditions(process.env);
  } catch (error) {
    out(
      `[FINDING] NETCOND_INVALID ${JSON.stringify({ error: errorMessage(error) })}`
    );
    process.exitCode = 2;
    return;
  }

  const rows = new Map<string, Row>();
  const mockedIds = new Set<string>();
  const sizes = new Map<string, number>();
  let inflight = 0;
  let lastActivity = Date.now();
  const settled = () => inflight <= 0 && Date.now() - lastActivity >= QUIET_MS;
  const touch = () => (lastActivity = Date.now());
  let consoleErrors = 0;
  let pageExceptions = 0;

  await cdp.send('Network.enable');
  await cdp.send('Page.enable');
  await cdp.send('Runtime.enable');
  await cdp.send('Log.enable').catch(() => {});

  cdp.on('Network.requestWillBeSent', (p: Payload) => {
    touch();
    if (p.redirectResponse) inflight--;
    inflight++;
    rows.set(p.requestId, {
      url: p.request.url,
      method: p.request.method,
      outcome: 'pending',
      status: null,
      bytes: null,
    });
  });
  cdp.on('Network.responseReceived', (p: Payload) => {
    touch();
    const row = rows.get(p.requestId);
    if (row) {
      row.status = p.response.status;
      row.outcome = mockedIds.has(p.requestId) ? 'mocked' : 'ok';
    }
  });
  cdp.on('Network.loadingFinished', (p: Payload) => {
    touch();
    inflight--;
    const row = rows.get(p.requestId);
    if (row) row.bytes = p.encodedDataLength;
    sizes.set(p.requestId, p.encodedDataLength);
  });
  cdp.on('Network.loadingFailed', (p: Payload) => {
    touch();
    inflight--;
    const row = rows.get(p.requestId);
    if (!row) return;
    row.error = p.errorText;
    row.outcome = p.blockedReason
      ? 'blocked'
      : c.offline && /INTERNET_DISCONNECTED/.test(p.errorText ?? '')
        ? 'failed-offline'
        : 'failed';
  });
  cdp.on('Runtime.consoleAPICalled', (p: Payload) => {
    if (p.type === 'error') consoleErrors++;
  });
  cdp.on('Runtime.exceptionThrown', () => pageExceptions++);

  // Apply every condition before the first navigation.
  await cdp.send('Network.emulateNetworkConditions', {
    offline: c.offline,
    latency: c.latencyMs < 0 ? 0 : c.latencyMs,
    downloadThroughput: kbpsToBytes(c.downKbps),
    uploadThroughput: kbpsToBytes(c.upKbps),
  });
  if (c.blockUrls.length)
    await cdp.send('Network.setBlockedURLs', { urls: c.blockUrls });
  if (Object.keys(c.extraHeaders).length)
    await cdp.send('Network.setExtraHTTPHeaders', { headers: c.extraHeaders });
  if (c.mocks.length) {
    cdp.on('Fetch.requestPaused', async (p: Payload) => {
      const mock = c.mocks.find(m => urlMatches(p.request.url, m.match));
      try {
        if (mock) {
          if (p.networkId) mockedIds.add(p.networkId);
          await cdp.send('Fetch.fulfillRequest', {
            requestId: p.requestId,
            responseCode: mock.status,
            responseHeaders: Object.entries(mock.headers).map(
              ([name, value]) => ({ name, value })
            ),
            body: Buffer.from(mock.body, 'utf8').toString('base64'),
          });
        } else {
          await cdp.send('Fetch.continueRequest', { requestId: p.requestId });
        }
      } catch {
        // The request ended (navigated away or blocked) before it could be answered.
      }
    });
    await cdp.send('Fetch.enable', {
      patterns: [{ urlPattern: '*', requestStage: 'Request' }],
    });
  }

  const loaded = new Promise<void>(resolve =>
    cdp.on('Page.loadEventFired', () => resolve())
  );
  const nav = await cdp.send('Page.navigate', { url: c.url });
  const loadReached = !nav.errorText;
  if (loadReached) {
    await loaded;
    // Requests started by the page's own scripts finish after the load event.
    while (!settled()) await new Promise(r => setTimeout(r, 50));
  }
  await new Promise(r => setTimeout(r, 100));

  const list = [...rows.values()];
  const byOutcome: Record<string, number> = {};
  for (const row of list)
    byOutcome[row.outcome] = (byOutcome[row.outcome] ?? 0) + 1;
  const page = {
    loadEventReached: loadReached,
    navigationError: nav.errorText ?? null,
    consoleErrors,
    pageExceptions,
  };
  const applied = {
    offline: c.offline,
    latencyMs: c.latencyMs < 0 ? null : c.latencyMs,
    downKbps: c.downKbps < 0 ? null : c.downKbps,
    upKbps: c.upKbps < 0 ? null : c.upKbps,
    blockUrls: c.blockUrls,
    extraHeaderNames: Object.keys(c.extraHeaders),
    mocks: c.mocks.map(m => ({ match: m.match, status: m.status })),
  };
  mkdirSync(cdp.outputDir, { recursive: true });
  const file = join(cdp.outputDir, 'network-conditions.json');
  writeFileSync(
    file,
    `${JSON.stringify({ url: c.url, applied, page, outcomes: byOutcome, requests: list, conditionsEnd: 'with this session' }, null, 2)}\n`,
    { mode: 0o600 }
  );

  out(
    `[METRIC] NETWORK_CONDITIONS requests=${list.length} ${Object.entries(
      byOutcome
    )
      .map(([k, v]) => `${k}=${v}`)
      .join(
        ' '
      )} loadEventReached=${loadReached} consoleErrors=${consoleErrors} pageExceptions=${pageExceptions}`
  );
  if (nav.errorText)
    out(
      `[FINDING] NETCOND_NAVIGATION_FAILED ${JSON.stringify({ errorText: nav.errorText })}`
    );
  for (const row of list.slice(0, PAGE_ROWS))
    out(`[REQUEST] ${JSON.stringify(row)}`);
  if (list.length > PAGE_ROWS) {
    out(
      `[REQUEST] ${PAGE_ROWS} of ${list.length} requests shown; every row is in the artifact`
    );
    out(
      `[NEXT] ${JSON.stringify({
        command: process.execPath,
        args: [
          join(
            dirname(fileURLToPath(import.meta.url)),
            '..',
            'evidence-query.mjs'
          ),
          '--file',
          file,
          '--pointer',
          '/requests',
          '--cursor',
          String(PAGE_ROWS),
          '--limit',
          String(PAGE_ROWS),
        ],
      })}`
    );
  }
  out(`[ARTIFACT] NETWORK_CONDITIONS ${file}`);
  out(
    '[NOTE] CONDITIONS_END_WITH_SESSION these conditions applied to this check run only; they end when its CDP session closes and do not affect later calls'
  );
}
