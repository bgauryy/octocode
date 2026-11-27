// The per-user registry of built graphs: which result directories hold a graph of which project root, so a graph
// built with --out anywhere answers `query <project>`, `refresh`, `diff`, openGraph and MCP query_graph without its
// path. It lives in the Octocode home (OCTOCODE_HOME, else ~/.octocode), as graph-registry/<root key>/<out key>.json:
// one small file per result, written whole and renamed into place, so concurrent builds never rewrite each other's
// entries. Builds and refreshes register their result; readers prune entries whose result directory is gone. An
// entry only suggests a candidate: the reader still checks that the result's own events header names the root.
import { createHash } from 'node:crypto';
import {
  mkdirSync,
  readdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

/** One registered result: the project root it was built from and its directory. */
interface Entry {
  root: string;
  out: string;
  at: string;
}

/** The registry directory: graph-registry in the Octocode home (OCTOCODE_HOME when set, else ~/.octocode). */
export function registryHome(env: NodeJS.ProcessEnv = process.env) {
  const home = env.OCTOCODE_HOME?.trim();
  return join(
    home ? resolve(home) : join(homedir(), '.octocode'),
    'graph-registry'
  );
}

// The same directory under any spelling (a symlinked path): its real path when it exists.
export function canonicalDir(path: string) {
  const p = resolve(path);
  try {
    return realpathSync(p);
  } catch {
    return p;
  }
}

const key = (path: string) =>
  createHash('sha256').update(canonicalDir(path)).digest('hex').slice(0, 32);
const isDir = (p: string) =>
  statSync(p, { throwIfNoEntry: false })?.isDirectory() ?? false;
const list = (dir: string) => {
  try {
    return readdirSync(dir);
  } catch {
    return [];
  }
};
function readEntry(file: string): Entry | null {
  try {
    const e = JSON.parse(readFileSync(file, 'utf8')) as Partial<Entry>;
    return typeof e.root === 'string' && typeof e.out === 'string'
      ? (e as Entry)
      : null;
  } catch {
    return null;
  }
}

// Entries of one root key whose result directory is gone (or that cannot be read) are removed; the live ones return.
function liveEntries(dir: string) {
  const live: Entry[] = [];
  for (const name of list(dir).filter(n => n.endsWith('.json'))) {
    const file = join(dir, name);
    const e = readEntry(file);
    if (e && isDir(e.out)) live.push(e);
    else rmSync(file, { force: true });
  }
  if (!list(dir).length)
    try {
      rmdirSync(dir);
    } catch {
      // Another writer added an entry meanwhile.
    }
  return live;
}

/**
 * Record that `out` holds a graph of `root`. Every entry whose result directory is gone is pruned at the same time,
 * so the registry holds only results that still exist.
 */
export function registerRun(root: string, out: string, home = registryHome()) {
  for (const dir of list(home)) liveEntries(join(home, dir));
  const dir = join(home, key(root));
  const file = join(dir, `${key(out)}.json`);
  const temp = `${file}.${process.pid}.tmp`;
  const entry: Entry = {
    root: resolve(root),
    out: resolve(out),
    at: new Date().toISOString(),
  };
  // Another process's prune may remove the root's directory between mkdir and write: create it again once.
  for (let attempt = 0; ; attempt++)
    try {
      mkdirSync(dir, { recursive: true });
      writeFileSync(temp, `${JSON.stringify(entry)}\n`);
      renameSync(temp, file);
      return;
    } catch (error) {
      const gone =
        error instanceof Error && 'code' in error && error.code === 'ENOENT';
      if (!gone || attempt) throw error;
    }
}

/** registerRun for a build or refresh: a registry it cannot write is reported through `warn`, never a failure. */
export function recordRun(
  root: string,
  out: string,
  warn: (text: string) => void,
  home = registryHome()
) {
  try {
    registerRun(root, out, home);
  } catch (error) {
    warn(
      `graph registry not updated (${home}): ${error instanceof Error ? error.message : String(error)}; query this result by its directory`
    );
  }
}

/** The result directories registered for `root` that still exist; entries whose directory is gone are pruned. */
export function registeredRuns(root: string, home = registryHome()) {
  const want = canonicalDir(root);
  return liveEntries(join(home, key(root)))
    .filter(e => canonicalDir(e.root) === want)
    .map(e => e.out);
}
