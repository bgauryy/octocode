#!/usr/bin/env node
/**
 * Multi-provider web search for brainstorming: Tavily, Exa, Serper, and DeepSeek in parallel, merged by canonical URL.
 * Results are leads, not evidence: read the page and record a verbatim quote before using a claim.
 * Keys come from getKey (./keys.mjs): the process environment, then <home>/.env, then <workspace>/.octocode/.env.
 * No key is ever printed.
 */
import { fileURLToPath } from 'node:url';
import { fail } from './lib/fail.mjs';
import { isMainModule } from './lib/entrypoint.mjs';
import { getKey, keyFiles } from './keys.mjs';

const KINDS = ['web', 'news', 'papers'];
// `kinds` are the searches a vendor can serve; it is skipped for the others.
const PROVIDERS = {
  tavily: { env: 'TAVILY_API_KEY', base: getKey('TAVILY_BASE_URL').value || 'https://api.tavily.com', kinds: KINDS, docs: 'https://app.tavily.com/' },
  exa: { env: 'EXA_API_KEY', base: getKey('EXA_BASE_URL').value || 'https://api.exa.ai', kinds: KINDS, docs: 'https://dashboard.exa.ai/' },
  serper: { env: 'SERPER_API_KEY', base: getKey('SERPER_BASE_URL').value || 'https://google.serper.dev', kinds: KINDS, docs: 'https://serper.dev/' },
  // DeepSeek has one web index and no news or scholarly surface.
  deepseek: { env: 'DEEPSEEK_API_KEY', base: getKey('DEEPSEEK_BASE_URL').value || 'https://api.deepseek.com', kinds: ['web'], docs: 'https://platform.deepseek.com/' },
};
const PAPER_DOMAINS = ['arxiv.org', 'openreview.net', 'aclanthology.org', 'dl.acm.org', 'ieeexplore.ieee.org', 'semanticscholar.org', 'nature.com', 'sciencedirect.com', 'link.springer.com', 'pubmed.ncbi.nlm.nih.gov'];
// Serper answers at most 10 results per request; a larger --limit reads further pages.
const SERPER_PAGE = 10;
const KEYS_CLI = fileURLToPath(new URL('./keys.mjs', import.meta.url));
const USAGE = `Usage: search.mjs --query <text> [--query <text>]... [--kind web|news|papers] [--providers tavily,exa,serper,deepseek]
                 [--limit <1-20>] [--since YYYY-MM-DD] [--include-domain <host>]... [--exclude-domain <host>]...

Searches every provider that has a key (TAVILY_API_KEY, EXA_API_KEY, SERPER_API_KEY, DEEPSEEK_API_KEY) and merges results
by canonical URL. DeepSeek serves only --kind web.
Results seen by more providers or queries rank first; "related" lists Google's related searches and questions
(Google omits them when --since is set). --limit (default 8) applies per query per provider, so two queries can
return up to 16 rows from each provider before merging.
A key comes from the first place that sets it: the process environment, then <home>/.env (OCTOCODE_HOME, default
~/.octocode), then <workspace>/.octocode/.env (the current directory). Check them with: node ${KEYS_CLI} get TAVILY_API_KEY EXA_API_KEY SERPER_API_KEY DEEPSEEK_API_KEY
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
  if (opts.providers && !servingProviders(opts).length) {
    const serving = Object.keys(PROVIDERS).filter((name) => PROVIDERS[name].kinds.includes(opts.kind));
    fail('invalidArguments', `--kind ${opts.kind}: ${opts.providers.join(', ')} cannot search it (use ${serving.join(', ')})`, 2);
  }
  return opts;
}

/** The selected providers that can serve --kind. */
const servingProviders = (opts) => (opts.providers || Object.keys(PROVIDERS)).filter((name) => PROVIDERS[name].kinds.includes(opts.kind));

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

/** A provider failure that keeps its HTTP status and Retry-After, so the run can name the fix. */
class ProviderError extends Error {
  constructor(status, message, retryAfter) {
    super(`HTTP ${status}${message ? `: ${message}` : ''}`);
    this.status = status;
    this.retryAfter = retryAfter;
  }
}

async function post(url, headers, body) {
  const res = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json', ...headers }, body: JSON.stringify(body), signal: AbortSignal.timeout(25_000) });
  const text = await res.text();
  let json = null;
  try { json = JSON.parse(text); } catch { /* not JSON: the status line alone explains the failure */ }
  if (!res.ok) {
    const detail = json?.error?.message || json?.error || json?.detail?.error || json?.message || '';
    const seconds = Number(res.headers.get('retry-after'));
    throw new ProviderError(res.status, String(detail).slice(0, 160), Number.isFinite(seconds) && seconds > 0 ? seconds : undefined);
  }
  return json || {};
}

const clip = (s, n = 320) => { const t = String(s || '').replace(/\s+/g, ' ').trim(); return t.length > n ? `${t.slice(0, n)}…` : t; };

const SEARCHERS = {
  async tavily(key, query, o) {
    // include_published_date dates general results too (news are dated by default); include_usage reports credits.
    const body = { query, max_results: o.limit, search_depth: 'basic', topic: o.kind === 'news' ? 'news' : 'general', include_answer: false, include_published_date: true, include_usage: true };
    if (o.since) body.start_date = o.since;
    // The caller's domains narrow the search; scholarly domains are only the papers default.
    const include = o.include.length ? o.include : o.kind === 'papers' ? PAPER_DOMAINS : [];
    if (include.length) body.include_domains = include;
    if (o.exclude.length) body.exclude_domains = o.exclude;
    const j = await post(`${PROVIDERS.tavily.base}/search`, { Authorization: `Bearer ${key}` }, body);
    return {
      rows: (j.results || []).map((r) => ({ url: r.url, title: r.title, snippet: r.content, published: r.published_date || null })),
      credits: Number(j.usage?.credits) || null,
    };
  },
  async exa(key, query, o) {
    const body = { query, numResults: o.limit, type: 'auto', contents: { highlights: { maxCharacters: 640 } } };
    if (o.kind === 'news') body.category = 'news';
    // `publication` is Exa's documented category for scholarly papers.
    if (o.kind === 'papers') body.category = 'publication';
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
    const base = { q, num: SERPER_PAGE };
    if (o.since && endpoint !== 'scholar') {
      const [y, m, d] = o.since.split('-');
      base.tbs = `cdr:1,cd_min:${m}/${d}/${y}`;
    }
    let rows = [];
    let credits = 0;
    const related = [];
    for (let page = 1; rows.length < o.limit; page += 1) {
      const j = await post(`${PROVIDERS.serper.base}/${endpoint}`, { 'X-API-KEY': key }, page === 1 ? base : { ...base, page });
      credits += Number(j.credits) || 0;
      const list = (endpoint === 'news' ? j.news : j.organic) || [];
      rows.push(...list.map((r) => ({
        url: r.link,
        title: r.title,
        snippet: [r.snippet, r.publicationInfo, r.source].filter(Boolean).join(' · '),
        published: r.date || (r.year ? String(r.year) : null),
        ...(r.pdfUrl ? { pdfUrl: r.pdfUrl } : {}),
        ...(r.citedBy != null ? { citedBy: r.citedBy } : {}),
      })));
      if (page === 1) {
        for (const r of j.relatedSearches || []) if (r.query) related.push({ query: r.query, kind: 'search' });
        for (const r of j.peopleAlsoAsk || []) if (r.question) related.push({ query: r.question, kind: 'question' });
      }
      if (list.length < SERPER_PAGE) break;
    }
    // Scholar has no date filter, so drop papers older than --since by their year.
    if (endpoint === 'scholar' && o.since) rows = rows.filter((r) => !/^\d{4}$/.test(r.published || '') || r.published >= o.since.slice(0, 4));
    return { rows: rows.slice(0, o.limit), credits: credits || null, related };
  },
  async deepseek(key, query, o) {
    // Filters are query operators: site: (joined with OR), -site:, after:.
    const sites = o.include.map((d) => `site:${d}`).join(' OR ');
    const q = [query, sites, ...o.exclude.map((d) => `-site:${d}`), o.since ? `after:${o.since}` : ''].filter(Boolean).join(' ');
    const j = await post(`${PROVIDERS.deepseek.base}/v1/tools/search`, { Authorization: `Bearer ${key}` }, { query: q, count: o.limit });
    // `summary` is the longer extract written for model reading; `snippet` the short display line.
    return { rows: (j.results || []).map((r) => ({ url: r.url, title: r.title, snippet: r.summary || r.snippet || '', published: r.date || null })) };
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

const setKeyFix = (env) => `renew ${env}: node ${KEYS_CLI} set ${env} --stdin (paste the new key), then rerun`;

/** Why every query of one provider failed, and what fixes it; null when the failures differ. */
function failureStatus(name, errors) {
  const statuses = errors.map((e) => e.status);
  if (statuses.every((s) => s === 401 || s === 403)) return { status: 'keyRejected', fix: setKeyFix(PROVIDERS[name].env) };
  // Exa and DeepSeek answer 402 out of credits; Tavily 432 (plan or key limit) and 433 (pay-as-you-go limit).
  if (errors.every((e) => [402, 432, 433].includes(e.status) || /credit/i.test(e.message))) return { status: 'quotaExceeded', fix: `add credits or raise the limit at ${PROVIDERS[name].docs}, then rerun` };
  if (statuses.every((s) => s === 429)) {
    const wait = Math.max(0, ...errors.map((e) => e.retryAfter || 0));
    return { status: 'rateLimited', fix: wait ? `retry after ${wait} s with fewer queries` : 'retry later with fewer queries', ...(wait ? { retryAfterSeconds: wait } : {}) };
  }
  return null;
}

// One search per provider with a key and query; marks providers without a key or the --kind as skipped.
function startSearches(opts, keyOf, providers) {
  const jobs = [];
  for (const name of opts.providers || Object.keys(PROVIDERS)) {
    if (!PROVIDERS[name].kinds.includes(opts.kind)) { providers[name] = { status: 'skipped', reason: `no ${opts.kind} search` }; continue; }
    const key = keyOf(PROVIDERS[name].env)?.trim();
    if (!key) { providers[name] = { status: 'skipped', reason: `${PROVIDERS[name].env} not set` }; continue; }
    providers[name] = { status: 'ok', results: 0 };
    for (const query of opts.queries) {
      jobs.push(SEARCHERS[name](key, query, opts)
        .then((out) => ({ name, query, ...out }))
        .catch((error) => ({ name, query, error: { message: error.message.replaceAll(key, '[key]'), status: error.status, retryAfter: error.retryAfter } })));
    }
  }
  return jobs;
}

export async function search(opts, keyOf = (name) => getKey(name).value) {
  const providers = {};
  const jobs = startSearches(opts, keyOf, providers);
  const merged = new Map();
  const related = new Map();
  for (const out of await Promise.all(jobs)) {
    const p = providers[out.name];
    if (out.error) { p.errors = [...(p.errors || []), { query: out.query, ...out.error }]; continue; }
    p.results += out.rows.length;
    if (out.costDollars != null) p.costDollars = Number(((p.costDollars || 0) + out.costDollars).toFixed(4));
    if (out.credits != null) p.credits = (p.credits || 0) + out.credits;
    for (const r of out.related || []) if (!related.has(r.query.toLowerCase())) related.set(r.query.toLowerCase(), { ...r, provider: out.name, from: out.query });
    mergeRows(merged, out);
  }
  for (const [name, p] of Object.entries(providers)) {
    if (p.status !== 'ok' || !p.errors || p.results) continue;
    const failure = failureStatus(name, p.errors);
    Object.assign(p, failure || { status: 'error' });
    for (const e of p.errors) { delete e.status; delete e.retryAfter; }
  }
  const results = [...merged.values()].toSorted((a, b) => score(b) - score(a));
  return { providers, results, related: [...related.values()] };
}

/** The error for a run with no result: a setup problem (keys, credits, rate) is not a query problem. */
export function noResultError(providers) {
  const ran = Object.entries(providers).filter(([, p]) => p.status !== 'skipped');
  const named = (status) => ran.filter(([, p]) => p.status === status).map(([n]) => n).join(', ');
  if (ran.length && ran.every(([, p]) => p.status === 'keyRejected')) {
    return { code: 'keysRejected', message: `every configured provider rejected its key (${named('keyRejected')}); renew them with the per-provider fix, and meanwhile use the host's web search and fetch tools` };
  }
  if (ran.length && ran.every(([, p]) => ['keyRejected', 'quotaExceeded', 'rateLimited'].includes(p.status))) {
    return { code: 'providersUnavailable', message: `no configured provider could answer (${ran.map(([n, p]) => `${n}: ${p.status}`).join(', ')}); apply each provider's fix, and meanwhile use the host's web search and fetch tools` };
  }
  return { code: 'noResults', message: 'no provider returned a result; reword the query, change --kind, or drop filters' };
}

if (isMainModule(import.meta.url)) {
  const opts = parseArgs(process.argv.slice(2));
  const names = servingProviders(opts).map((n) => PROVIDERS[n].env);
  if (!names.some((name) => getKey(name).value)) {
    const files = keyFiles();
    fail('missingKey', `no key for ${names.join(', ')}: looked in the process environment, ${files.home} and ${files.workspace}. Save one with: node ${KEYS_CLI} set ${names[0]} --stdin (writes ${files.home}). Meanwhile use the host's web search and fetch tools.`, 1, { files });
  }
  const { providers, results, related } = await search(opts);
  const ok = results.length > 0;
  console.log(JSON.stringify({
    ok,
    kind: opts.kind,
    since: opts.since,
    queries: opts.queries,
    providers,
    totalResults: results.length,
    results,
    ...(related.length ? { related } : {}),
    ...(ok ? {} : { error: noResultError(providers) }),
  }, null, 2));
  process.exit(ok ? 0 : 1);
}
