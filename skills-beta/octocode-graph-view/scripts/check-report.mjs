#!/usr/bin/env node
// Round trip: screenshot the finished report, OCR its side panel, and compare with the data the page was built from.
import { execFile, execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chrome, crop, flags, pool } from './lib.mjs';
import { HINTS } from './hints.mjs';
import { STRIP_TOKENS, createSession, deleteSession, ocrRoute } from './freellama.mjs';

const USAGE = `usage: check-report.mjs --data <data.json> [--tabs insights,evidence]
  Needs report.html beside data.json (render.mjs, run after verify.mjs). Screenshots each tab at 1600x2200 so the whole panel is visible,
  crops the side panel, reads it with OCR and checks that headings, verdicts and key names appear.
  Writes report-check.json. Exit 1 when expected text is unread; without OCR it reports a skip and how to enable it (exit 0).`;

const argv = process.argv.slice(2);
const f = flags(argv, { data: '', tabs: 'insights,evidence' });
if (!f.data) {
  console.error(USAGE);
  process.exit(2);
}
const dataPath = resolve(f.data);
const out = dirname(dataPath);
const D = JSON.parse(readFileSync(dataPath, 'utf8'));
const V = D.verify ?? (() => { try { return JSON.parse(readFileSync(join(out, 'verify.json'), 'utf8')); } catch { return null; } })();
const short = (id) => id.split('/').slice(-2).join('/');
const ocrScript = join(dirname(fileURLToPath(import.meta.url)), 'ocr.mjs');

const expectations = {
  insights: () => ['Key insights', 'Health', 'Hubs', `Import cycles (${D.insights.cycles.total})`, ...D.insights.hubs.slice(0, 3).map((h) => short(h.id))],
  evidence: () => V ? ['Leads', ...(V.connections ? ['Connections'] : []), ...(V.history ? ['History'] : []), 'Render fidelity', 'Layers run', ...new Set(V.leads.map((l) => l.verdict)), ...new Set(V.leads.map((l) => l.detector)), ...V.leads.slice(0, 3).map((l) => short(l.file))] : [],
};
// One FreeLlama affinity session for every strip of this check; raw Ollama has none.
const route = await ocrRoute();
const session = route.via === 'freellama' ? await createSession(route.endpoint) : null;
const runOcr = (image, labels) => new Promise((res) => {
  if (route.via === 'none') return res({ error: `OCR unavailable: ${route.reason}` });
  const exp = join(mkdtempSync(join(tmpdir(), 'gv-')), 'expected.json');
  writeFileSync(exp, JSON.stringify({ labels }));
  execFile(process.execPath, [ocrScript, image, '--expect', exp, '--max-tokens', String(STRIP_TOKENS), '--via', route.via, '--endpoint', route.endpoint, '--tag', route.model, ...(session ? ['--session', session] : [])], { maxBuffer: 64 << 20 }, (err, stdout, stderr) => {
    if (err && err.code !== 1) return res({ error: (stderr || err.message).trim().slice(0, 200) });
    res(JSON.parse(stdout));
  });
});

const result = {};
for (const tab of f.tabs.split(',')) {
  const shot = join(out, `page-${tab}.png`);
  execFileSync(chrome(), ['--headless=new', '--disable-gpu', '--hide-scrollbars', '--window-size=1600,2200', '--virtual-time-budget=8000', `--screenshot=${shot}`, `file://${join(out, 'report.html')}#tab=${tab}`], { stdio: 'ignore' });
  // Dense small text makes the OCR model loop, so read the panel in overlapping strips and merge.
  const expected = expectations[tab]?.() ?? [];
  const strips = Array.from({ length: 6 }, (_, i) => { const dst = join(out, `panel-${tab}-${i + 1}.png`); crop(shot, { x: 1200, y: 90 + i * 350, w: 400, h: 370 }, dst, 2); return dst; });
  // Two strips in flight match the sidecar's vision admission width; more only sit in its queue.
  const tabStart = Date.now();
  const reads = await pool(strips, 2, (p) => runOcr(p, expected));
  const tabMs = Date.now() - tabStart;
  const ok = reads.filter((r) => !r.error);
  const found = new Set(ok.flatMap((r) => r.found));
  const unavailable = reads.every((r) => r.error && /unavailable|unreachable|not found|model/i.test(r.error));
  result[tab] = unavailable ? { skipped: `OCR unavailable. ${HINTS.ocr}` } : ok.length ? { expected: expected.length, found: found.size, missing: expected.filter((l) => !found.has(l)), strips: `${ok.length}/${strips.length} read`, via: route.via, ms: tabMs, slowestStripMs: Math.max(...ok.map((r) => r.ms)) } : { error: reads[0].error };
}
if (session) await deleteSession(route.endpoint, session);
writeFileSync(join(out, 'report-check.json'), JSON.stringify(result));
console.log(JSON.stringify(result));
process.exit(Object.values(result).some((r) => r.error || r.missing?.length) ? 1 : 0); // a skipped layer is not a failure
