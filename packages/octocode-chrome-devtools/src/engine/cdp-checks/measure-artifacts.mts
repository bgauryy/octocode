// The measure artifacts measure-query reads: performance/network/storage JSON and an optional HAR,
// found in one run folder (named, or the newest with a measure) or passed as files, and the
// next-page continuation that pins them.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

const MEASURES = [
  'performance-measure.json',
  'network-measure.json',
  'storage-measure.json',
];

/** Parsed JSON at path; null without a path. An unreadable artifact fails. */
export function readMeasure(path) {
  if (!path) return null;
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new Error(
      `Cannot read measurement artifact ${path}: ${error.message}`,
      { cause: error }
    );
  }
}

const isDirectory = p => {
  try {
    return statSync(p).isDirectory();
  } catch {
    return false;
  }
};

const newestFirst = (a, b) => {
  try {
    return statSync(b).mtimeMs - statSync(a).mtimeMs;
  } catch {
    return 0;
  }
};

// A directory is one capture boundary. Never fill missing measures from older runs.
function latestRunDir(under) {
  const root = resolve(under);
  if (!existsSync(root)) return null;
  return (
    readdirSync(root)
      .map(name => join(root, name))
      .filter(isDirectory)
      .toSorted(newestFirst)
      .find(dir => MEASURES.some(name => existsSync(join(dir, name)))) ?? null
  );
}

function firstHar(dir) {
  if (!existsSync(dir)) return null;
  const names = readdirSync(dir).filter(n => /\.har$/i.test(n));
  return names.length ? join(dir, names.toSorted()[0]) : null;
}

function discoverInDir(dir) {
  const d = resolve(dir);
  const pick = name => {
    const p = join(d, name);
    return existsSync(p) ? p : null;
  };
  return {
    dir: d,
    perf: pick('performance-measure.json'),
    net: pick('network-measure.json'),
    storage: pick('storage-measure.json'),
    har: firstHar(d),
  };
}

const resolved = file => (file ? resolve(file) : null);

/** The artifact paths: --latest under a root, one --dir, or explicit files. */
export function measurePaths({ latest, under, dir, perf, net, storage, har }) {
  if (latest) {
    const found = latestRunDir(under);
    return found
      ? discoverInDir(found)
      : {
          dir: resolve(under),
          perf: null,
          net: null,
          storage: null,
          har: null,
        };
  }
  if (dir) return discoverInDir(dir);
  return {
    dir: null,
    perf: resolved(perf),
    net: resolved(net),
    storage: resolved(storage),
    har: resolved(har),
  };
}

const hostOf = url => {
  try {
    return new URL(url).hostname;
  } catch {
    return '';
  }
};

// HAR custom fields carry a leading underscore by spec.
function compactEntry(entry, index) {
  const request = entry.request ?? {};
  const response = entry.response ?? {};
  return {
    index,
    method: request.method,
    url: request.url,
    host: hostOf(request.url),
    status: response.status,
    ms: Math.round(entry.time ?? 0),
    type: entry['_resourceType'] ?? '',
    failed: Boolean(
      entry['_failed'] || response.status >= 400 || response.status === 0
    ),
  };
}

/** One compact row per HAR entry, in file order. */
export function harRows(path) {
  const har = JSON.parse(readFileSync(path, 'utf8'));
  const entries = Array.isArray(har.log?.entries) ? har.log.entries : [];
  return entries.map(compactEntry);
}

/** Rows matching --filter: all, failures, slow (>= minMs) or domain:<host>. */
export function filterHar(rows, filter, minMs) {
  if (filter === 'failures') return rows.filter(r => r.failed);
  if (filter === 'slow') return rows.filter(r => r.ms >= minMs);
  if (!filter.startsWith('domain:')) return rows;
  const host = filter.slice('domain:'.length).toLowerCase();
  return rows.filter(r => r.host.toLowerCase().includes(host));
}

const FILTER_FLAGS = [
  '--code',
  '--kind',
  '--domain',
  '--url-regex',
  '--name-regex',
  '--min-ms',
  '--min-health',
  '--max-health',
  '--filter',
];

/** The next-page command line: same view, limit and filters, with every source file pinned. */
export function nextPageArgs(
  { script, view, limit, page, paths, includeHar },
  getArg
) {
  const args = [
    script,
    '--view',
    view,
    '--limit',
    String(limit),
    '--page',
    String(page + 1),
  ];
  for (const [flag, file] of [
    ['--perf', paths.perf],
    ['--net', paths.net],
    ['--storage', paths.storage],
    ['--har-file', paths.har],
  ])
    if (file) args.push(flag, resolve(file));
  for (const flag of FILTER_FLAGS) {
    const value = getArg(flag, null);
    if (value !== null) args.push(flag, value);
  }
  if (includeHar) args.push('--har');
  return args;
}
