/**
 * Network measure: classify requests, failures, slow calls, by resource type.
 * Hermetic: Fetch-mocks an API + navigates a fixture. Live: MEASURE_URL or MEASURE_EXISTING=1.
 */
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { observeFrameEvents } from '../frame-events.mjs';

const SLOW_MS = Number(process.env.NET_SLOW_MS || 1000);
const API_URL = 'https://example.test/api/measure';
const API_BODY = JSON.stringify({ ok: true, id: 'net-measure' });
const FIXTURE = `data:text/html,${encodeURIComponent(`<!doctype html><script>
fetch('${API_URL}').then(r=>r.json()).then(j=>{document.title='ok';document.body.textContent=JSON.stringify(j)}).catch(e=>{document.body.textContent=e.message});
fetch('https://example.test/missing').catch(()=>{});
</script><body>network-measure</body>`)}`;

function classify(url = '') {
  try {
    const u = new URL(url);
    const path = u.pathname.toLowerCase();
    if (/\.(js|mjs)(\?|$)/.test(path)) return 'script';
    if (/\.(css)(\?|$)/.test(path)) return 'stylesheet';
    if (/\.(png|jpe?g|gif|webp|svg|ico)(\?|$)/.test(path)) return 'image';
    if (/\.(woff2?|ttf|otf)(\?|$)/.test(path)) return 'font';
    if (u.protocol === 'data:') return 'data';
    if (path.includes('/api/') || u.search.includes('graphql')) return 'api';
    return 'other';
  } catch {
    return 'other';
  }
}

