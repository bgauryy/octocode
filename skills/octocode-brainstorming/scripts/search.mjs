#!/usr/bin/env node
/**
 * Multi-provider web search for brainstorming: Tavily, Exa, and Serper in parallel, merged by canonical URL.
 * Results are leads, not evidence: read the page and record a verbatim quote before using a claim.
 * Keys are loaded from the process environment and trusted Octocode dotenv files, and are never printed.
 */
import { fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { loadOctocodeEnv } from './lib/config-env.mjs';

loadOctocodeEnv();

const PROVIDERS = {
  tavily: { env: 'TAVILY_API_KEY', base: process.env.TAVILY_BASE_URL || 'https://api.tavily.com' },
  exa: { env: 'EXA_API_KEY', base: process.env.EXA_BASE_URL || 'https://api.exa.ai' },
  serper: { env: 'SERPER_API_KEY', base: process.env.SERPER_BASE_URL || 'https://google.serper.dev' },
};
const KINDS = ['web', 'news', 'papers'];
const PAPER_DOMAINS = ['arxiv.org', 'openreview.net', 'aclanthology.org', 'dl.acm.org', 'ieeexplore.ieee.org', 'semanticscholar.org', 'nature.com', 'sciencedirect.com', 'link.springer.com', 'pubmed.ncbi.nlm.nih.gov'];
const USAGE = `Usage: search.mjs --query <text> [--query <text>]... [--kind web|news|papers] [--providers tavily,exa,serper]
                 [--limit <1-20>] [--since YYYY-MM-DD] [--include-domain <host>]... [--exclude-domain <host>]...

Searches every provider whose key is available in the process environment or Octocode dotenv files (TAVILY_API_KEY, EXA_API_KEY, SERPER_API_KEY)
and merges results by canonical URL. Results seen by more providers or queries rank first.
Exit 0 with results, 1 when nothing was found or no provider could run, 2 for invalid input.`;


const REPEATED = { '--query': 'queries', '--include-domain': 'include', '--exclude-domain': 'exclude' };

function setOption(opts, flag, value) {
  if (REPEATED[flag]) opts[REPEATED[flag]].push(value.trim());
  else if (flag === '--kind') opts.kind = value;
  else if (flag === '--providers') opts.providers = value.split(',').map((s) => s.trim()).filter(Boolean);
  else if (flag === '--limit') opts.limit = Number(value);
  else if (flag === '--since') opts.since = value;
}

function parseArgs(argv) {
  const opts = { queries: [], kind: 'web', providers: /** @type {string[] | null} */ (null), limit: 8, since: null, include: [], exclude: [] };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    if (flag === '--help' || flag === '-h') { console.log(USAGE); process.exit(0); }
    const value = argv[i + 1];
    if (!['--query', '--kind', '--providers', '--limit', '--since', '--include-domain', '--exclude-domain'].includes(flag)) fail('invalidArguments', `Unknown option: ${flag} (see --help)`, 2);
    if (value == null || value.startsWith('--')) fail('invalidArguments', `missing value for ${flag}`, 2);
    i += 1;
    setOption(opts, flag, value);
  }
  return validateOptions(opts);
}

function validateOptions(opts) {
  if (!opts.queries.filter(Boolean).length) fail('invalidArguments', 'at least one non-empty --query is required', 2);
  if (!KINDS.includes(opts.kind)) fail('invalidArguments', `--kind must be one of ${KINDS.join(', ')}`, 2);
  if (!Number.isSafeInteger(opts.limit) || opts.limit < 1 || opts.limit > 20) fail('invalidArguments', '--limit must be an integer from 1 to 20', 2);
  if (opts.since && !/^\d{4}-\d{2}-\d{2}$/.test(opts.since)) fail('invalidArguments', '--since must be YYYY-MM-DD', 2);
  for (const name of opts.providers || []) if (!PROVIDERS[name]) fail('invalidArguments', `unknown provider: ${name} (use ${Object.keys(PROVIDERS).join(', ')})`, 2);
  return opts;
}

