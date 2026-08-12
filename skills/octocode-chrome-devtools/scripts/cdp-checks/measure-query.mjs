#!/usr/bin/env node
/**
 * Smart filter/query over chrome-devtools measure artifacts (+ optional HAR).
 * Compact JSON only — never dump full files into agent context.
 *
 * Usage:
 *   measure-query.mjs --dir <cdp-run> [--view summary|findings|failures|slow|sample|resources|cookies|keys|all]
 *   measure-query.mjs --dir <cdp-run> --code HTTP_FAILURES --view findings
 *   measure-query.mjs --dir <cdp-run> --kind api --view sample
 *   measure-query.mjs --dir <cdp-run> --max-health 90 --view summary
 *   measure-query.mjs --dir <cdp-run> --har --filter failures [--min-ms 1000]
 *   measure-query.mjs --latest [--under .octocode/tmp/chrome-devtools]
 */
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';

const argv = process.argv.slice(2);
const getArg = (flag, def) => {
  const i = argv.indexOf(flag);
  return i === -1 ? def : argv[i + 1] ?? def;
};
const hasFlag = (flag) => argv.includes(flag);

if (hasFlag('--help') || hasFlag('-h')) {
  console.error(`Usage:
  measure-query.mjs --dir <cdp-run> [filters] [--view summary|findings|failures|slow|sample|resources|cookies|keys|all]
  measure-query.mjs --latest [--under <tmp/chrome-devtools>] [filters]
  measure-query.mjs --perf <file> --net <file> --storage <file> [filters]

Filters:
  --code <FINDING_CODE>     e.g. HTTP_FAILURES, SLOW_FCP, SENSITIVE_COOKIE_NOT_HTTPONLY
  --kind <api|doc|css|js|img|font|xhr|other|data>
  --domain <host-substring>
  --url-regex <pattern>
  --name-regex <cookie|storage key>
  --min-ms <n>              slow threshold (default 1000)
  --min-health / --max-health <0-100>
  --limit <n>               max rows per list (default 25)
  --har                     also page/filter HAR in the same dir
  --filter <all|failures|slow|domain:<host>>   HAR filter when --har

Stdout: compact JSON { ok, summary, findings, … }.`);
  process.exit(0);
}

const view = getArg('--view', 'summary');
const codeFilter = getArg('--code', '');
const kindFilter = (getArg('--kind', '') || '').toLowerCase();
const domainFilter = (getArg('--domain', '') || '').toLowerCase();
const urlRegexStr = getArg('--url-regex', '');
const nameRegexStr = getArg('--name-regex', '');
const minMs = Math.max(0, Number.parseInt(getArg('--min-ms', '1000'), 10) || 1000);
const minHealth = getArg('--min-health', null);
const maxHealth = getArg('--max-health', null);
const limit = Math.max(1, Math.min(200, Number.parseInt(getArg('--limit', '25'), 10) || 25));
const includeHar = hasFlag('--har');
const harFilter = getArg('--filter', 'all');

const urlRe = urlRegexStr ? new RegExp(urlRegexStr, 'i') : null;
const nameRe = nameRegexStr ? new RegExp(nameRegexStr, 'i') : null;

function readJson(path) {
  if (!path || !existsSync(path)) return null;
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch {
    return null;
  }
}

function findLatestRuns(under, need = ['performance-measure.json', 'network-measure.json', 'storage-measure.json']) {
  const root = resolve(under);
  if (!existsSync(root)) return { dirs: [], files: {} };
  const dirs = readdirSync(root)
    .map((name) => join(root, name))
    .filter((p) => {
      try {
        return statSync(p).isDirectory();
      } catch {
        return false;
      }
    })
    .sort((a, b) => {
      try {
        return statSync(b).mtimeMs - statSync(a).mtimeMs;
      } catch {
        return 0;
      }
    });

  const files = {};
  for (const name of need) {
    for (const dir of dirs) {
      const p = join(dir, name);
      if (existsSync(p) && !files[name]) {
        files[name] = p;
        break;
      }
    }
  }
  return { dirs, files };
}

