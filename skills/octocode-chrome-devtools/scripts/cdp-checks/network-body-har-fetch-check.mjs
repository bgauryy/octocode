import { writeFileSync } from 'fs';
import { join } from 'path';
import { observeFrameEvents } from '../frame-events.mjs';

const API_URL = 'https://example.test/api/data';
const BODY = JSON.stringify({ ok: true, items: [{ id: 1, name: 'alpha' }] });
// Live: BODY_URL=<page> loads a real page; BODY_MATCH=<substring> picks responses
// (default: XHR/Fetch or JSON). No BODY_URL runs the hermetic fixture.
const LIVE_URL = process.env.BODY_URL || '';
const MATCH = process.env.BODY_MATCH || '';
const WAIT_MS = Math.max(500, Math.min(30000, Number.parseInt(process.env.BODY_WAIT_MS ?? '3000', 10)));
const wanted = (record) => MATCH
  ? record.url.includes(MATCH)
  : (LIVE_URL ? ['XHR', 'Fetch'].includes(record.type) || /json/i.test(record.mimeType || '') : record.url.includes('/api/data'));

function harEntry(record, captured = {}) {
  const bodyText = captured.body || '';
  const size = Buffer.byteLength(bodyText, captured.base64Encoded ? 'base64' : 'utf8');
  return {
    startedDateTime: new Date(record.start).toISOString(),
    time: Math.max(0, (record.end || Date.now()) - record.start),
    request: { method: record.method, url: record.url, httpVersion: record.protocol || '', cookies: [], headers: [], queryString: [], headersSize: -1, bodySize: 0 },
    response: { status: record.status || 0, statusText: record.statusText || '', httpVersion: record.protocol || '', cookies: [], headers: [], content: { size, ...(captured.base64Encoded ? { encoding: 'base64' } : {}), mimeType: record.mimeType || 'application/json', text: bodyText }, redirectURL: '', headersSize: -1, bodySize: size },
    cache: {},
    timings: { blocked: -1, dns: -1, connect: -1, send: 0, wait: Math.max(0, (record.end || Date.now()) - record.start), receive: 0, ssl: -1 },
    _requestId: record.requestId,
    _frameId: record.frameId,
    _sessionId: record.sessionId,
    _pending: !record.complete,
    _bodyError: captured.error || null,
    _errorText: record.errorText || null,
    _timingsEstimated: true
  };
}

export async function run(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  await cdp.send('Page.enable');
  if (!LIVE_URL) await cdp.send('Fetch.enable', { patterns: [{ urlPattern: '*example.test/api/data*', requestStage: 'Request' }] });

  const records = new Map();
  const requestKey = (requestId, meta) => JSON.stringify([meta.sessionId || null, requestId]);
  const redirects = [];
  const bodies = [];
  const pending = [];
  if (!LIVE_URL) cdp.on('Fetch.requestPaused', async ({ requestId, request }) => {
    if (request.url.includes('/api/data')) {
      await cdp.send('Fetch.fulfillRequest', { requestId, responseCode: 200, responsePhrase: 'OK', responseHeaders: [{ name: 'content-type', value: 'application/json' }, { name: 'access-control-allow-origin', value: '*' }], body: Buffer.from(BODY).toString('base64') });
    } else {
      await cdp.send('Fetch.continueRequest', { requestId });
    }
  });
  cdp.on('Network.requestWillBeSent', ({ requestId, request, type, frameId, redirectResponse }, meta = {}) => {
    const prior = records.get(requestKey(requestId, meta));
    if (prior && redirectResponse) { prior.status = redirectResponse.status; prior.end = Date.now(); prior.complete = true; redirects.push(prior); }
    records.set(requestKey(requestId, meta), { requestId, frameId, sessionId: meta.sessionId, url: request.url, method: request.method, type, start: Date.now(), complete: false });
  });
  cdp.on('Network.responseReceived', ({ requestId, response }, meta = {}) => {
    const record = records.get(requestKey(requestId, meta));
    if (!record) return;
    record.status = response.status;
    record.statusText = response.statusText;
    record.mimeType = response.mimeType;
    record.protocol = response.protocol;
    record.headersReceived = Date.now();
  });
  // Bodies are complete only after loadingFinished.
  cdp.on('Network.loadingFinished', ({ requestId }, meta = {}) => {
    const record = records.get(requestKey(requestId, meta));
    if (record) { record.end = Date.now(); record.complete = true; }
    if (!record?.status || !wanted(record)) return;
    pending.push(cdp.send('Network.getResponseBody', { requestId }, record.sessionId).then((body) => {
      bodies.push({ requestId, sessionId: record.sessionId, url: record.url, status: record.status, mimeType: record.mimeType, base64Encoded: body.base64Encoded, body: body.body });
      console.log(`[NETWORK_BODY] ${record.status} ${record.url} chars=${body.body.length}`);
    }).catch(error => { bodies.push({ requestId, sessionId: record.sessionId, url: record.url, error: error.message }); console.log(`[NETWORK_BODY_ERROR] ${record.url} ${error.message}`); }));
  });

  cdp.on('Network.loadingFailed', ({ requestId, errorText }, meta = {}) => { const record = records.get(requestKey(requestId, meta)); if (record) { record.end = Date.now(); record.complete = true; record.errorText = errorText; } });
  const frameEvents = await observeFrameEvents(cdp);
  const html = `<script>fetch('${API_URL}').then(r=>r.json()).then(j=>document.body.textContent=JSON.stringify(j)).catch(e=>document.body.textContent=e.message)</script>`;
  await cdp.send('Page.navigate', { url: LIVE_URL || `data:text/html,${encodeURIComponent(html)}` });
  await new Promise(r => setTimeout(r, LIVE_URL ? WAIT_MS : 1500));
  await Promise.allSettled(pending);
  await frameEvents.stop();
  const entries = [...redirects, ...records.values()].map(r => harEntry(r, bodies.find(b => b.requestId === r.requestId && b.sessionId === r.sessionId) || {}));
  const har = { coverage: { iframeEvents: frameEvents.coverage(), pending: entries.filter(e => e._pending).length }, log: { version: '1.2', creator: { name: 'octocode-chrome-devtools', version: '1' }, entries } };
  const harPath = join(cdp.outputDir, 'network-body.har');
  const bodiesPath = join(cdp.outputDir, 'network-bodies.json');
  writeFileSync(harPath, `${JSON.stringify(har, null, 2)}\n`, { mode: 0o600 });
  writeFileSync(bodiesPath, `${JSON.stringify(bodies, null, 2)}\n`, { mode: 0o600 });
  console.log(`[METRIC] HAR entries=${entries.length} bodies=${bodies.length}`);
  console.log(`[ARTIFACT] HAR ${harPath}`);
  console.log(`[ARTIFACT] NETWORK_BODIES ${bodiesPath}`);
}
