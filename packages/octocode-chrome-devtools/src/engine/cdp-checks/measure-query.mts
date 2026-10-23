#!/usr/bin/env node
/**
 * Filter/query over chrome-devtools measure artifacts (+ optional HAR).
 * Each list is paged; next.continue pins the source files and filters.
 *
 * Usage:
 *   measure-query.mjs --dir <cdp-run> [--view summary|findings|failures|slow|sample|resources|cookies|keys|all]
 *   measure-query.mjs --dir <cdp-run> --code HTTP_FAILURES --view findings
 *   measure-query.mjs --dir <cdp-run> --kind api --view sample
 *   measure-query.mjs --dir <cdp-run> --max-health 90 --view summary
 *   measure-query.mjs --dir <cdp-run> --har --filter failures [--min-ms 1000]
 *   measure-query.mjs --latest [--under .octocode/octocode-chrome-devtools/tmp]
 */
import { flagArgs, validateFlags } from '../cli-flags.mjs';
import {
  filterHar,
  harRows,
  measurePaths,
  nextPageArgs,
  readMeasure,
} from './measure-artifacts.mjs';

const argv = process.argv.slice(2);
const { getArg, hasFlag, getLowerArg } = flagArgs(argv);

if (hasFlag('--help') || hasFlag('-h')) {
  console.error(`Usage:
  measure-query.mjs --dir <cdp-run> [filters] [--view summary|findings|failures|slow|sample|resources|cookies|keys|all]
  measure-query.mjs --latest [--under <octocode-chrome-devtools/tmp>] [filters]
  measure-query.mjs --perf <file> --net <file> --storage <file> [filters]

Filters:
  --code <FINDING_CODE>     e.g. HTTP_FAILURES, SLOW_FCP, SENSITIVE_COOKIE_NOT_HTTPONLY
  --kind <api|doc|css|js|img|font|xhr|other|data>
  --domain <host-substring>
  --url-regex <pattern>
  --name-regex <cookie|storage key>
  --min-ms <n>              slow threshold (default 1000)
  --min-health / --max-health <0-100>
  --limit <n>               rows per page (default 25)
  --page <n>                page number (default 1)
  --latest                  newest run containing a measure; missing types stay absent
  --har                     also page/filter HAR in the same dir
  --filter <all|failures|slow|domain:<host>>   HAR filter when --har

Stdout: compact JSON { ok, summary, findings, … }.`);
  process.exit(0);
}

validateFlags(
  argv,
  [
    '--dir',
    '--under',
    '--perf',
    '--net',
    '--storage',
    '--har-file',
    '--view',
    '--code',
    '--kind',
    '--domain',
    '--url-regex',
    '--name-regex',
    '--min-ms',
    '--min-health',
    '--max-health',
    '--limit',
    '--page',
    '--filter',
  ],
  ['--help', '-h', '--latest', '--har']
);
const view = getArg('--view', 'summary');
const codeFilter = getArg('--code', '');
const kindFilter = getLowerArg('--kind');
const domainFilter = getLowerArg('--domain');
const urlRegexStr = getArg('--url-regex', '');
const nameRegexStr = getArg('--name-regex', '');
const minMs = Math.max(
  0,
  Number.parseInt(getArg('--min-ms', '1000'), 10) || 1000
);
const minHealth = getArg('--min-health', null);
const maxHealth = getArg('--max-health', null);
const limit = Math.max(
  1,
  Math.min(200, Number.parseInt(getArg('--limit', '25'), 10) || 25)
);
const page = Number(getArg('--page', '1'));
if (!Number.isSafeInteger(page) || page < 1)
  throw new Error('--page must be a positive integer');
const pagination = {};
const includeHar = hasFlag('--har');
const harFilter = getArg('--filter', 'all');

const urlRe = urlRegexStr ? new RegExp(urlRegexStr, 'i') : null;
const nameRe = nameRegexStr ? new RegExp(nameRegexStr, 'i') : null;

const paths = measurePaths({
  latest: hasFlag('--latest'),
  under: getArg('--under', '.octocode/octocode-chrome-devtools/tmp'),
  dir: getArg('--dir', null),
  perf: getArg('--perf', null),
  net: getArg('--net', null),
  storage: getArg('--storage', null),
  har: getArg('--har-file', null),
});

