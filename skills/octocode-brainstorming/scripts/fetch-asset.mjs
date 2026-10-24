#!/usr/bin/env node
/**
 * Plan or download source assets (PDFs, office documents, data files, images) into a brainstorm run folder.
 * Planning is the default and saves nothing; a download needs --approve, which the agent passes only after the
 * user has approved the plan. Only HTTPS URLs on public hosts, allowed content types, and files under the size
 * cap are accepted.
 */
import { fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { createHash } from 'node:crypto';
import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join, relative, resolve, sep } from 'node:path';
import { guardedFetch, urlProblem as guardProblem } from './lib/net-guard.mjs';

const USAGE = `Usage: fetch-asset.mjs --run-dir <workspace>/.octocode/octocode-brainstorming/tmp/<run> --url <https-url> [--url ...] [--max-mb <1-100>] [--approve]

Without --approve: checks each URL and prints the plan (host, content type, size, file name, or the reason it is refused). Nothing is saved.
With --approve: downloads the accepted URLs into <run-dir>/assets/ and records each in <run-dir>/assets/manifest.jsonl.
Show the plan to the user and pass --approve only for the URLs the user approved.
Accepted: HTTPS on a public host; PDF, Word, Excel, PowerPoint, OpenDocument, CSV, JSON, text, Markdown, HTML, PNG, JPEG, GIF, WebP, SVG; at most --max-mb (default 25).
Exit 0 when every URL is accepted, 1 when any is refused or fails, 2 for invalid input.`;

export const TYPES = {
  'application/pdf': '.pdf',
  'application/msword': '.doc',
  'application/vnd.openxmlformats-officedocument.wordprocessingml.document': '.docx',
  'application/vnd.ms-excel': '.xls',
  'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet': '.xlsx',
  'application/vnd.ms-powerpoint': '.ppt',
  'application/vnd.openxmlformats-officedocument.presentationml.presentation': '.pptx',
  'application/vnd.oasis.opendocument.text': '.odt',
  'application/vnd.oasis.opendocument.spreadsheet': '.ods',
  'text/csv': '.csv',
  'application/json': '.json',
  'text/plain': '.txt',
  'text/markdown': '.md',
  'text/html': '.html',
  'image/png': '.png',
  'image/jpeg': '.jpg',
  'image/gif': '.gif',
  'image/webp': '.webp',
  'image/svg+xml': '.svg',
};


/** Why a URL may not be fetched, or null when it may: HTTPS on a public host only. */
export function urlProblem(raw) {
  return guardProblem(raw, { httpsOnly: true });
}

/** The allowed type for a Content-Type header, or null. */
export function allowedType(contentType) {
  const type = String(contentType || '').split(';')[0].trim().toLowerCase();
  return TYPES[type] ? type : null;
}