function discoverInDir(dir) {
  const d = resolve(dir);
  const pick = (name) => {
    const p = join(d, name);
    return existsSync(p) ? p : null;
  };
  let har = null;
  if (existsSync(d)) {
    const names = readdirSync(d).filter((n) => /\.har$/i.test(n));
    har = names.length ? join(d, names.sort()[0]) : null;
  }
  return {
    dir: d,
    perf: pick('performance-measure.json'),
    net: pick('network-measure.json'),
    storage: pick('storage-measure.json'),
    har,
  };
}

let paths;
if (hasFlag('--latest')) {
  const under = getArg('--under', '.octocode/tmp/chrome-devtools');
  const { files } = findLatestRuns(under);
  paths = {
    dir: under,
    perf: files['performance-measure.json'] || null,
    net: files['network-measure.json'] || null,
    storage: files['storage-measure.json'] || null,
    har: null,
  };
  // HAR from same dirs as measures when possible
  for (const key of ['network-measure.json', 'performance-measure.json']) {
    if (files[key]) {
      const sibling = discoverInDir(dirname(files[key]));
      if (sibling.har) {
        paths.har = sibling.har;
        break;
      }
    }
  }
} else if (getArg('--dir', null)) {
  paths = discoverInDir(getArg('--dir'));
} else {
  paths = {
    dir: null,
    perf: getArg('--perf', null) ? resolve(getArg('--perf')) : null,
    net: getArg('--net', null) ? resolve(getArg('--net')) : null,
    storage: getArg('--storage', null) ? resolve(getArg('--storage')) : null,
    har: getArg('--har-file', null) ? resolve(getArg('--har-file')) : null,
  };
}

if (!paths.perf && !paths.net && !paths.storage && !(includeHar && paths.har)) {
  console.log(JSON.stringify({
    ok: false,
    error: 'No measure artifacts found. Pass --dir, --latest, or --perf/--net/--storage.',
    hint: 'Run performance/network/storage-measure-check.mjs first, or point --dir at a cdp output folder.',
  }));
  process.exit(1);
}

const perf = readJson(paths.perf);
const net = readJson(paths.net);
const storage = readJson(paths.storage);

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
  return String(row.kind || row.type || '').toLowerCase().includes(kindFilter);
}

function collectFindings() {
  const out = [];
  if (perf?.score?.findings && healthPass(healthOf('perf'))) {
    for (const f of perf.score.findings) {
      if (codeFilter && f.code !== codeFilter) continue;
      out.push({ source: 'performance', ...f });
    }
  }
  if (net?.findings && healthPass(healthOf('net'))) {
    for (const f of net.findings) {
      if (codeFilter && f.code !== codeFilter) continue;
      out.push({ source: 'network', ...f });
    }
  }
  if (storage?.score?.findings && healthPass(healthOf('storage'))) {
    for (const f of storage.score.findings) {
      if (codeFilter && f.code !== codeFilter) continue;
      out.push({ source: 'storage', ...f });
    }
  }
  return out;
}

function slice(rows) {
  return rows.slice(0, limit);
}

