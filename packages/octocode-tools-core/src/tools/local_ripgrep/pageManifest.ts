import { createHash, randomUUID } from 'node:crypto';
import { constants } from 'node:fs';
import {
  lstat,
  mkdir,
  open,
  readdir,
  realpath,
  rename,
  unlink,
} from 'node:fs/promises';
import { isAbsolute, join, relative } from 'node:path';
import { getOctocodeHome } from '@octocodeai/config';
import type { RipgrepQuery } from '@octocodeai/octocode-core/schema';
import type { LocalSearchCodeFile } from '@octocodeai/octocode-core/types';
import { sanitizeStructuredContent } from '../../responses.js';
import { contextUtils } from '../../utils/contextUtils.js';
import type { SearchStats } from '../../utils/core/types.js';

const PREFIX = 'lexical-v1:';
const MAX_BYTES = 1_048_576;
const MAX_ENTRIES = 10_000;
const TTL_MS = 60_000;
const MAX_MANIFESTS = 64;

type Options = {
  home?: string;
  root: string;
  query: RipgrepQuery;
  now?: number;
  maxBytes?: number;
};
type RestartReason =
  | 'invalidSnapshot'
  | 'missing'
  | 'expired'
  | 'tampered'
  | 'sourceChanged'
  | 'unsafeStorage'
  | 'queryChanged';
type Manifest = {
  version: 1;
  created: number;
  expires: number;
  queryKey: string;
  source: string;
  files: LocalSearchCodeFile[];
  stats: SearchStats;
};