function fileNameFor(finalUrl, type, sha) {
  let stem = 'asset';
  try { stem = decodeURIComponent(basename(new URL(finalUrl).pathname)) || 'asset'; } catch { /* keep the default stem */ }
  stem = stem.replace(/\.[A-Za-z0-9]{1,5}$/, '').replace(/[^A-Za-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 60) || 'asset';
  return `${sha.slice(0, 10)}-${stem}${TYPES[type]}`;
}

async function inspect(url) {
  const problem = urlProblem(url);
  if (problem) return { url, accepted: false, reason: problem };
  let res;
  try {
    res = await guardedFetch(url, { method: 'HEAD', signal: AbortSignal.timeout(20_000) }, { httpsOnly: true });
    if (!res.ok || !res.headers.get('content-type')) res = await guardedFetch(url, { headers: { range: 'bytes=0-0' }, signal: AbortSignal.timeout(20_000) }, { httpsOnly: true });
  } catch (error) { return { url, accepted: false, reason: error.guard ? error.message : `request failed: ${error.message}` }; }
  const finalUrl = res.finalUrl;
  if (!res.ok && res.status !== 206) return { url, finalUrl, accepted: false, reason: `HTTP ${res.status}` };
  const contentType = res.headers.get('content-type');
  const type = allowedType(contentType);
  const range = res.headers.get('content-range');
  const bytes = Number(range?.split('/')[1]) || Number(res.headers.get('content-length')) || null;
  res.body?.cancel?.().catch(() => {});
  return { url, finalUrl, host: new URL(finalUrl).hostname, contentType, bytes, accepted: Boolean(type), ...(type ? { type } : { reason: `content type ${contentType || 'unknown'} is not an allowed document or image type` }) };
}

async function download(plan, assetsDir, maxBytes, manifest) {
  const res = await guardedFetch(plan.finalUrl, { signal: AbortSignal.timeout(120_000) }, { httpsOnly: true });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const type = allowedType(res.headers.get('content-type'));
  if (!type) throw new Error(`content type ${res.headers.get('content-type')} is not allowed`);
  const chunks = [];
  let total = 0;
  for await (const chunk of res.body) {
    total += chunk.length;
    if (total > maxBytes) { await res.body.cancel?.().catch(() => {}); throw new Error(`larger than the ${maxBytes / 1_000_000} MB cap`); }
    chunks.push(chunk);
  }
  const data = Buffer.concat(chunks);
  const sha256 = createHash('sha256').update(data).digest('hex');
  const existing = manifest.find((row) => row.sha256 === sha256);
  if (existing) return { ...existing, reused: true };
  const file = join('assets', fileNameFor(res.finalUrl, type, sha256));
  writeFileSync(join(assetsDir, '..', file), data, { mode: 0o600 });
  return { url: plan.url, finalUrl: res.finalUrl, file, contentType: type, bytes: data.length, sha256, fetchedAt: new Date().toISOString() };
}

if (isMainModule(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes('--help') || args.includes('-h')) { console.log(USAGE); process.exit(0); }
  const opts = { urls: [], runDir: null, maxMb: 25, approve: false };
  for (let i = 0; i < args.length; i += 1) {
    const flag = args[i];
    if (flag === '--approve') { opts.approve = true; continue; }
    if (!['--url', '--run-dir', '--max-mb'].includes(flag)) fail('invalidArguments', `Unknown option: ${flag} (see --help)`, 2);
    const value = args[i + 1];
    if (value == null || value.startsWith('--')) fail('invalidArguments', `missing value for ${flag}`, 2);
    i += 1;
    if (flag === '--url') opts.urls.push(value);
    else if (flag === '--run-dir') opts.runDir = value;
    else opts.maxMb = Number(value);
  }
  if (!opts.urls.length) fail('invalidArguments', 'at least one --url is required', 2);
  if (!opts.runDir) fail('invalidArguments', '--run-dir is required', 2);
  if (!Number.isFinite(opts.maxMb) || opts.maxMb < 1 || opts.maxMb > 100) fail('invalidArguments', '--max-mb must be from 1 to 100', 2);
  const runDir = resolve(opts.runDir);
  if (!runDir.split(sep).includes('.octocode')) fail('invalidArguments', '--run-dir must be inside a .octocode folder, such as <workspace>/.octocode/octocode-brainstorming/tmp/<run>', 2);

  const plans = await Promise.all(opts.urls.map(inspect));
  const maxBytes = Math.round(opts.maxMb * 1_000_000);
  for (const plan of plans) {
    if (plan.accepted && plan.bytes && plan.bytes > maxBytes) Object.assign(plan, { accepted: false, reason: `${plan.bytes} bytes exceeds the ${opts.maxMb} MB cap` });
  }
  if (!opts.approve) {
    const accepted = plans.filter((p) => p.accepted).length;
    console.log(JSON.stringify({ ok: accepted === plans.length, approved: false, accepted, refused: plans.length - accepted, plan: plans }, null, 2));
    process.exit(accepted === plans.length ? 0 : 1);
  }

  const assetsDir = join(runDir, 'assets');
  mkdirSync(assetsDir, { recursive: true, mode: 0o700 });
  const manifestPath = join(assetsDir, 'manifest.jsonl');
  const manifest = existsSync(manifestPath) ? readFileSync(manifestPath, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l)) : [];
  const results = [];
  for (const plan of plans) {
    if (!plan.accepted) { results.push({ url: plan.url, status: 'refused', reason: plan.reason }); continue; }
    try {
      const row = await download(plan, assetsDir, maxBytes, manifest);
      if (!row.reused) { appendFileSync(manifestPath, `${JSON.stringify(row)}\n`); manifest.push(row); }
      results.push({ status: row.reused ? 'already-downloaded' : 'downloaded', ...row, path: join(runDir, row.file) });
    } catch (error) {
      results.push({ url: plan.url, status: 'failed', reason: error.message });
    }
  }
  const saved = results.filter((r) => r.status === 'downloaded' || r.status === 'already-downloaded').length;
  console.log(JSON.stringify({ ok: saved === plans.length, approved: true, saved, manifest: relative(process.cwd(), manifestPath) || manifestPath, results }, null, 2));
  process.exit(saved === plans.length ? 0 : 1);
}