function filterRows(rows) {
  return (rows || []).filter((r) => matchUrl(r.url) && matchKind(r));
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

const result = { ...summary };

const want = (name) => view === 'all' || view === name || (view === 'summary' && name === 'summary');

if (want('summary') && view === 'summary') {
  // summary already on result; add top findings
  result.findings = slice(collectFindings());
}

if (want('findings') && view !== 'summary') {
  result.findings = slice(collectFindings());
}

if (want('failures') || (view === 'all')) {
  result.failures = slice(filterRows(net?.failures || []));
}

if (want('slow') || view === 'all') {
  const listedSlow = filterRows(net?.slow || []).filter((r) => (r.ms ?? 0) >= minMs);
  const fromSample = filterRows(net?.sample || []).filter((r) => (r.ms ?? 0) >= minMs);
  const slowPerf = (perf?.score?.slowResources || perf?.resources || [])
    .filter((r) => (r.duration ?? r.ms ?? 0) >= minMs)
    .filter((r) => matchUrl(r.name || r.url));
  result.slow = {
    network: slice(listedSlow.length ? listedSlow : fromSample),
    resources: slice(slowPerf),
  };
}

if (want('sample') || view === 'all') {
  result.sample = slice(filterRows(net?.sample || []));
}

if (want('resources') || view === 'all') {
  result.resources = slice(
    (perf?.resources || [])
      .filter((r) => matchUrl(r.name || r.url))
      .filter((r) => !kindFilter || String(r.initiatorType || r.type || '').toLowerCase().includes(kindFilter)),
  );
  result.measures = slice(perf?.measures || []);
  result.paints = perf?.paints ?? null;
  result.fcp = perf?.fcp ?? null;
  result.lcp = perf?.lcp ?? null;
  result.cls = perf?.cls ?? null;
}

if (want('cookies') || view === 'all') {
  result.cookies = slice(
    (storage?.cookies?.rows || []).filter((c) => {
      if (nameRe && !nameRe.test(c.name || '')) return false;
      if (domainFilter && !String(c.domain || '').toLowerCase().includes(domainFilter)) return false;
      return true;
    }),
  );
}

if (want('keys') || view === 'all') {
  const local = (storage?.storage?.localStorageKeys || []).filter((k) => !nameRe || nameRe.test(k));
  const session = (storage?.storage?.sessionStorageKeys || []).filter((k) => !nameRe || nameRe.test(k));
  const suspicious = [
    ...(storage?.storage?.suspiciousLocalKeys || []).map((k) => ({ scope: 'local', key: k })),
    ...(storage?.storage?.suspiciousSessionKeys || []).map((k) => ({ scope: 'session', key: k })),
  ].filter((row) => !nameRe || nameRe.test(row.key));
  result.keys = {
    local: slice(local),
    session: slice(session),
    suspicious: slice(suspicious),
    indexedDB: storage?.storage?.indexedDBDatabases || [],
    caches: storage?.storage?.cacheNames || [],
    serviceWorkers: storage?.storage?.serviceWorkers || [],
  };
}

if (includeHar && paths.har) {
  try {
    const har = JSON.parse(readFileSync(paths.har, 'utf8'));
    const entries = Array.isArray(har.log?.entries) ? har.log.entries : [];
    const compact = entries.map((entry, index) => {
      const request = entry.request ?? {};
      const response = entry.response ?? {};
      let host = '';
      try {
        host = new URL(request.url).hostname;
      } catch {
        host = '';
      }
      return {
        index,
        method: request.method,
        url: request.url,
        host,
        status: response.status,
        ms: Math.round(entry.time ?? 0),
        type: entry._resourceType ?? '',
        failed: Boolean(entry._failed || response.status >= 400 || response.status === 0),
      };
    });
    let rows = compact;
    if (harFilter === 'failures') rows = rows.filter((r) => r.failed);
    else if (harFilter === 'slow') rows = rows.filter((r) => r.ms >= minMs);
    else if (harFilter.startsWith('domain:')) {
      const host = harFilter.slice('domain:'.length).toLowerCase();
      rows = rows.filter((r) => r.host.toLowerCase().includes(host));
    }
    rows = rows.filter((r) => matchUrl(r.url) && (!kindFilter || String(r.type).toLowerCase().includes(kindFilter)));
    result.har = {
      path: paths.har,
      filter: harFilter,
      total: compact.length,
      matched: rows.length,
      rows: slice(rows),
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

result.next = [];
if ((result.findings?.length || 0) > 0 && view === 'summary') {
  result.next.push({ view: 'findings', code: result.findings[0]?.code });
}
if ((result.failures?.length || result.counts?.failures || 0) > 0 && view === 'summary') {
  result.next.push({ view: 'failures' });
}
if ((storage?.score?.findings?.length || 0) > 0 && view === 'summary') {
  result.next.push({ view: 'cookies' }, { view: 'keys' });
}

console.log(JSON.stringify(result, null, 2));
console.error(`[METRIC] QUERY view=${view} perf=${healthOf('perf') ?? '-'} net=${healthOf('net') ?? '-'} storage=${healthOf('storage') ?? '-'} findings=${collectFindings().length}`);