function digest(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

function queryKey(query: RipgrepQuery, root: string): string {
  const controls = new Set([
    'snapshot',
    'page',
    'matchPage',
    'itemsPerPage',
    'maxFiles',
    'maxMatchesPerFile',
    'goal',
    'reasoning',
    'path',
  ]);
  return digest(
    JSON.stringify([
      root,
      Object.entries(query)
        .filter(([key, value]) => !controls.has(key) && value !== undefined)
        .sort(([a], [b]) => a.localeCompare(b)),
    ])
  );
}

/** Live continuations still rescan, but this fingerprint prevents mixing pages
 * whose results or coverage changed. Volatile scan telemetry is not identity. */
export function fingerprintLexicalResult(options: {
  query: RipgrepQuery;
  root: string;
  files: LocalSearchCodeFile[];
  stats: SearchStats;
}): string {
  const stats = Object.fromEntries(
    Object.entries(options.stats).filter(
      ([key]) => key !== 'searchTime' && key !== 'bytesSearched'
    )
  );
  const canonical = JSON.stringify(
    [queryKey(options.query, options.root), options.files, stats],
    (_key, value: unknown) => {
      if (value && typeof value === 'object' && !Array.isArray(value))
        return Object.fromEntries(
          Object.entries(value).sort(([a], [b]) => a.localeCompare(b))
        );
      return value;
    }
  );
  return `lexical-live-v1:${digest(canonical)}`;
}

function within(root: string, path: string): boolean {
  const child = relative(root, path);
  return child === '' || (!child.startsWith('..') && !isAbsolute(child));
}

/** Ignore-aware walks depend on ancestor/global ignore configuration that the
 * native search contract does not expose. Only explicit noIgnore scopes can be
 * frozen. Access-time ordering is also excluded because searching changes it.
 * Capped, unreadable, symlink-containing, or changing inventories stay live. */
export function canSnapshotLexicalQuery(query: RipgrepQuery): boolean {
  return query.noIgnore === true && query.sort !== 'accessed';
}

async function inventory(root: string): Promise<string | undefined> {
  const result = await contextUtils.queryFileSystem({
    path: root,
    recursive: true,
    includeRoot: true,
    showHidden: true,
    stopAtLimit: true,
    limit: MAX_ENTRIES,
  });
  if (
    result.wasCapped ||
    result.skipped ||
    result.permissionDenied ||
    result.entries.length === 0
  )
    return undefined;
  const paths = result.entries.map(entry => entry.path).sort();
  const records: string[] = [];
  for (let offset = 0; offset < paths.length; offset += 64) {
    const batch = await Promise.all(
      paths.slice(offset, offset + 64).map(async path => {
        if (!within(root, path)) throw new Error('Outside scope');
        const stat = await lstat(path, { bigint: true });
        if (!stat.isFile() && !stat.isDirectory())
          throw new Error('Unsupported entry');
        return [
          path,
          stat.dev,
          stat.ino,
          stat.mode,
          stat.size,
          stat.mtimeNs,
          stat.ctimeNs,
          stat.birthtimeNs,
        ]
          .map(String)
          .join('\0');
      })
    );
    records.push(...batch);
  }
  return digest(JSON.stringify(records));
}

/** Two complete native inventories detect mutations during enumeration itself.
 * Nanosecond ctime also catches same-size writes with restored mtime. */
export async function captureLexicalSource(
  root: string
): Promise<string | undefined> {
  try {
    const first = await inventory(root);
    return first && first === (await inventory(root)) ? first : undefined;
  } catch {
    return undefined;
  }
}

async function storage(
  home: string,
  root: string,
  create: boolean
): Promise<string> {
  if (create) await mkdir(home, { recursive: true, mode: 0o700 });
  const canonicalHome = await realpath(home);
  if (within(await realpath(root), canonicalHome))
    throw new Error('Cache overlaps source');
  let directory = canonicalHome;
  for (const segment of ['cache', 'lexical-pages']) {
    directory = join(directory, segment);
    if (create)
      await mkdir(directory, { mode: 0o700 }).catch(error => {
        if (error.code !== 'EEXIST') throw error;
      });
    const stat = await lstat(directory);
    if (
      !stat.isDirectory() ||
      stat.isSymbolicLink() ||
      (stat.mode & (segment === 'lexical-pages' ? 0o077 : 0o022)) !== 0 ||
      (process.getuid && stat.uid !== process.getuid())
    )
      throw new Error('Unsafe cache directory');
  }
  return directory;
}

export async function saveLexicalPageManifest(
  options: Options & {
    files: LocalSearchCodeFile[];
    stats: SearchStats;
    ttlMs?: number;
    source?: string;
  }
): Promise<
  | {
      status: 'saved';
      path: string;
      snapshot: string;
      files: LocalSearchCodeFile[];
    }
  | {
      status: 'terminal';
      reason:
        | 'manifestSizeLimit'
        | 'unsafeStorage'
        | 'incompleteScope'
        | 'sourceChanged';
    }
> {
  const { query, root } = options;
  if (
    !canSnapshotLexicalQuery(query) ||
    options.stats.capped ||
    options.stats.capReached ||
    options.stats.errorCount
  )
    return { status: 'terminal', reason: 'incompleteScope' };
  const source = await captureLexicalSource(root);
  if (!source || (options.source && options.source !== source))
    return { status: 'terminal', reason: 'sourceChanged' };
  const now = options.now ?? Date.now();
  const safe = sanitizeStructuredContent({
    files: options.files,
    stats: options.stats,
  }) as Pick<Manifest, 'files' | 'stats'>;
  const doc: Manifest = {
    version: 1,
    created: now,
    expires: now + (options.ttlMs ?? TTL_MS),
    queryKey: queryKey(query, root),
    source,
    ...safe,
  };
  const text = JSON.stringify(doc);
  if (Buffer.byteLength(text) > (options.maxBytes ?? MAX_BYTES))
    return { status: 'terminal', reason: 'manifestSizeLimit' };
  const hash = digest(text);
  let temporary: string | undefined;
  try {
    const directory = await storage(
      options.home ?? getOctocodeHome(),
      root,
      true
    );
    // Bound disk use as well as individual manifests. Eviction is an explicit
    // restart on the next request; it never changes another snapshot's identity.
    const entries = (await readdir(directory)).filter(name =>
      /^[a-f0-9]{64}\.json$/.test(name)
    );
    if (entries.length >= MAX_MANIFESTS) {
      const ages = await Promise.all(
        entries.map(async name => ({
          name,
          time: (await lstat(join(directory, name))).mtimeMs,
        }))
      );
      ages.sort((a, b) => a.time - b.time);
      await Promise.all(
        ages
          .slice(0, entries.length - MAX_MANIFESTS + 1)
          .map(entry => unlink(join(directory, entry.name)))
      );
    }
    temporary = join(directory, `${randomUUID()}.tmp`);
    const file = await open(
      temporary,
      constants.O_WRONLY |
        constants.O_CREAT |
        constants.O_EXCL |
        constants.O_NOFOLLOW,
      0o600
    );
    try {
      await file.writeFile(text);
    } finally {
      await file.close();
    }
    const path = join(directory, `${hash}.json`);
    await rename(temporary, path);
    return {
      status: 'saved',
      path,
      snapshot: `${PREFIX}${hash}`,
      files: safe.files,
    };
  } catch {
    return { status: 'terminal', reason: 'unsafeStorage' };
  } finally {
    if (temporary) await unlink(temporary).catch(() => undefined);
  }
}

export async function loadLexicalPageManifest(
  options: Options
): Promise<
  | { status: 'loaded'; files: LocalSearchCodeFile[]; stats: SearchStats }
  | { status: 'restart'; reason: RestartReason }
> {
  const snapshot = options.query.snapshot;
  if (!snapshot || !/^lexical-v1:[a-f0-9]{64}$/.test(snapshot))
    return { status: 'restart', reason: 'invalidSnapshot' };
  if (!canSnapshotLexicalQuery(options.query))
    return { status: 'restart', reason: 'queryChanged' };
  let text: string;
  try {
    const directory = await storage(
      options.home ?? getOctocodeHome(),
      options.root,
      false
    );
    const file = await open(
      join(directory, `${snapshot.slice(PREFIX.length)}.json`),
      constants.O_RDONLY | constants.O_NOFOLLOW
    );
    try {
      const stat = await file.stat();
      if (
        !stat.isFile() ||
        (stat.mode & 0o077) !== 0 ||
        (process.getuid && stat.uid !== process.getuid()) ||
        stat.size > (options.maxBytes ?? MAX_BYTES)
      )
        return { status: 'restart', reason: 'unsafeStorage' };
      const bytes = Buffer.alloc(stat.size + 1);
      const { bytesRead } = await file.read(bytes, 0, bytes.length, 0);
      if (bytesRead !== stat.size)
        return { status: 'restart', reason: 'tampered' };
      text = bytes.subarray(0, bytesRead).toString('utf8');
    } finally {
      await file.close();
    }
  } catch (error) {
    return {
      status: 'restart',
      reason:
        (error as NodeJS.ErrnoException).code === 'ENOENT'
          ? 'missing'
          : 'unsafeStorage',
    };
  }
  if (`${PREFIX}${digest(text)}` !== snapshot)
    return { status: 'restart', reason: 'tampered' };
  try {
    const doc = JSON.parse(text) as Manifest;
    if (
      doc.version !== 1 ||
      !Array.isArray(doc.files) ||
      !doc.stats ||
      !Number.isFinite(doc.expires)
    )
      return { status: 'restart', reason: 'tampered' };
    if (doc.queryKey !== queryKey(options.query, options.root))
      return { status: 'restart', reason: 'queryChanged' };
    if ((options.now ?? Date.now()) >= doc.expires)
      return { status: 'restart', reason: 'expired' };
    if (doc.source !== (await captureLexicalSource(options.root)))
      return { status: 'restart', reason: 'sourceChanged' };
    return { status: 'loaded', files: doc.files, stats: doc.stats };
  } catch {
    return { status: 'restart', reason: 'tampered' };
  }
}
