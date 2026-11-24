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
import { getKey } from './keys.mjs';

const USAGE = `Usage: verify-claims.mjs --ledger <claims.jsonl|claims.json> [--ledger <another>]... [--out <verified.jsonl>]
                        [--reuse <earlier.verified.jsonl>] [--since YYYY-MM-DD] [--until YYYY-MM-DD]
                        [--concurrency <1-8>] [--via auto|exa|tavily|none]

Each ledger row: {"id","claim","url","quote", ...}. A row with "asset" (a file saved by fetch-asset.mjs, relative to the
ledger) must match that run's assets/manifest.jsonl by path, URL, and SHA-256. It is checked as text, Markdown, CSV, JSON, or HTML directly, PDF through pdftotext when installed. The quote must be copied from the page, at least 20 characters;
separate non-adjacent passages with "..." or "…". Statuses: verified, quote-not-found, quote-too-short, missing-field,
fetch-failed, blocked (a bot check answered), unsupported-content, refused (a private or non-http host, or an unbound/changed asset).
A row the script cannot read (blocked, unsupported-content, fetch-failed) that a person or the host fetch tool checked
may carry "verifiedBy":"manual" and a "method" naming how; it then reports verified-manual. Manual marks never
override quote-not-found. --out writes every row with a "verification" object added.
A page the script cannot read (blocked, fetch-failed, unsupported-content) is read once more through a provider copy of
the same URL: Exa /contents, then Tavily /extract, whichever has a key (--via picks one; none turns it off). A quote
found there reports verified with "via" naming the copy; a quote missing from it keeps the original status.
--ledger repeats (one per worker); the summary then counts each ledger and the total; --out takes one ledger.
--reuse keeps an earlier verified or verified-manual result for a row with the same url, quote and asset, so a
rerun fetches only new or changed rows. --since/--until add dateInWindow (yes, no or unknown) from the page's
published or modified date (a dated sentence in the text, such as "Last updated on ...", when the page has none; never
the server's Last-Modified header, which is a deploy time, nor a timestamp two or more pages of one site share, marked
publishedFrom:"site-build"); dateWindow counts yes, no and unknown. Optional row fields:
decides (the id of the deciding claim or option the row is evidence for), direction (supports or refutes), claimStatus
(proven, contradicted or unresolved: the status of that deciding claim), primary (true or false), stake (who gains;
passed through only). decidingClaims summarizes each decides id: rows, verified, distinctSources, supports, refutes,
the claimStatus values given and conflictingStatus when they differ. Rows citing one source under different URLs get
sameSourceAs; distinctSources counts sources per decides id (else per claim text). providerCopies totals the
provider-copy spend, and reusedPages the rows that still rest on an earlier copy.
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
    published: meta(['article:published_time', 'datePublished', 'citation_publication_date', 'citation_date', 'dc.date', 'dcterms.date', 'DC.date.issued', 'publish_date', 'publication_date', 'parsely-pub-date', 'sailthru.date', 'date']) || ld('datePublished') || (html.match(/<time[^>]+datetime=["']([^"']+)["']/i) || [])[1] || meta(['ms.date']) || null,
    modified: meta(['article:modified_time', 'dateModified', 'og:updated_time', 'updated_at', 'dcterms.modified', 'last-modified', 'revised']) || ld('dateModified'),
  };
}

const MONTHS = 'jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?';
const DATED = new RegExp(`\\b(?:last\\s+(?:updated|modified|reviewed|tested|revised)|updated|published|posted|revised)(?:\\s+on)?\\s*:?\\s*(\\d{4}-\\d{2}-\\d{2}|(?:${MONTHS})\\.?\\s+\\d{1,2},?\\s+\\d{4}|\\d{1,2}\\s+(?:${MONTHS})\\.?\\s+\\d{4})`, 'i');

/** The date of a "Last updated on …" style sentence in the page text, as YYYY-MM-DD, or null. */
export function textDate(text) {
  const found = DATED.exec(String(text || '').slice(0, 200_000));
  if (!found) return null;
  if (/^\d{4}-/.test(found[1])) return found[1];
  const parsed = Date.parse(`${found[1].replace(/\.(?=\s)/, '').replace(',', '')} UTC`);
  return Number.isFinite(parsed) ? new Date(parsed).toISOString().slice(0, 10) : null;
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
    const dates = isHtml ? pageDates(body) : { published: null, modified: null };
    // The server's Last-Modified header is the fallback when the page names no update date.
    if (!dates.modified && res.headers.get('last-modified')) Object.assign(dates, { modified: res.headers.get('last-modified'), modifiedFrom: 'http' });
    const text = isHtml ? htmlToText(body) : body;
    return { text, title, ...dates, ...(dates.published ? {} : { textDate: textDate(text) }), finalUrl: res.finalUrl };
  } catch (error) {
    if (error.guard) return { status: 'refused', error: error.message, finalUrl: error.guard };
    return { status: 'fetch-failed', error: error.name === 'TimeoutError' ? 'timeout after 25 s' : error.message };
  }
}

// A provider reads the page for us when a bot wall, an error or a PDF stops the direct read. Each returns the text
// of that exact URL, or throws; the copy can be cached, so its source and date travel with the result.
const COPIERS = {
  async exa(key, url) {
    const base = getKey('EXA_BASE_URL').value || 'https://api.exa.ai';
    const j = await postJson(`${base}/contents`, { 'x-api-key': key }, { urls: [url], text: true });
    const status = j.statuses?.[0];
    if (status && status.status !== 'success') throw new Error(status.error?.tag || 'no copy');
    const page = j.results?.[0];
    if (!page?.text) throw new Error('no text');
    return { text: page.text, title: page.title || null, published: page.publishedDate || null, source: status?.source || null, costDollars: j.costDollars?.total ?? null };
  },
  async tavily(key, url) {
    const base = getKey('TAVILY_BASE_URL').value || 'https://api.tavily.com';
    const j = await postJson(`${base}/extract`, { Authorization: `Bearer ${key}` }, { urls: [url], format: 'text', include_usage: true });
    const page = j.results?.[0];
    if (!page?.raw_content) throw new Error(j.failed_results?.[0]?.error || 'no text');
    return { text: page.raw_content, title: null, published: page.published_date || null, source: null, credits: Number(j.usage?.credits) || null };
  },
};
const COPY_KEYS = { exa: 'EXA_API_KEY', tavily: 'TAVILY_API_KEY' };
const COPY_STATUSES = new Set(['blocked', 'fetch-failed', 'unsupported-content']);
let via = 'auto';
export const VIA_MODES = ['auto', 'none', ...Object.keys(COPIERS)];

async function postJson(url, headers, body) {
  const res = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json', ...headers }, body: JSON.stringify(body), signal: AbortSignal.timeout(30_000) });
  const text = await res.text();
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return JSON.parse(text);
}

/** The direct read, or for an unreadable public page the first provider copy that has text. */
async function readWithCopy(url, mode) {
  const page = await readPage(url);
  if (!COPY_STATUSES.has(page.status) || mode === 'none') return page;
  const attempts = [];
  for (const name of mode === 'auto' ? Object.keys(COPIERS) : [mode]) {
    const key = getKey(COPY_KEYS[name]).value;
    if (!key) continue;
    try {
      const copy = await COPIERS[name](key, url);
      const cost = copy.costDollars != null ? { costDollars: copy.costDollars } : copy.credits != null ? { credits: copy.credits } : {};
      return { text: copy.text, title: copy.title || page.title || null, published: copy.published, modified: null, finalUrl: url, via: { provider: name, ...(copy.source ? { source: copy.source } : {}), ...cost }, direct: { status: page.status, error: page.error } };
    } catch (error) {
      attempts.push(`${name}: ${String(error.message).replaceAll(key, '[key]')}`);
    }
  }
  return attempts.length ? { ...page, error: `${page.error}; provider copies failed (${attempts.join('; ')})` } : page;
}

/** One cited page as text, read directly or through a provider copy (`mode`: auto, none, exa, tavily). */
export function loadSource(url, mode = via) {
  const key = `${mode} ${url}`;
  if (!pageCache.has(key)) pageCache.set(key, readWithCopy(url, mode));
  return pageCache.get(key);
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
  const page = await loadSource(row.url);
  const meta = { finalUrl: page.finalUrl || row.url, title: page.title || null, published: page.published || null, modified: page.modified || null, ...(page.modifiedFrom ? { modifiedFrom: page.modifiedFrom } : {}), ...(page.textDate ? { textDate: page.textDate } : {}), ...(page.via ? { via: page.via } : {}) };
  if (page.status) return { status: page.status, reason: page.error, ...meta };
  // A provider copy can miss what the page shows, so a quote absent from it is unread, not refuted.
  if (page.via && !quoteInText(row.quote, page.text)) return { status: page.direct.status, reason: `${page.direct.error}; the quote is not in the ${page.via.provider} copy either: recopy it exactly or read the page another way`, ...meta };
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

const REUSABLE = new Set(['verified', 'verified-manual']);
const CLAIM_STATUSES = new Set(['proven', 'contradicted', 'unresolved']);
const sameRow = (a, b) => a.url === b.url && a.quote === b.quote && (a.asset ?? null) === (b.asset ?? null);

/** A day (YYYY-MM-DD) or a year (YYYY) from a page or vendor date, or null when unreadable. */
export function dayOf(raw) {
  const text = String(raw ?? '').trim();
  if (/^\d{4}$/.test(text)) return text;
  const ymd = text.match(/^(\d{4})[-/](\d{2})[-/](\d{2})/);
  if (ymd) return `${ymd[1]}-${ymd[2]}-${ymd[3]}`;
  const parsed = Date.parse(text);
  return Number.isFinite(parsed) ? new Date(parsed).toISOString().slice(0, 10) : null;
}

/** yes, no or unknown: whether a page date falls in [since, until]; a bare year compares by year. */
export function dateInWindow(result, since, until) {
  // A deploy header says when the site was published, not the article: it never decides the window.
  const published = result.publishedFrom === 'site-build' ? null : result.published;
  const day = dayOf(published) || (result.modifiedFrom === 'http' ? null : dayOf(result.modified)) || dayOf(result.textDate);
  if (!day) return 'unknown';
  const cut = (bound) => (day.length === 4 ? bound.slice(0, 4) : bound);
  return (since && day < cut(since)) || (until && day > cut(until)) ? 'no' : 'yes';
}

function parseArgs(args) {
  const opts = { ledgers: [], out: null, reuse: null, since: null, until: null, concurrency: 4, via: 'auto' };
  for (let i = 0; i < args.length; i += 2) {
    const [flag, value] = [args[i], args[i + 1]];
    if (!['--ledger', '--out', '--reuse', '--since', '--until', '--concurrency', '--via'].includes(flag)) fail('invalidArguments', `Unknown option: ${flag} (see --help)`, 2);
    if (value == null || value.startsWith('--')) fail('invalidArguments', `missing value for ${flag}`, 2);
    if (flag === '--ledger') opts.ledgers.push(value);
    else opts[flag.slice(2)] = flag === '--concurrency' ? Number(value) : value;
  }
  if (!opts.ledgers.length) fail('invalidArguments', '--ledger is required', 2);
  if (opts.out && opts.ledgers.length > 1) fail('invalidArguments', '--out takes one --ledger; run once per ledger to write each verified copy', 2);
  if (!Number.isSafeInteger(opts.concurrency) || opts.concurrency < 1 || opts.concurrency > 8) fail('invalidArguments', '--concurrency must be 1-8', 2);
  if (!VIA_MODES.includes(opts.via)) fail('invalidArguments', `--via must be one of ${VIA_MODES.join(', ')}`, 2);
  for (const flag of ['since', 'until']) if (opts[flag] && !/^\d{4}-\d{2}-\d{2}$/.test(opts[flag])) fail('invalidArguments', `--${flag} must be YYYY-MM-DD`, 2);
  return opts;
}

function earlierResults(path) {
  if (!path) return [];
  return readLedger(resolve(path)).filter((row) => REUSABLE.has(row?.verification?.status));
}

/** A timestamp (with a time of day) that two or more different pages of one host share is the site's build stamp. */
export function markBuildStamps(rows, results) {
  const pages = new Map();
  results.forEach((r, i) => {
    if (!r.published || !/\d:\d/.test(r.published)) return;
    let host;
    try { host = new URL(r.finalUrl || rows[i]?.url).hostname; } catch { return; }
    const key = `${host} ${r.published}`;
    const entry = pages.get(key) || { paths: new Set(), indexes: [] };
    entry.paths.add(canonicalSource(r.finalUrl || rows[i]?.url));
    entry.indexes.push(i);
    pages.set(key, entry);
  });
  for (const { paths, indexes } of pages.values()) if (paths.size > 1) for (const i of indexes) results[i].publishedFrom = 'site-build';
}

/** Rows that cite one source (same final URL) under different ledger URLs, and distinct sources per claim text. */
function sourceLinks(rows, results) {
  const bySource = new Map();
  rows.forEach((row, i) => {
    const source = canonicalSource(results[i].finalUrl || row?.url);
    if (!source) return;
    bySource.set(source, [...(bySource.get(source) || []), i]);
  });
  const sameSourceAs = new Map();
  for (const indexes of bySource.values()) {
    if (new Set(indexes.map((i) => rows[i].url)).size < 2) continue;
    for (const i of indexes) sameSourceAs.set(i, indexes.filter((j) => j !== i && rows[j].url !== rows[i].url).map((j) => rows[j]?.id ?? j + 1));
  }
  const byClaim = new Map();
  rows.forEach((row, i) => {
    const key = groupOf(row);
    if (!REUSABLE.has(results[i].status) || !key) return;
    const sources = byClaim.get(key) || new Set();
    sources.add(canonicalSource(results[i].finalUrl || row.url));
    byClaim.set(key, sources);
  });
  return { sameSourceAs, distinct: (row) => byClaim.get(groupOf(row))?.size ?? 0 };
}

/** The deciding claim a row serves (`decides`), else its own claim text. */
const groupOf = (row) => (typeof row?.decides === 'string' && row.decides) ? `decides:${row.decides}` : typeof row?.claim === 'string' ? `claim:${row.claim}` : null;

/** Per deciding claim: rows, verified rows, independent sources, supporting and refuting rows, and the statuses given. */
function decidingClaims(rows, results, links) {
  const out = {};
  rows.forEach((row, i) => {
    if (typeof row?.decides !== 'string' || !row.decides) return;
    const entry = (out[row.decides] ||= { rows: 0, verified: 0, distinctSources: 0, supports: 0, refutes: 0, claimStatus: [] });
    entry.rows += 1;
    if (REUSABLE.has(results[i].status)) entry.verified += 1;
    entry.distinctSources = links.distinct(row);
    if (row.direction === 'supports') entry.supports += 1;
    if (row.direction === 'refutes') entry.refutes += 1;
    if (row.claimStatus && !entry.claimStatus.includes(row.claimStatus)) entry.claimStatus.push(row.claimStatus);
  });
  for (const entry of Object.values(out)) {
    entry.claimStatus.sort();
    entry.conflictingStatus = entry.claimStatus.length > 1;
  }
  return out;
}

function canonicalSource(raw) {
  try {
    const u = new URL(raw);
    return `${u.hostname.toLowerCase().replace(/^www\./, '')}${u.pathname.replace(/\/+$/, '')}`;
  } catch { return null; }
}

if (isMainModule(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes('--help') || args.includes('-h')) { console.log(USAGE); process.exit(0); }
  const opts = parseArgs(args);
  via = opts.via;
  const earlier = earlierResults(opts.reuse);
  const rows = [];
  const ledgerOf = [];
  for (const ledger of opts.ledgers) for (const row of readLedger(resolve(ledger))) { rows.push(row); ledgerOf.push(resolve(ledger)); }
  const results = Array.from({ length: rows.length });
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(opts.concurrency, rows.length) }, async () => {
    while (next < rows.length) {
      const i = next++;
      const kept = rows[i] && typeof rows[i] === 'object' ? earlier.find((old) => sameRow(old, rows[i])) : null;
      results[i] = kept ? { ...kept.verification, reused: true } : await verifyClaim(rows[i], { baseDir: dirname(ledgerOf[i]) });
    }
  }));
  markBuildStamps(rows, results);
  if (opts.since || opts.until) results.forEach((r) => { r.dateInWindow = dateInWindow(r, opts.since, opts.until); });
  const links = sourceLinks(rows, results);
  const checkedAt = new Date().toISOString();
  const count = (indexes) => {
    const byStatus = {};
    for (const i of indexes) byStatus[results[i].status] = (byStatus[results[i].status] || 0) + 1;
    const verified = (byStatus.verified || 0) + (byStatus['verified-manual'] || 0);
    return { total: indexes.length, verified, unverified: indexes.length - verified, byStatus };
  };
  const all = rows.map((_, i) => i);
  const totals = count(all);
  if (opts.out) writeFileSync(resolve(opts.out), `${rows.map((row, i) => JSON.stringify({ ...row, verification: { checkedAt, ...results[i] } })).join('\n')}\n`);
  const tally = (field, allowed) => {
    const out = {};
    for (const row of rows) if (row?.[field] != null) out[String(row[field])] = (out[String(row[field])] || 0) + 1;
    const invalid = allowed ? Object.keys(out).filter((value) => !allowed.has(value)) : [];
    return Object.keys(out).length ? { counts: out, ...(invalid.length ? { invalid } : {}) } : null;
  };
  const spend = {};
  for (const r of results) {
    if (!r.via) continue;
    const p = (spend[r.via.provider] ||= { pages: 0 });
    if (r.reused) { p.reusedPages = (p.reusedPages || 0) + 1; continue; }
    p.pages += 1;
    if (r.via.costDollars != null) p.costDollars = Number(((p.costDollars || 0) + r.via.costDollars).toFixed(4));
    if (r.via.credits != null) p.credits = (p.credits || 0) + r.via.credits;
  }
  const allVerified = totals.verified === rows.length;
  const claimStatus = tally('claimStatus', CLAIM_STATUSES);
  const primary = tally('primary');
  const deciding = decidingClaims(rows, results, links);
  const window = opts.since || opts.until ? { since: opts.since, until: opts.until, yes: 0, no: 0, unknown: 0 } : null;
  if (window) for (const r of results) window[r.dateInWindow] += 1;
  console.log(JSON.stringify({
    ok: allVerified,
    ...totals,
    verifiedRatio: rows.length ? Number((totals.verified / rows.length).toFixed(3)) : 0,
    ...(opts.ledgers.length > 1 ? { ledgers: opts.ledgers.map((ledger) => ({ ledger: resolve(ledger), ...count(all.filter((i) => ledgerOf[i] === resolve(ledger))) })) } : {}),
    ...(results.some((r) => r.reused) ? { reused: results.filter((r) => r.reused).length } : {}),
    ...(claimStatus ? { claimStatus } : {}),
    ...(primary ? { primary } : {}),
    ...(Object.keys(deciding).length ? { decidingClaims: deciding } : {}),
    ...(window ? { dateWindow: window } : {}),
    ...(Object.keys(spend).length ? { providerCopies: spend } : {}),
    ...(opts.out ? { out: resolve(opts.out) } : {}),
    claims: rows.map((row, i) => ({
      id: row?.id ?? i + 1,
      status: results[i].status,
      url: row?.url,
      ...(results[i].reason ? { reason: results[i].reason } : {}),
      title: results[i].title ?? null,
      published: results[i].published ?? null,
      modified: results[i].modified ?? null,
      ...(results[i].publishedFrom ? { publishedFrom: results[i].publishedFrom } : {}),
      ...(results[i].textDate ? { textDate: results[i].textDate } : {}),
      ...(results[i].dateInWindow ? { dateInWindow: results[i].dateInWindow } : {}),
      ...(results[i].via ? { via: results[i].via } : {}),
      ...(results[i].reused ? { reused: true } : {}),
      ...(row?.claimStatus ? { claimStatus: row.claimStatus } : {}),
      ...(links.sameSourceAs.has(i) ? { sameSourceAs: links.sameSourceAs.get(i) } : {}),
      ...(row?.decides ? { decides: row.decides } : {}),
      ...(groupOf(row) && REUSABLE.has(results[i].status) ? { distinctSources: links.distinct(row) } : {}),
    })),
    ...(allVerified ? {} : { error: { code: totals.verified ? 'someUnverified' : 'noneVerified', message: `${rows.length - totals.verified} of ${rows.length} claims are not verified; recopy quotes, fix URLs, or drop those claims from the evidence` } }),
  }, null, 2));
  process.exit(allVerified ? 0 : 1);
}