if (!paths.perf && !paths.net && !paths.storage && !(includeHar && paths.har)) {
  console.log(
    JSON.stringify({
      ok: false,
      error:
        'No measure artifacts found. Pass --dir, --latest, or --perf/--net/--storage.',
      hint: 'Run performance/network/storage-measure-check.mjs first, or point --dir at a cdp output folder.',
    })
  );
  process.exit(1);
}

const perf = readMeasure(paths.perf);
const net = readMeasure(paths.net);
const storage = readMeasure(paths.storage);

function healthOf(kind) {
  if (kind === 'perf') return perf?.score?.health ?? null;
  if (kind === 'net') return net?.health ?? null;
  if (kind === 'storage') return storage?.score?.health ?? null;
  return null;
}

function healthPass(h) {
  if (h == null) return true;
  if (minHealth != null && h < Number(minHealth)) return false;
  if (maxHealth != null && h > Number(maxHealth)) return false;
  return true;
}

function matchUrl(url) {
  if (!url) return !urlRe && !domainFilter;
  if (domainFilter) {
    try {
      const host = new URL(url).hostname.toLowerCase();
      if (!host.includes(domainFilter)) return false;
    } catch {
      if (!String(url).toLowerCase().includes(domainFilter)) return false;
    }
  }
  if (urlRe && !urlRe.test(url)) return false;
  return true;
}

function matchKind(row) {
  if (!kindFilter) return true;
  return String(row.kind || row.type || '')
    .toLowerCase()
    .includes(kindFilter);
}

function collectFindings() {
  const out = [];
  for (const [source, findings, kind] of [
    ['performance', perf?.score?.findings, 'perf'],
    ['network', net?.findings, 'net'],
    ['storage', storage?.score?.findings, 'storage'],
  ]) {
    if (!findings || !healthPass(healthOf(kind))) continue;
    for (const f of findings)
      if (!codeFilter || f.code === codeFilter) out.push({ source, ...f });
  }
  return out;
}

function slice(rows, name) {
  const totalPages = Math.max(1, Math.ceil(rows.length / limit));
  pagination[name] = {
    page,
    pageSize: limit,
    totalRows: rows.length,
    totalPages,
    hasMore: page < totalPages,
  };
  return rows.slice((page - 1) * limit, page * limit);
}

function filterRows(rows) {
  return (rows || []).filter(r => matchUrl(r.url) && matchKind(r));
}

const summary = {
  ok: true,
  view,
  paths: {
    dir: paths.dir,
    performance: paths.perf,
    network: paths.net,
    storage: paths.storage,
    har: paths.har,
  },
  health: {
    performance: healthOf('perf'),
    network: healthOf('net'),
    storage: healthOf('storage'),
  },
  counts: {
    perfFindings: perf?.score?.findings?.length ?? 0,
    netFindings: net?.findings?.length ?? 0,
    storageFindings: storage?.score?.findings?.length ?? 0,
    failures: net?.counts?.failed ?? net?.failures?.length ?? 0,
    slow: net?.counts?.slow ?? net?.slow?.length ?? 0,
    requests: net?.counts?.requests ?? net?.sample?.length ?? 0,
    cookies: storage?.cookies?.count ?? storage?.cookies?.rows?.length ?? 0,
    localKeys: storage?.storage?.localStorageKeys?.length ?? 0,
    longTasks: perf?.longTasks?.length ?? 0,
    resources: perf?.resources?.length ?? 0,
  },
  byKind: net?.byKind ?? null,
  byStatus: net?.byStatus ?? null,
};

const result: Record<string, any> = { ...summary };

const want = name =>
  view === 'all' || view === name || (view === 'summary' && name === 'summary');

if (want('summary') && view === 'summary') {
  // summary already on result; add top findings
  result.findings = slice(collectFindings(), 'findings');
}

if (want('findings') && view !== 'summary') {
  result.findings = slice(collectFindings(), 'findings');
}

if (want('failures') || view === 'all') {
  result.failures = slice(filterRows(net?.failures || []), 'failures');
}

if (want('slow') || view === 'all') {
  const listedSlow = filterRows(net?.slow || []).filter(
    r => (r.ms ?? 0) >= minMs
  );
  const fromSample = filterRows(net?.sample || []).filter(
    r => (r.ms ?? 0) >= minMs
  );
  const slowPerf = (perf?.score?.slowResources || perf?.resources || [])
    .filter(r => (r.duration ?? r.ms ?? 0) >= minMs)
    .filter(r => matchUrl(r.name || r.url));
  result.slow = {
    network: slice(listedSlow.length ? listedSlow : fromSample, 'slow.network'),
    resources: slice(slowPerf, 'slow.resources'),
  };
}