/** One identity per page: no scheme, www, fragment, trailing slash, or tracking parameters. */
export function canonicalUrl(raw) {
  try {
    const u = new URL(raw);
    u.hash = '';
    for (const key of Array.from(u.searchParams.keys())) if (/^(utm_|ref$|ref_src$|gclid$|fbclid$|mc_|source$)/i.test(key)) u.searchParams.delete(key);
    const host = u.hostname.toLowerCase().replace(/^www\./, '');
    const path = u.pathname.replace(/\/+$/, '');
    return `${host}${path}${u.search}`;
  } catch {
    return String(raw || '').trim();
  }
}

async function post(url, headers, body) {
  const res = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json', ...headers }, body: JSON.stringify(body), signal: AbortSignal.timeout(25_000) });
  const text = await res.text();
  let json = null;
  try { json = JSON.parse(text); } catch { /* not JSON: the status line alone explains the failure */ }
  if (!res.ok) throw new Error(`HTTP ${res.status}${json?.error ? `: ${String(json.error.message || json.error).slice(0, 160)}` : ''}`);
  return json || {};
}

const clip = (s, n = 320) => { const t = String(s || '').replace(/\s+/g, ' ').trim(); return t.length > n ? `${t.slice(0, n)}…` : t; };

const SEARCHERS = {
  async tavily(key, query, o) {
    const body = { query, max_results: o.limit, search_depth: 'basic', topic: o.kind === 'news' ? 'news' : 'general', include_answer: false };
    if (o.since) body.start_date = o.since;
    const include = o.kind === 'papers' ? [...o.include, ...PAPER_DOMAINS] : o.include;
    if (include.length) body.include_domains = include;
    if (o.exclude.length) body.exclude_domains = o.exclude;
    const j = await post(`${PROVIDERS.tavily.base}/search`, { Authorization: `Bearer ${key}` }, body);
    return { rows: (j.results || []).map((r) => ({ url: r.url, title: r.title, snippet: r.content, published: r.published_date || null })) };
  },
  async exa(key, query, o) {
    const body = { query, numResults: o.limit, type: 'auto', contents: { highlights: { numSentences: 2 } } };
    if (o.kind === 'news') body.category = 'news';
    if (o.kind === 'papers') body.category = 'research paper';
    if (o.since) body.startPublishedDate = `${o.since}T00:00:00.000Z`;
    if (o.include.length) body.includeDomains = o.include;
    if (o.exclude.length) body.excludeDomains = o.exclude;
    const j = await post(`${PROVIDERS.exa.base}/search`, { 'x-api-key': key }, body);
    return {
      rows: (j.results || []).map((r) => ({ url: r.url, title: r.title, snippet: (r.highlights || []).join(' … ') || r.text || '', published: r.publishedDate || null })),
      costDollars: j.costDollars?.total ?? null,
    };
  },
  async serper(key, query, o) {
    const sites = o.include.map((d) => `site:${d}`);
    const q = [query, sites.length ? `(${sites.join(' OR ')})` : '', ...o.exclude.map((d) => `-site:${d}`)].filter(Boolean).join(' ');
    const endpoint = o.kind === 'news' ? 'news' : o.kind === 'papers' ? 'scholar' : 'search';
    const body = { q, num: o.limit };
    if (o.since && endpoint !== 'scholar') {
      const [y, m, d] = o.since.split('-');
      body.tbs = `cdr:1,cd_min:${m}/${d}/${y}`;
    }
    const j = await post(`${PROVIDERS.serper.base}/${endpoint}`, { 'X-API-KEY': key }, body);
    const list = endpoint === 'news' ? j.news : j.organic;
    let rows = (list || []).map((r) => ({
      url: r.link,
      title: r.title,
      snippet: [r.snippet, r.publicationInfo].filter(Boolean).join(' · '),
      published: r.date || (r.year ? String(r.year) : null),
      ...(r.pdfUrl ? { pdfUrl: r.pdfUrl } : {}),
      ...(r.citedBy != null ? { citedBy: r.citedBy } : {}),
    }));
    // Scholar has no date filter, so drop papers older than --since by their year.
    if (endpoint === 'scholar' && o.since) rows = rows.filter((r) => !/^\d{4}$/.test(r.published || '') || r.published >= o.since.slice(0, 4));
    // Serper ignores num on some endpoints, so the limit is applied here.
    return { rows: rows.slice(0, o.limit) };
  },
};

