#!/usr/bin/env node
// Image -> text through a local OCR model, run as a FreeLlama-managed vision task (raw Ollama only without the sidecar);
// with --expect, diff the text against known labels.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, resolve } from 'node:path';
import { cropScale } from './png.mjs';
import { createSession, deleteSession, ocr, ocrRoute } from './freellama.mjs';

const USAGE = `usage: ocr.mjs <image> [--expect expected.json] [--model glm-ocr] [--prompt "Text Recognition:"] [--max-tokens 1000] [--via freellama|ollama --endpoint <url> --tag <installed tag>] [--session <id>] [--priority interactive|normal|background]
  Prints JSON: {text, ms, tokens, via} plus {found, missing} when --expect lists labels.
  --via/--endpoint/--tag skip the route probe (callers pass what ocrRoute() found); --session reuses a FreeLlama affinity session.
  Exit: 0 ok, 1 expected labels missing, 2 bad input, 3 OCR unavailable (no sidecar or Ollama, or model missing).`;

const argv = process.argv.slice(2);
const flag = (name, dflt) => {
  const i = argv.indexOf(`--${name}`);
  return i < 0 ? dflt : argv.splice(i, 2)[1];
};
const model = flag('model', 'glm-ocr');
const prompt = flag('prompt', 'Text Recognition:');
const maxTokens = Number(flag('max-tokens', 1000));
const expectPath = flag('expect');
const via = flag('via');
const endpoint = flag('endpoint');
const tag = flag('tag');
const priority = flag('priority', 'background');
let session = flag('session');
const image = argv[0];
if (!image || image.startsWith('-')) {
  console.error(USAGE);
  process.exit(2);
}

// The model returns garbage for PNGs with an alpha channel: flatten PNGs in JS, convert other formats to JPEG.
function asJpeg(path) {
  if (/^\.jpe?g$/i.test(extname(path))) return path;
  if (/^\.png$/i.test(extname(path))) {
    try { // flatten alpha in JS: an RGB PNG is read reliably and needs no external tool
      const rgb = join(mkdtempSync(join(tmpdir(), 'ocr-')), 'in.png');
      writeFileSync(rgb, cropScale(readFileSync(path)));
      return rgb;
    } catch {}
  }
  const dst = join(mkdtempSync(join(tmpdir(), 'ocr-')), 'in.jpg');
  const tries = [
    ['sips', ['-s', 'format', 'jpeg', path, '--out', dst]],
    ['magick', [path, '-background', 'white', '-alpha', 'remove', dst]],
    ['convert', [path, '-background', 'white', '-alpha', 'remove', dst]],
  ];
  for (const [bin, args] of tries) {
    try {
      execFileSync(bin, args, { stdio: 'ignore' });
      return dst;
    } catch {}
  }
  console.error('cannot convert this image: pass a .png or .jpg, or install ImageMagick');
  process.exit(2);
}

let b64;
try { b64 = readFileSync(asJpeg(resolve(image))).toString('base64'); } catch (e) { console.error(`cannot read ${image}: ${e.code ?? e.message}`); process.exit(2); }
const t0 = Date.now();
const route = via && endpoint && tag ? { via, endpoint, model: tag } : await ocrRoute(model);
if (route.via === 'none') {
  console.error(`OCR unavailable: ${route.reason}`);
  process.exit(3);
}
// A standalone call gets its own affinity session; callers running many images pass one with --session.
const ownSession = route.via === 'freellama' && !session;
if (ownSession) session = await createSession(route.endpoint);
// Ollama aborts a looping prediction ("token repeat limit reached"); the text before the loop is what we want, so retry
// with half, then a quarter of the budget (a repeat penalty was measured to loop more, not less).
async function ask(extra) {
  try {
    return await ocr(route, { model: route.model, prompt, image: b64, maxTokens, sessionId: session, options: extra, priority });
  } catch (e) {
    if (ownSession) await deleteSession(route.endpoint, session);
    console.error(`${route.via} unreachable at ${route.endpoint}: ${e.message}`);
    process.exit(3);
  }
}
let r = await ask({});
for (const extra of [{ num_predict: Math.max(32, Math.floor(maxTokens / 2)) }, { num_predict: Math.max(24, Math.floor(maxTokens / 4)) }]) {
  if (!r.ok && /repeat limit/.test(r.error ?? '')) r = await ask(extra);
}
if (ownSession) await deleteSession(route.endpoint, session);
if (!r.ok || typeof r.text !== 'string') {
  console.error(r.error ?? `HTTP ${r.status}: no text in the ${route.via} answer (${JSON.stringify(r.body).slice(0, 200)})`);
  process.exit(3);
}

// The model often repeats its answer inside code fences and loops on them until the token cap.
const lines = r.text.split('\n').filter((l) => !/^\s*```/.test(l));
const seen = new Set();
const text = lines.filter((l) => !l.trim() || !seen.has(l.trim()) && seen.add(l.trim())).join('\n').trim();

const result = { text, ms: Date.now() - t0, tokens: r.tokens, via: route.via, session: session ? 'affinity' : 'none' };
let code = 0;
if (expectPath) {
  const norm = (s) => s.toLowerCase().replace(/\s+/g, '');
  const hay = norm(text);
  const { labels } = JSON.parse(readFileSync(expectPath, 'utf8'));
  result.found = labels.filter((l) => hay.includes(norm(l)));
  result.missing = labels.filter((l) => !hay.includes(norm(l)));
  if (result.missing.length) code = 1;
}
console.log(JSON.stringify(result, null, 1));
process.exit(code);