export async function run(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('Network.enable');
  await cdp.send('Page.enable');
  await cdp.send('Log.enable');

  const existing = process.env.MEASURE_EXISTING === '1';
  const measureUrl = process.env.MEASURE_URL || null;
  const useFixture = !existing && !measureUrl;

  if (useFixture) {
    await cdp.send('Fetch.enable', {
      patterns: [
        { urlPattern: '*example.test/api/measure*', requestStage: 'Request' },
        { urlPattern: '*example.test/missing*', requestStage: 'Request' },
      ],
    });
    cdp.on('Fetch.requestPaused', async ({ requestId, request }) => {
      if (request.url.includes('/api/measure')) {
        await cdp.send('Fetch.fulfillRequest', {
          requestId,
          responseCode: 200,
          responsePhrase: 'OK',
          responseHeaders: [
            { name: 'content-type', value: 'application/json' },
            { name: 'access-control-allow-origin', value: '*' },
          ],
          body: Buffer.from(API_BODY).toString('base64'),
        });
      } else if (request.url.includes('/missing')) {
        await cdp.send('Fetch.fulfillRequest', {
          requestId,
          responseCode: 404,
          responsePhrase: 'Not Found',
          responseHeaders: [{ name: 'content-type', value: 'text/plain' }, { name: 'access-control-allow-origin', value: '*' }],
          body: Buffer.from('missing').toString('base64'),
        });
      } else {
        await cdp.send('Fetch.continueRequest', { requestId });
      }
    });
  }

  const records = new Map();
  const requestKey = (requestId, meta) => JSON.stringify([meta.sessionId || null, requestId]);
  const history = [];
  const startedAt = Date.now();
  const finish = (rec, timestamp) => {
    rec.end = Date.now();
    rec.ms = Math.max(0, timestamp != null && rec.timestamp != null ? Math.round((timestamp - rec.timestamp) * 1000) : rec.end - rec.start);
    rec.complete = true;
  };
  cdp.on('Network.requestWillBeSent', ({ requestId, request, type, initiator, timestamp, redirectResponse, frameId }, meta = {}) => {
    const prior = records.get(requestKey(requestId, meta));
    if (prior && redirectResponse) {
      prior.status = redirectResponse.status;
      prior.redirectURL = request.url;
      finish(prior, timestamp);
      history.push(prior);
    }
    records.set(requestKey(requestId, meta), { requestId, hop: prior ? prior.hop + 1 : 0, url: request.url, method: request.method,
      type: type || 'Other', kind: classify(request.url), initiator: initiator?.type || null,
      frameId, sessionId: meta.sessionId || null, start: Date.now(), timestamp, complete: false });
  });
  cdp.on('Network.responseReceived', ({ requestId, response, type, timestamp }, meta = {}) => {
    const rec = records.get(requestKey(requestId, meta));
    if (!rec) return;
    rec.status = response.status; rec.mimeType = response.mimeType; rec.type = type || rec.type;
    rec.headersMs = timestamp != null && rec.timestamp != null ? Math.round((timestamp - rec.timestamp) * 1000) : Date.now() - rec.start;
  });
  cdp.on('Network.loadingFinished', ({ requestId, timestamp, encodedDataLength }, meta = {}) => {
    const rec = records.get(requestKey(requestId, meta));
    if (rec) { finish(rec, timestamp); rec.encodedDataLength = encodedDataLength; }
  });
  cdp.on('Network.loadingFailed', ({ requestId, errorText, blockedReason, timestamp }, meta = {}) => {
    const rec = records.get(requestKey(requestId, meta)) || { requestId, hop: 0, url: 'unknown', method: 'unknown', start: Date.now() };
    rec.failed = true; rec.errorText = errorText; rec.blockedReason = blockedReason;
    finish(rec, timestamp); records.set(requestKey(requestId, meta), rec);
  });
  const progress = setInterval(() => console.log(`[PROGRESS] NETWORK elapsedMs=${Date.now() - startedAt} requests=${records.size + history.length} pending=${[...records.values()].filter(r => !r.complete).length}`), 1000);

  const frameEvents = await observeFrameEvents(cdp);
  const navUrl = measureUrl || (useFixture ? FIXTURE : null);
  try {
  if (navUrl) {
    const navigation = await cdp.send('Page.navigate', { url: navUrl });
    if (navigation.errorText) throw new Error(navigation.errorText);
    await new Promise(r => setTimeout(r, Number(process.env.NET_WAIT_MS || 1500)));
  } else {
    await new Promise(r => setTimeout(r, Number(process.env.NET_WAIT_MS || 2000)));
  }

  } finally { clearInterval(progress); await frameEvents.stop(); }
  const rows = [...history, ...records.values()];
  for (const row of rows) if (!row.complete) row.ms = Date.now() - row.start;
  const failures = rows.filter(r => r.failed || r.status >= 400);
  const pending = rows.filter(r => !r.complete);
  const slow = rows.filter(r => (r.ms || 0) >= SLOW_MS && !r.failed);
  const byKind = {};
  const byStatus = {};
  for (const r of rows) {
    byKind[r.kind] = (byKind[r.kind] || 0) + 1;
    const key = r.failed ? 'failed' : String(r.status || 0);
    byStatus[key] = (byStatus[key] || 0) + 1;
  }

  const findings = [];
  if (failures.length) findings.push({ code: 'HTTP_FAILURES', count: failures.length });
  if (slow.length) findings.push({ code: 'SLOW_REQUESTS', count: slow.length, thresholdMs: SLOW_MS });
  const apiFails = failures.filter(f => f.kind === 'api' || /\/api\//.test(f.url || ''));
  if (apiFails.length) findings.push({ code: 'API_FAILURES', count: apiFails.length });

  let health = 100;
  health -= Math.min(50, failures.length * 15);
  health -= Math.min(30, slow.length * 5);
  health = Math.max(0, Math.round(health));

  const summary = {
    url: cdp.targetInfo?.url || null,
    counts: { requests: rows.length, failed: failures.length, slow: slow.length, pending: pending.length },
    coverage: { mode: existing ? 'observation-window' : 'navigation-window', completed: rows.length - pending.length, pending: pending.length, redirects: history.length, monitorMs: Date.now() - startedAt, iframeEvents: frameEvents.coverage() },
    byKind,
    byStatus,
    health: rows.length && !pending.length && !frameEvents.coverage().errors.length ? health : null,
    findings,
    failures: failures.map(f => ({
      status: f.status || 0,
      method: f.method,
      url: f.url,
      ms: f.ms,
      kind: f.kind,
      errorText: f.errorText || null,
      blockedReason: f.blockedReason || null,
    })),
    slow: slow.map(s => ({ status: s.status, method: s.method, url: s.url, ms: s.ms, kind: s.kind })),
    sample: rows.map(r => ({
      status: r.status || 0,
      method: r.method,
      url: r.url,
      ms: r.ms || 0,
      kind: r.kind,
      type: r.type,
      requestId: r.requestId,
      frameId: r.frameId,
      sessionId: r.sessionId,
      hop: r.hop,
      complete: r.complete,
      headersMs: r.headersMs,
      encodedDataLength: r.encodedDataLength,
      redirectURL: r.redirectURL,
    })),
    collectedAt: new Date().toISOString(),
  };

  if (pending.length || !rows.length) console.log(`[FINDING] NET_INCOMPLETE pending=${pending.length} observed=${rows.length}; observation window does not prove full-page coverage`);
  const artifact = join(cdp.outputDir, 'network-measure.json');
  writeFileSync(artifact, `${JSON.stringify(summary, null, 2)}\n`, { mode: 0o600 });

  console.log(`[METRIC] NET health=${summary.health ?? 'unavailable'} requests=${rows.length} failed=${failures.length} slow=${slow.length} kinds=${JSON.stringify(byKind)}`);
  for (const f of findings.slice(0, 8)) console.log(`[FINDING] NET_${f.code} ${JSON.stringify(f)}`);
  for (const f of failures.slice(0, 5)) {
    console.log(`[NETWORK_ERROR] status=${f.status || 0} method=${f.method} url=${f.url}`);
  }
  for (const s of slow.slice(0, 5)) {
    console.log(`[METRIC] slow status=${s.status} method=${s.method} ms=${s.ms} kind=${s.kind} url=${s.url}`);
  }
  console.log(`[ARTIFACT] NETWORK_MEASURE ${artifact}`);
}
