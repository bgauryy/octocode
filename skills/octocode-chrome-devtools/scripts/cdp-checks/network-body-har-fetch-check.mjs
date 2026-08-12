import { writeFileSync } from 'fs';
import { join } from 'path';

const API_URL = 'https://example.test/api/data';
const BODY = JSON.stringify({ ok: true, items: [{ id: 1, name: 'alpha' }] });

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
  await cdp.send('Fetch.enable', { patterns: [{ urlPattern: '*example.test/api/data*', requestStage: 'Request' }] });

  const records = new Map();
  const bodies = [];
  cdp.on('Fetch.requestPaused', async ({ requestId, request }) => {
    if (request.url.includes('/api/data')) {
      await cdp.send('Fetch.fulfillRequest', { requestId, responseCode: 200, responsePhrase: 'OK', responseHeaders: [{ name: 'content-type', value: 'application/json' }, { name: 'access-control-allow-origin', value: '*' }], body: Buffer.from(BODY).toString('base64') });
    } else {
      await cdp.send('Fetch.continueRequest', { requestId });
    }
  });
  cdp.on('Network.requestWillBeSent', ({ requestId, request }) => records.set(requestId, { requestId, url: request.url, method: request.method, start: Date.now() }));
  cdp.on('Network.responseReceived', async ({ requestId, response }) => {
    const record = records.get(requestId);
    if (!record) return;
    record.status = response.status;
    record.statusText = response.statusText;
    record.mimeType = response.mimeType;
    record.end = Date.now();
    if (response.url.includes('/api/data')) {
      try {
        const body = await cdp.send('Network.getResponseBody', { requestId });
        bodies.push({ requestId, url: response.url, base64Encoded: body.base64Encoded, body: body.body });
        console.log(`[NETWORK_BODY] ${response.status} ${response.url} chars=${body.body.length}`);
      } catch (error) {
        console.log(`[NETWORK_BODY_ERROR] ${response.url} ${error.message}`);
      }
    }
  });

  const html = `<script>fetch('${API_URL}').then(r=>r.json()).then(j=>document.body.textContent=JSON.stringify(j)).catch(e=>document.body.textContent=e.message)</script>`;
  await cdp.send('Page.navigate', { url: `data:text/html,${encodeURIComponent(html)}` });
  await new Promise(r => setTimeout(r, 1500));
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
