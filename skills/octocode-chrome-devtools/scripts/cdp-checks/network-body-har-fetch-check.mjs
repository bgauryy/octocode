import { writeFileSync } from 'fs';
import { join } from 'path';

const API_URL = 'https://example.test/api/data';
const BODY = JSON.stringify({ ok: true, items: [{ id: 1, name: 'alpha' }] });
// Live: BODY_URL=<page> loads a real page; BODY_MATCH=<substring> picks responses
// (default: XHR/Fetch or JSON). No BODY_URL runs the hermetic fixture.
const LIVE_URL = process.env.BODY_URL || '';
const MATCH = process.env.BODY_MATCH || '';
const WAIT_MS = Math.max(500, Math.min(30000, Number.parseInt(process.env.BODY_WAIT_MS ?? '3000', 10)));
const MAX_BODIES = 50;
const wanted = (record) => MATCH
  ? record.url.includes(MATCH)
  : (LIVE_URL ? ['XHR', 'Fetch'].includes(record.type) || /json/i.test(record.mimeType || '') : record.url.includes('/api/data'));

function harEntry(record, bodyText = '') {
  return {
    startedDateTime: new Date(record.start).toISOString(),
    time: Math.max(0, (record.end || Date.now()) - record.start),
    request: { method: record.method, url: record.url, httpVersion: 'HTTP/2', cookies: [], headers: [], queryString: [], headersSize: -1, bodySize: 0 },
    response: { status: record.status || 0, statusText: record.statusText || '', httpVersion: 'HTTP/2', cookies: [], headers: [], content: { size: bodyText.length, mimeType: record.mimeType || 'application/json', text: bodyText.slice(0, 2000) }, redirectURL: '', headersSize: -1, bodySize: bodyText.length },
    cache: {},
    timings: { blocked: -1, dns: -1, connect: -1, send: 0, wait: Math.max(0, (record.end || Date.now()) - record.start), receive: 0, ssl: -1 },
    _requestId: record.requestId
  };
}

export async function run(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  await cdp.send('Page.enable');
  if (!LIVE_URL) await cdp.send('Fetch.enable', { patterns: [{ urlPattern: '*example.test/api/data*', requestStage: 'Request' }] });

  const records = new Map();
  const bodies = [];
  const pending = [];
  if (!LIVE_URL) cdp.on('Fetch.requestPaused', async ({ requestId, request }) => {
    if (request.url.includes('/api/data')) {
      await cdp.send('Fetch.fulfillRequest', { requestId, responseCode: 200, responsePhrase: 'OK', responseHeaders: [{ name: 'content-type', value: 'application/json' }, { name: 'access-control-allow-origin', value: '*' }], body: Buffer.from(BODY).toString('base64') });
    } else {
      await cdp.send('Fetch.continueRequest', { requestId });
    }
  });
  cdp.on('Network.requestWillBeSent', ({ requestId, request, type }) => records.set(requestId, { requestId, url: request.url, method: request.method, type, start: Date.now() }));
  cdp.on('Network.responseReceived', ({ requestId, response }) => {
    const record = records.get(requestId);
    if (!record) return;
    record.status = response.status;
    record.statusText = response.statusText;
    record.mimeType = response.mimeType;
    record.end = Date.now();
  });
  // Bodies are complete only after loadingFinished.
  cdp.on('Network.loadingFinished', ({ requestId }) => {
    const record = records.get(requestId);
    if (!record?.status || !wanted(record) || bodies.length + pending.length >= MAX_BODIES) return;
    pending.push(cdp.send('Network.getResponseBody', { requestId }).then((body) => {
      bodies.push({ requestId, url: record.url, status: record.status, mimeType: record.mimeType, base64Encoded: body.base64Encoded, body: body.body });
      console.log(`[NETWORK_BODY] ${record.status} ${record.url} chars=${body.body.length}`);
    }).catch((error) => console.log(`[NETWORK_BODY_ERROR] ${record.url} ${error.message}`)));
  });

  const html = `<script>fetch('${API_URL}').then(r=>r.json()).then(j=>document.body.textContent=JSON.stringify(j)).catch(e=>document.body.textContent=e.message)</script>`;
  await cdp.send('Page.navigate', { url: LIVE_URL || `data:text/html,${encodeURIComponent(html)}` });
  await new Promise(r => setTimeout(r, LIVE_URL ? WAIT_MS : 1500));
  await Promise.allSettled(pending);
  const entries = [...records.values()].filter(r => r.status).map(r => harEntry(r, bodies.find(b => b.requestId === r.requestId)?.body || ''));
  const har = { log: { version: '1.2', creator: { name: 'octocode-chrome-devtools', version: '1' }, entries } };
  const harPath = join(cdp.outputDir, 'network-body.har');
  const bodiesPath = join(cdp.outputDir, 'network-bodies.json');
  writeFileSync(harPath, `${JSON.stringify(har, null, 2)}\n`, { mode: 0o600 });
  writeFileSync(bodiesPath, `${JSON.stringify(bodies, null, 2)}\n`, { mode: 0o600 });
  console.log(`[METRIC] HAR entries=${entries.length} bodies=${bodies.length}`);
  console.log(`[ARTIFACT] HAR ${harPath}`);
  console.log(`[ARTIFACT] NETWORK_BODIES ${bodiesPath}`);
}
