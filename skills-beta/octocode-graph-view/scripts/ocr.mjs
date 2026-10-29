#!/usr/bin/env node
// Image -> text through a local Ollama OCR model; with --expect, diff the text against known labels.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, resolve } from 'node:path';
import { cropScale } from './png.mjs';

const USAGE = `usage: ocr.mjs <image> [--expect expected.json] [--model glm-ocr] [--prompt "Text Recognition:"] [--max-tokens 1000]
  Prints JSON: {text, ms, tokens} plus {found, missing} when --expect lists labels.
  Exit: 0 ok, 1 expected labels missing, 2 bad input, 3 Ollama or model unavailable.`;

const argv = process.argv.slice(2);
const flag = (name, dflt) => {
  const i = argv.indexOf(`--${name}`);
  return i < 0 ? dflt : argv.splice(i, 2)[1];
};
const model = flag('model', 'glm-ocr');
const prompt = flag('prompt', 'Text Recognition:');
const maxTokens = Number(flag('max-tokens', 1000));
const expectPath = flag('expect');
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

const host = process.env.OLLAMA_HOST?.replace(/^(?!http)/, 'http://') ?? 'http://localhost:11434';
let b64;
try { b64 = readFileSync(asJpeg(resolve(image))).toString('base64'); } catch (e) { console.error(`cannot read ${image}: ${e.code ?? e.message}`); process.exit(2); }
const t0 = Date.now();
// Ollama aborts a looping prediction with an error; one retry with a repeat penalty recovers most pages.
async function ask(extra) {
  let res;
  try {
    res = await fetch(`${host}/api/chat`, {
      method: 'POST',
      body: JSON.stringify({
        model,
        stream: false,
        messages: [{ role: 'user', content: prompt, images: [b64] }],
        options: { temperature: 0, num_predict: maxTokens, ...extra },
      }),
    });
  } catch (e) {
    console.error(`Ollama unreachable at ${host}: ${e.message}`);
    process.exit(3);
  }
  return { ok: res.ok, status: res.status, body: await res.json() };
}
let r = await ask({});
for (const extra of [{ repeat_penalty: 1.15 }, { repeat_penalty: 1.3, temperature: 0.2, num_predict: Math.min(maxTokens, 500) }]) {
  if (!r.ok && /repeat limit/.test(r.body.error ?? '')) r = await ask(extra);
}
const body = r.body;
if (!r.ok) {
  console.error(body.error ?? `HTTP ${r.status}`);
  process.exit(3);
}

// The model often repeats its answer inside code fences and loops on them until the token cap.
const lines = body.message.content.split('\n').filter((l) => !/^\s*```/.test(l));
const seen = new Set();
const text = lines.filter((l) => !l.trim() || !seen.has(l.trim()) && seen.add(l.trim())).join('\n').trim();

const result = { text, ms: Date.now() - t0, tokens: body.eval_count };
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
