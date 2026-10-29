#!/usr/bin/env node
/**
 * Verify a claim ledger: every claim must cite a URL and a verbatim quote that the page really contains.
 * Fetches each cited page once, normalizes typography, checks the quote (ellipsis-separated fragments in order),
 * and reports title and publish dates. Only `verified` claims may be used as evidence.
 */
import { fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { existsSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { dirname, extname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { guardedFetch } from './lib/net-guard.mjs';

const USAGE = `Usage: verify-claims.mjs --ledger <claims.jsonl|claims.json> [--out <verified.jsonl>] [--concurrency <1-8>]

Each ledger row: {"id","claim","url","quote", ...}. A row with "asset" (a file saved by fetch-asset.mjs, relative to the
ledger) must match that run's assets/manifest.jsonl by path, URL, and SHA-256. It is checked as text, Markdown, CSV, JSON, or HTML directly, PDF through pdftotext when installed. The quote must be copied from the page, at least 20 characters;
separate non-adjacent passages with "..." or "…". Statuses: verified, quote-not-found, quote-too-short, missing-field,
fetch-failed, blocked (a bot check answered), unsupported-content, refused (a private or non-http host, or an unbound/changed asset).
A row the script cannot read (blocked, unsupported-content, fetch-failed) that a person or the host fetch tool checked
may carry "verifiedBy":"manual" and a "method" naming how; it then reports verified-manual. Manual marks never
override quote-not-found. --out writes every row with a "verification" object added.
Exit 0 when every claim is verified or verified-manual, 1 otherwise, 2 for invalid input.`;


const ENTITIES = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ', rsquo: "'", lsquo: "'", rdquo: '"', ldquo: '"', ndash: '-', mdash: '-', hellip: '...', laquo: '"', raquo: '"', middot: '·' };

export function decodeEntities(s) {
  return String(s || '').replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, e) => {
    if (e[0] === '#') { const n = e[1].toLowerCase() === 'x' ? parseInt(e.slice(2), 16) : Number(e.slice(1)); return Number.isFinite(n) ? String.fromCodePoint(n) : m; }
    return ENTITIES[e.toLowerCase()] ?? m;
  });
}

export function htmlToText(html) {
  return decodeEntities(String(html || '')
    .replace(/<(script|style|noscript|svg|template)\b[\s\S]*?<\/\1>/gi, ' ')
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .replace(/<(br|\/p|\/div|\/li|\/h[1-6]|\/tr|\/td|\/th)\b[^>]*>/gi, ' ')
    .replace(/<[^>]+>/g, ''));
}

/** Compare text the way a reader would: same letters and words, whatever the typography. */
export function normalize(s) {
  return String(s || '')
    .normalize('NFKC')
    .replace(/[​-‍﻿­]/g, '')
    .replace(/[‘’‚‛′`]/g, "'")
    .replace(/[“”„‟″]/g, '"')
    .replace(/[‐-―−]/g, '-')
    .replace(/\[\d+\]/g, ' ')
    .replace(/[*_]{1,2}/g, '')
    .toLowerCase()
    .replace(/\s+/g, ' ')
    .trim();
}

/** Every ellipsis-separated fragment must appear, in order. */
export function quoteInText(quote, text) {
  const page = normalize(text);
  const fragments = normalize(quote).split(/\s*(?:\.\.\.|…)\s*/).map((f) => f.replace(/^["']|["']$/g, '').trim()).filter(Boolean);
  if (!fragments.length) return false;
  let at = 0;
  for (const fragment of fragments) {
    const hit = page.indexOf(fragment, at);
    if (hit === -1) return false;
    at = hit + fragment.length;
  }
  return true;
}

export function pageDates(html) {
  const meta = (names) => {
    for (const name of names) {
      const m = html.match(new RegExp(`<meta[^>]+(?:property|name|itemprop)=["']${name}["'][^>]*content=["']([^"']+)["']`, 'i'))
        || html.match(new RegExp(`<meta[^>]+content=["']([^"']+)["'][^>]*(?:property|name|itemprop)=["']${name}["']`, 'i'));
      if (m) return m[1];
    }
    return null;
  };
  const ld = (key) => (html.match(new RegExp(`"${key}"\\s*:\\s*"([^"]+)"`)) || [])[1] || null;
  return {
    published: meta(['article:published_time', 'datePublished', 'citation_publication_date', 'citation_date', 'dc.date', 'date']) || ld('datePublished') || (html.match(/<time[^>]+datetime=["']([^"']+)["']/i) || [])[1] || null,
    modified: meta(['article:modified_time', 'dateModified', 'og:updated_time']) || ld('dateModified'),
  };
}

const CHALLENGE = /checking your browser|just a moment|attention required|are you a robot|access denied|verify you are human|captcha/i;
const pageCache = new Map();
const pageTitle = (body) => htmlToText((body.match(/<title[^>]*>([\s\S]*?)<\/title>/i) || [])[1] || '').trim().slice(0, 200) || null;
// A bot wall can answer 200; a missing quote there says nothing about the source.
const botWall = (title, body) => CHALLENGE.test(title || '') || /cf-challenge|g-recaptcha|challenge-platform|captcha-delivery/i.test(body.slice(0, 20000)) && body.length < 60000;

/** The page text and dates, or a status and error explaining why it cannot be checked. */
async function readPage(url) {
  try {
    const res = await guardedFetch(url, {
      signal: AbortSignal.timeout(25_000),
      headers: { 'user-agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126 Safari/537.36', accept: 'text/html,application/xhtml+xml,text/plain,application/json;q=0.9,*/*;q=0.5' },
    });
    const type = res.headers.get('content-type') || '';
    if (!res.ok) return { error: `HTTP ${res.status}`, status: 'fetch-failed', finalUrl: res.finalUrl };
    if (/pdf|octet-stream|image|audio|video/i.test(type)) return { status: 'unsupported-content', error: `content-type ${type}; cite the HTML abstract or landing page`, finalUrl: res.finalUrl };
    const buf = Buffer.from(await res.arrayBuffer()).subarray(0, 4_000_000);
    const body = buf.toString('utf8');
    const isHtml = /html|xml/i.test(type) || /<html|<body/i.test(body.slice(0, 2000));
    const title = isHtml ? pageTitle(body) : null;
    if (botWall(title, body)) return { status: 'blocked', error: `bot check (${title || 'challenge page'}); verify with the host fetch tool or octocode-scraping`, finalUrl: res.finalUrl };
    return { text: isHtml ? htmlToText(body) : body, title, ...(isHtml ? pageDates(body) : { published: null, modified: null }), finalUrl: res.finalUrl };
  } catch (error) {
    if (error.guard) return { status: 'refused', error: error.message, finalUrl: error.guard };
    return { status: 'fetch-failed', error: error.name === 'TimeoutError' ? 'timeout after 25 s' : error.message };
  }
}

function loadPage(url) {
  if (!pageCache.has(url)) pageCache.set(url, readPage(url));
  return pageCache.get(url);
}

const TEXT_EXT = new Set(['.txt', '.md', '.csv', '.json', '.html', '.htm']);

/** Text of a downloaded asset, or a status explaining why the script cannot read it. */
export function loadAsset(path) {
  if (!existsSync(path)) return { status: 'missing-field', error: `asset not found: ${path}` };
  const ext = extname(path).toLowerCase();
  if (TEXT_EXT.has(ext)) { const body = readFileSync(path, 'utf8'); return { text: /^\.html?$/.test(ext) ? htmlToText(body) : body }; }
  if (ext === '.pdf') {
    const out = spawnSync('pdftotext', ['-layout', path, '-'], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
    if (!out.error && out.status === 0) return { text: out.stdout };
    return { status: 'unsupported-content', error: 'PDF text needs pdftotext; read the PDF with the host file reader and mark the row "verifiedBy":"manual"' };
  }
  return { status: 'unsupported-content', error: `cannot read ${ext || 'this'} files; read the asset with the host file reader and mark the row "verifiedBy":"manual"` };
}

// Statuses a person may settle by reading the source another way; a quote the script read and did not find stays unverified.
const MANUAL_OK = new Set(['blocked', 'unsupported-content', 'fetch-failed']);

export async function verifyClaim(row, options = {}) {
  const result = await checkClaim(row, options);
  if (row?.verifiedBy === 'manual' && MANUAL_OK.has(result.status) && typeof row.method === 'string' && row.method.trim()) {
    return { ...result, status: 'verified-manual', automatic: result.status, method: row.method.trim() };
  }
  return result;
}

/** Why a ledger row cannot be checked at all, or null. */
function rowProblem(row) {
  if (!row || typeof row !== 'object' || Array.isArray(row)) return { status: 'missing-field', reason: 'ledger row must be an object' };
  for (const field of ['url', 'quote', 'claim']) if (!row[field] || typeof row[field] !== 'string') return { status: 'missing-field', reason: `no ${field}` };
  if (row.asset != null && (typeof row.asset !== 'string' || !row.asset.trim())) return { status: 'missing-field', reason: 'asset must be a non-empty path' };
  if (!/^https?:\/\//i.test(row.url)) return { status: 'missing-field', reason: 'url must be http(s)' };
  if (normalize(row.quote).replace(/\s*(\.\.\.|…)\s*/g, '').length < 20) return { status: 'quote-too-short', reason: 'quote needs at least 20 characters of the source text' };
  return null;
}

function assetPath(row, baseDir) {
  const file = isAbsolute(row.asset) ? row.asset : resolve(baseDir, row.asset);
  const assetsDir = join(baseDir, 'assets');
  const inside = relative(assetsDir, file);
  if (!inside || inside.startsWith(`..${sep}`) || inside === '..' || isAbsolute(inside)) return { error: { status: 'refused', reason: 'asset must be inside the ledger run assets directory' } };
  if (!existsSync(file) || !existsSync(join(assetsDir, 'manifest.jsonl'))) return { error: { status: 'missing-field', reason: 'asset or approved-download manifest is missing' } };
  try {
    if (realpathSync(file) !== join(realpathSync(assetsDir), inside)) return { error: { status: 'refused', reason: 'asset must not be a symlink' } };
  } catch { return { error: { status: 'refused', reason: 'asset path cannot be resolved safely' } }; }
  return { file, inside, assetsDir };
}

function assetProvenance(row, { file, inside, assetsDir }) {
  let entries;
  try { entries = readFileSync(join(assetsDir, 'manifest.jsonl'), 'utf8').trim().split('\n').map((line) => JSON.parse(line)); }
  catch { return { status: 'missing-field', reason: 'approved-download manifest is invalid' }; }
  const entry = entries.find((item) => item?.file === join('assets', inside) && (item.url === row.url || item.finalUrl === row.url));
  if (!entry || !/^[a-f0-9]{64}$/.test(entry.sha256 || '')) return { status: 'refused', reason: 'asset and URL are not recorded in the approved-download manifest' };
  let digest;
  try { digest = createHash('sha256').update(readFileSync(file)).digest('hex'); }
  catch { return { status: 'refused', reason: 'asset could not be read for integrity check' }; }
  if (digest !== entry.sha256) return { status: 'refused', reason: 'asset differs from the approved download' };
  return null;
}

function checkAsset(row, baseDir) {
  const source = assetPath(row, baseDir);
  if (source.error) return source.error;
  const mismatch = assetProvenance(row, source);
  if (mismatch) return mismatch;
  const { file } = source;
  let local;
  try { local = loadAsset(file); }
  catch { return { status: 'refused', reason: 'asset could not be read safely' }; }
  if (local.status) return { status: local.status, reason: local.error, asset: file };
  if (!quoteInText(row.quote, local.text)) return { status: 'quote-not-found', reason: 'the quote is not in the downloaded asset; recopy it exactly', asset: file };
  return { status: 'verified', asset: file, finalUrl: row.url, title: null, published: null, modified: null };
}

async function checkClaim(row, { baseDir = process.cwd() } = {}) {
  const problem = rowProblem(row);
  if (problem) return problem;
  if (row.asset) return checkAsset(row, baseDir);
  const page = await loadPage(row.url);
  const meta = { finalUrl: page.finalUrl || row.url, title: page.title || null, published: page.published || null, modified: page.modified || null };
  if (page.status) return { status: page.status, reason: page.error, ...meta };
  if (!quoteInText(row.quote, page.text)) return { status: 'quote-not-found', reason: 'the quote is not in the fetched page text; recopy it exactly, or the page may render with JavaScript', ...meta };
  return { status: 'verified', ...meta };
}

function readLedger(path) {
  let text;
  try { text = readFileSync(path, 'utf8'); } catch (error) { fail('notFound', `cannot read ledger: ${error.message}`, 1); }
  const trimmed = text.trim();
  if (!trimmed) fail('invalidArguments', 'the ledger is empty', 2);
  let rows;
  try {
    rows = trimmed.startsWith('[') ? JSON.parse(trimmed) : trimmed.split('\n').filter((l) => l.trim()).map((l) => JSON.parse(l));
  } catch (error) { fail('invalidArguments', `ledger is not JSON or JSONL: ${error.message}`, 2); }
  return rows;
}

if (isMainModule(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes('--help') || args.includes('-h')) { console.log(USAGE); process.exit(0); }
  const opts = { ledger: null, out: null, concurrency: 4 };
  for (let i = 0; i < args.length; i += 2) {
    const [flag, value] = [args[i], args[i + 1]];
    if (!['--ledger', '--out', '--concurrency'].includes(flag)) fail('invalidArguments', `Unknown option: ${flag} (see --help)`, 2);
    if (value == null || value.startsWith('--')) fail('invalidArguments', `missing value for ${flag}`, 2);
    opts[flag.slice(2)] = flag === '--concurrency' ? Number(value) : value;
  }
  if (!opts.ledger) fail('invalidArguments', '--ledger is required', 2);
  if (!Number.isSafeInteger(opts.concurrency) || opts.concurrency < 1 || opts.concurrency > 8) fail('invalidArguments', '--concurrency must be 1-8', 2);
  const rows = readLedger(resolve(opts.ledger));
  const results = Array.from({ length: rows.length });
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(opts.concurrency, rows.length) }, async () => {
    while (next < rows.length) { const i = next++; results[i] = await verifyClaim(rows[i], { baseDir: dirname(resolve(opts.ledger)) }); }
  }));
  const checkedAt = new Date().toISOString();
  const byStatus = {};
  for (const r of results) byStatus[r.status] = (byStatus[r.status] || 0) + 1;
  if (opts.out) writeFileSync(resolve(opts.out), `${rows.map((row, i) => JSON.stringify({ ...row, verification: { ...results[i], checkedAt } })).join('\n')}\n`);
  const verified = (byStatus.verified || 0) + (byStatus['verified-manual'] || 0);
  const allVerified = verified === rows.length;
  console.log(JSON.stringify({
    ok: allVerified,
    total: rows.length,
    verified,
    unverified: rows.length - verified,
    verifiedRatio: rows.length ? Number((verified / rows.length).toFixed(3)) : 0,
    byStatus,
    ...(opts.out ? { out: resolve(opts.out) } : {}),
    claims: rows.map((row, i) => ({ id: row?.id ?? i + 1, status: results[i].status, url: row?.url, ...(results[i].reason ? { reason: results[i].reason } : {}), title: results[i].title ?? null, published: results[i].published ?? null })),
    ...(allVerified ? {} : { error: { code: verified ? 'someUnverified' : 'noneVerified', message: `${rows.length - verified} of ${rows.length} claims are not verified; recopy quotes, fix URLs, or drop those claims from the evidence` } }),
  }, null, 2));
  process.exit(allVerified ? 0 : 1);
}