if (want('sample') || view === 'all') {
  result.sample = slice(filterRows(net?.sample || []), 'sample');
}

if (want('resources') || view === 'all') {
  result.resources = slice(
    (perf?.resources || [])
      .filter(r => matchUrl(r.name || r.url))
      .filter(
        r =>
          !kindFilter ||
          String(r.initiatorType || r.type || '')
            .toLowerCase()
            .includes(kindFilter)
      ),
    'resources'
  );
  result.measures = slice(perf?.measures || [], 'measures');
  result.paints = perf?.paints ?? null;
  result.fcp = perf?.fcp ?? null;
  result.lcp = perf?.lcp ?? null;
  result.cls = perf?.cls ?? null;
}

if (want('cookies') || view === 'all') {
  result.cookies = slice(
    (storage?.cookies?.rows || []).filter(c => {
      if (nameRe && !nameRe.test(c.name || '')) return false;
      if (
        domainFilter &&
        !String(c.domain || '')
          .toLowerCase()
          .includes(domainFilter)
      )
        return false;
      return true;
    }),
    'cookies'
  );
}

if (want('keys') || view === 'all') {
  const local = (storage?.storage?.localStorageKeys || []).filter(
    k => !nameRe || nameRe.test(k)
  );
  const session = (storage?.storage?.sessionStorageKeys || []).filter(
    k => !nameRe || nameRe.test(k)
  );
  const suspicious = [
    ...(storage?.storage?.suspiciousLocalKeys || []).map(k => ({
      scope: 'local',
      key: k,
    })),
    ...(storage?.storage?.suspiciousSessionKeys || []).map(k => ({
      scope: 'session',
      key: k,
    })),
  ].filter(row => !nameRe || nameRe.test(row.key));
  result.keys = {
    local: slice(local, 'keys.local'),
    session: slice(session, 'keys.session'),
    suspicious: slice(suspicious, 'keys.suspicious'),
    indexedDB: slice(
      storage?.storage?.indexedDBDatabases || [],
      'keys.indexedDB'
    ),
    caches: slice(storage?.storage?.cacheNames || [], 'keys.caches'),
    serviceWorkers: slice(
      storage?.storage?.serviceWorkers || [],
      'keys.serviceWorkers'
    ),
  };
}

if (includeHar && paths.har) {
  try {
    const compact = harRows(paths.har);
    const rows = filterHar(compact, harFilter, minMs).filter(
      r =>
        matchUrl(r.url) &&
        (!kindFilter || String(r.type).toLowerCase().includes(kindFilter))
    );
    result.har = {
      path: paths.har,
      filter: harFilter,
      total: compact.length,
      matched: rows.length,
      rows: slice(rows, 'har.rows'),
    };
  } catch (err) {
    result.har = { path: paths.har, error: String(err?.message || err) };
  }
}

// Health gate: if summary-only and health filters exclude all sources, mark filtered
if (minHealth != null || maxHealth != null) {
  result.healthFilter = {
    min: minHealth != null ? Number(minHealth) : null,
    max: maxHealth != null ? Number(maxHealth) : null,
    included: {
      performance: healthPass(healthOf('perf')),
      network: healthPass(healthOf('net')),
      storage: healthPass(healthOf('storage')),
    },
  };
}

result.hints = [];
if ((result.findings?.length || 0) > 0 && view === 'summary') {
  result.hints.push({ view: 'findings', code: result.findings[0]?.code });
}
if (
  (result.failures?.length || result.counts?.failures || 0) > 0 &&
  view === 'summary'
) {
  result.hints.push({ view: 'failures' });
}
if ((storage?.score?.findings?.length || 0) > 0 && view === 'summary') {
  result.hints.push({ view: 'cookies' }, { view: 'keys' });
}

result.pagination = pagination;
result.next = {};
if (
  Object.values(pagination).some((list: { hasMore: boolean }) => list.hasMore)
) {
  const args = nextPageArgs(
    { script: process.argv[1], view, limit, page, paths, includeHar },
    getArg
  );
  result.next.continue = { command: process.execPath, args };
}
console.log(JSON.stringify(result, null, 2));
console.error(
  `[METRIC] QUERY view=${view} perf=${healthOf('perf') ?? '-'} net=${healthOf('net') ?? '-'} storage=${healthOf('storage') ?? '-'} findings=${collectFindings().length}`
);