const score = (h) => new Set(h.seenBy.map((s) => s.provider)).size * 10 + h.seenBy.length - Math.min(...h.seenBy.map((s) => s.rank)) / 100;

// Merges one provider answer into `merged` (canonical URL → hit); the first non-empty field wins.
function mergeRows(merged, out) {
  out.rows.forEach((row, index) => {
    if (!row.url) return;
    const id = canonicalUrl(row.url);
    const hit = merged.get(id) || { url: row.url, title: row.title || null, snippet: clip(row.snippet), published: row.published || null, seenBy: [] };
    if (!hit.published && row.published) hit.published = row.published;
    if (!hit.snippet && row.snippet) hit.snippet = clip(row.snippet);
    for (const extra of ['pdfUrl', 'citedBy']) if (row[extra] != null && hit[extra] == null) hit[extra] = row[extra];
    hit.seenBy.push({ provider: out.name, query: out.query, rank: index + 1 });
    merged.set(id, hit);
  });
}

// One search per provider with a key and query; marks providers without a key as skipped.
function startSearches(opts, env, providers) {
  const jobs = [];
  for (const name of opts.providers || Object.keys(PROVIDERS)) {
    const key = env[PROVIDERS[name].env]?.trim();
    if (!key) { providers[name] = { status: 'skipped', reason: `${PROVIDERS[name].env} not set` }; continue; }
    providers[name] = { status: 'ok', results: 0 };
    for (const query of opts.queries) {
      jobs.push(SEARCHERS[name](key, query, opts)
        .then((out) => ({ name, query, ...out }))
        .catch((error) => ({ name, query, error: error.message.replaceAll(key, '[key]') })));
    }
  }
  return jobs;
}

export async function search(opts, env = process.env) {
  const providers = {};
  const jobs = startSearches(opts, env, providers);
  const merged = new Map();
  for (const out of await Promise.all(jobs)) {
    const p = providers[out.name];
    if (out.error) { p.errors = [...(p.errors || []), { query: out.query, message: out.error }]; continue; }
    p.results += out.rows.length;
    if (out.costDollars != null) p.costDollars = Number(((p.costDollars || 0) + out.costDollars).toFixed(4));
    mergeRows(merged, out);
  }
  for (const [name, p] of Object.entries(providers)) {
    if (p.status !== 'ok' || !p.errors || p.results) continue;
    // 401 and 403 on every query: the stored key is wrong or revoked, not the query. Name the fix.
    const rejected = p.errors.every((e) => /^HTTP 40[13]\b/.test(e.message));
    p.status = rejected ? 'keyRejected' : 'error';
    if (rejected) p.fix = `renew ${PROVIDERS[name].env}: npx -y octocode config set ${PROVIDERS[name].env} --stdin (paste the new key), then rerun`;
  }
  const results = [...merged.values()].toSorted((a, b) => score(b) - score(a));
  return { providers, results };
}

/** The error for a run with no result: all keys rejected is a setup problem, not a query problem. */
export function noResultError(providers) {
  const ran = Object.values(providers).filter((p) => p.status !== 'skipped');
  if (ran.length && ran.every((p) => p.status === 'keyRejected')) {
    return { code: 'keysRejected', message: `every configured provider rejected its key (${Object.entries(providers).filter(([, p]) => p.status === 'keyRejected').map(([n]) => n).join(', ')}); renew them with the per-provider fix, and meanwhile use the host's web search and fetch tools` };
  }
  return { code: 'noResults', message: 'no provider returned a result; reword the query, change --kind, or drop filters' };
}

if (isMainModule(import.meta.url)) {
  const opts = parseArgs(process.argv.slice(2));
  const ran = (opts.providers || Object.keys(PROVIDERS)).filter((n) => process.env[PROVIDERS[n].env]?.trim());
  if (!ran.length) fail('missingKey', `no provider key in the environment; set one of ${Object.values(PROVIDERS).map((p) => p.env).join(', ')}`, 1);
  const { providers, results } = await search(opts);
  const ok = results.length > 0;
  console.log(JSON.stringify({
    ok,
    kind: opts.kind,
    since: opts.since,
    queries: opts.queries,
    providers,
    totalResults: results.length,
    results,
    ...(ok ? {} : { error: noResultError(providers) }),
  }, null, 2));
  process.exit(ok ? 0 : 1);
}
