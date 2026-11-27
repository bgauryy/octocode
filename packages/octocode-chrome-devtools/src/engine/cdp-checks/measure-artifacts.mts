// The measure artifacts measure-query reads: performance/network/storage JSON and an optional HAR,
// found in one run folder (named, or the newest with a measure) or passed as files, and the
// next-page continuation that pins them.
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { errorMessage } from '../error-message.mjs';
import type { HarEntry, HarFile } from './har-types.mjs';
import type { HarRow, MeasurePaths } from './measure-types.mjs';

const MEASURES = [
  'performance-measure.json',
  'network-measure.json',
  'storage-measure.json',
];

/**
 * Parsed JSON at path; null without a path. An unreadable artifact fails. A value that is not a
 * JSON object holds none of the fields a query reads, so it reads as null.
 */
export function readMeasure(path: string | null): object | null {
  if (!path) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new Error(
      `Cannot read measurement artifact ${path}: ${errorMessage(error)}`,
      { cause: error }
    );
  }
  return typeof parsed === 'object' ? parsed : null;
}

const isDirectory = (p: string) => {
  try {
    return statSync(p).isDirectory();
  } catch {
    return false;
  }
};

const newestFirst = (a: string, b: string) => {
  try {
    return statSync(b).mtimeMs - statSync(a).mtimeMs;
  } catch {
    return 0;
  }
};

// A directory is one capture boundary. Never fill missing measures from older runs.
function latestRunDir(under: string) {
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

function firstHar(dir: string) {
  if (!existsSync(dir)) return null;
  const names = readdirSync(dir).filter(n => /\.har$/i.test(n));
  return names.length ? join(dir, names.toSorted()[0]) : null;
}

function discoverInDir(dir: string): MeasurePaths {
  const d = resolve(dir);
  const pick = (name: string) => {
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

const resolved = (file: string | undefined) => (file ? resolve(file) : null);

/** The artifact paths: --latest under a root, one --dir, or explicit files. */
type MeasureSources = {
  latest: boolean;
  under: string;
  dir?: string;
  perf?: string;
  net?: string;
  storage?: string;
  har?: string;
};

export function measurePaths({
  latest,
  under,
  dir,
  perf,
  net,
  storage,
  har,
}: MeasureSources): MeasurePaths {
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

const hostOf = (url: string | undefined) => {
  try {
    return new URL(url ?? '').hostname;
  } catch {
    return '';
  }
};

// HAR custom fields carry a leading underscore by spec.
function compactEntry(entry: HarEntry, index: number): HarRow {
  const request = entry.request ?? {};
  const response = entry.response ?? {};
  const status = response.status;
  return {
    index,
    method: request.method,
    url: request.url,
    host: hostOf(request.url),
    status,
    ms: Math.round(entry.time ?? 0),
    type: entry['_resourceType'] ?? '',
    failed: Boolean(
      entry['_failed'] ||
      (status !== undefined && status >= 400) ||
      status === 0
    ),
  };
}

/** One compact row per HAR entry, in file order. */
export function harRows(path: string): HarRow[] {
  const har: HarFile = JSON.parse(readFileSync(path, 'utf8'));
  const entries: HarEntry[] = Array.isArray(har.log?.entries)
    ? har.log.entries
    : [];
  return entries.map(compactEntry);
}

/** Rows matching --filter: all, failures, slow (>= minMs) or domain:<host>. */
export function filterHar(rows: HarRow[], filter: string, minMs: number) {
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

/** Every value flag measure-query accepts: the sources, the view and paging, and the filters. */
export const MEASURE_VALUE_FLAGS = [
  '--dir',
  '--under',
  '--perf',
  '--net',
  '--storage',
  '--har-file',
  '--view',
  '--limit',
  '--page',
  ...FILTER_FLAGS,
];

/** The next-page command line: same view, limit and filters, with every source file pinned. */
type NextPage = {
  script: string;
  view: string;
  limit: number;
  page: number;
  paths: MeasurePaths;
  includeHar: boolean;
};

export function nextPageArgs(
  { script, view, limit, page, paths, includeHar }: NextPage,
  getArg: (flag: string) => string | undefined
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
  const pinned: [string, string | null][] = [
    ['--perf', paths.perf],
    ['--net', paths.net],
    ['--storage', paths.storage],
    ['--har-file', paths.har],
  ];
  for (const [flag, file] of pinned) if (file) args.push(flag, resolve(file));
  for (const flag of FILTER_FLAGS) {
    const value = getArg(flag);
    if (value !== undefined) args.push(flag, value);
  }
  if (includeHar) args.push('--har');
  return args;
}
