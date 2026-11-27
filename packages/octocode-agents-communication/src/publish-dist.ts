import { randomUUID } from 'node:crypto';
import { existsSync, linkSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, normalize } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { processAlive, processStart } from './runtime/process-identity.ts';
import { isReply, sha256 } from './script-support.ts';

type Step = (step: string) => void | Promise<void>;

const isChunk = (path: string): boolean => /^chunk-[^/]+\.js$/.test(path);

/** Every file under `directory`, as a path relative to it; dot entries (staging directories, the lock) are skipped. */
function files(directory: string): string[] {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { recursive: true, withFileTypes: true }).filter(entry => entry.isFile())
    .map(entry => join(entry.parentPath, entry.name).slice(directory.length + 1)).filter(path => !path.split(/[\\/]/).some(part => part.startsWith('.')));
}

/** Every .js file under `directory`, as a path relative to it. */
export const outputs = (directory: string): string[] => files(directory).filter(path => path.endsWith('.js'));

/** The files `path` imports (static, side-effect and dynamic), relative to `directory`. */
function imports(directory: string, path: string): string[] {
  return [...readFileSync(join(directory, path), 'utf8').matchAll(/\b(?:from|import)\s*\(?\s*["'](\.\.?\/[^"']+)["']/g)]
    .map(([, specifier]) => normalize(join(dirname(path), specifier))).filter(target => !target.startsWith('..'));
}

/** `paths` and every file they import, transitively, among the files that exist in `directory`. */
function reachable(directory: string, paths: string[]): Set<string> {
  const seen = new Set<string>();
  const visit = (path: string): void => {
    if (seen.has(path) || !existsSync(join(directory, path))) return;
    seen.add(path);
    imports(directory, path).forEach(visit);
  };
  paths.forEach(visit);
  return seen;
}

/**
 * Moves a finished build from `staging` into `dist` while other processes may be starting from `dist`.
 * Each file is renamed into place (atomic per file, same filesystem): declarations first, those under types/ before
 * the stubs that re-export them, then each .js file after every file it imports, so chunks land before the entries that
 * load them. No entry or stub in `dist` ever points at a missing file. Chunk names carry a content hash, so a chunk
 * renamed over one of the same name has the same content. Then the files the new build no longer emits go, and only the
 * chunks the new build or the build it replaced reach are kept: a process that loaded the replaced build can still
 * import its chunks. Run it under the publish lock (stageAndPublish): another build's prune would delete chunks this one
 * has moved in but its entries do not reach yet.
 */
export async function publishDist(staging: string, dist: string, onStep: Step = () => {}): Promise<void> {
  mkdirSync(dist, { recursive: true });
  const previous = reachable(dist, outputs(dist).filter(path => !isChunk(path)));
  const fresh = outputs(staging);
  const order = files(staging).filter(path => !path.endsWith('.js')).sort((a, b) => Number(!a.startsWith('types')) - Number(!b.startsWith('types')));
  const moved = new Set<string>(order);
  const move = (path: string): void => {
    if (moved.has(path)) return;
    moved.add(path);
    imports(staging, path).filter(target => existsSync(join(staging, target))).forEach(move);
    order.push(path);
  };
  fresh.forEach(move);
  for (const path of order) {
    mkdirSync(dirname(join(dist, path)), { recursive: true });
    renameSync(join(staging, path), join(dist, path));
    await onStep(`publish ${path}`);
  }
  for (const path of files(dist).filter(file => !isChunk(file) && !moved.has(file))) {
    rmSync(join(dist, path));
    await onStep(`remove ${path}`);
  }
  const current = reachable(dist, fresh.filter(path => !isChunk(path)));
  for (const path of outputs(dist).filter(isChunk)) {
    if (previous.has(path) || current.has(path)) continue;
    rmSync(join(dist, path));
    await onStep(`prune ${path}`);
  }
}

const errorCode = (error: unknown): unknown => (error as NodeJS.ErrnoException).code;

function readText(path: string): string | undefined {
  try {
    return readFileSync(path, 'utf8');
  } catch (error) {
    if (errorCode(error) === 'ENOENT') return undefined;
    throw error;
  }
}

/** The process that wrote a lock-format text: its pid, its start time (null where it is unknown) and a unique id. */
export interface Owner { pid: number; start: string | null; id: string }

/** What a waiting build is told: the live process it waits for, the file in dist/ it holds, and since when (epoch ms). */
export interface Holder { pid: number; start: string | null; file: string; since: number }

function holderOf(owner: Owner, path: string): Holder {
  let since = Date.now();
  try { since = statSync(path).mtimeMs; } catch { /* released meanwhile */ }
  return { pid: owner.pid, start: owner.start, file: basename(path), since };
}

/** The text of a lock, a waiter's file and a reclaim marker: one JSON line naming the process that wrote it. */
export const lockText = (pid: number, start: string | null): string => `${JSON.stringify({ pid, start, id: randomUUID() })}\n`;

function parseOwner(text: string | undefined): Owner | undefined {
  try {
    const value: unknown = JSON.parse(text ?? '');
    return isReply(value) && Number.isInteger(value.pid) && (typeof value.start === 'string' || value.start === null) && typeof value.id === 'string' ? (value as unknown as Owner) : undefined;
  } catch {
    return undefined;
  }
}

/** A text no live process wrote (unreadable, another format, or an owner whose pid is gone or now names another process) is dead. */
function ownerAlive(text: string | undefined): boolean {
  const owner = parseOwner(text);
  return owner !== undefined && processAlive(owner.pid, owner.start);
}

export const waitingMessage = (holder: Holder): string =>
  `Waiting for pid ${holder.pid} (started ${holder.start ?? 'at an unknown time'}), which has held dist/${holder.file} for ${Math.round((Date.now() - holder.since) / 1000)} s. If that process is not a build, delete dist/${holder.file}.`;

/**
 * Removes `path` if it still holds `held`, whose writer is dead. A reclaimer first takes the per-text marker
 * `.publish-reclaim-<hash of held>` by hard-linking its own file to it, so only one reclaimer at a time can act on that
 * text, and it never renames or relinks the shared name: a lock a live build took meanwhile is never touched. A marker
 * left by a reclaimer that died is itself reclaimed the same way; one a live process holds is returned, to wait for.
 */
function reclaim(dist: string, path: string, held: string, mine: string): Holder | undefined {
  const marker = join(dist, `.publish-reclaim-${sha256(held).slice(0, 32)}`);
  try {
    linkSync(mine, marker);
  } catch (error) {
    if (errorCode(error) !== 'EEXIST') throw error;
    const other = readText(marker);
    if (other === undefined) return undefined;
    const owner = parseOwner(other);
    if (owner !== undefined && processAlive(owner.pid, owner.start)) return holderOf(owner, marker);
    return reclaim(dist, marker, other, mine) ?? reclaim(dist, path, held, mine);
  }
  try {
    // Only the marker holder removes a text this dead writer left, so the check and the unlink see the same file.
    if (readText(path) === held) unlinkSync(path);
  } finally {
    unlinkSync(marker);
  }
  return undefined;
}

/** Removes `dist/.publish.lock` if it still holds `held`, whose owner is dead; see `reclaim`. Returns a live marker holder. */
export function reclaimLock(dist: string, held: string): Holder | undefined {
  const mine = join(dist, `.publish-${randomUUID()}`);
  writeFileSync(mine, lockText(process.pid, processStart(process.pid)));
  try {
    return reclaim(dist, join(dist, '.publish.lock'), held, mine);
  } finally {
    rmSync(mine, { force: true });
  }
}

/** One try at the lock: true when taken, the live process to wait for (the lock's or a reclaim marker's), undefined to try again at once. */
function attempt(dist: string, lock: string, mine: string, self: string): true | Holder | undefined {
  try {
    linkSync(mine, lock);
    return true;
  } catch (error) {
    // ENOENT: a holder's sweep took this file while it was being written; write it again.
    if (errorCode(error) === 'ENOENT') {
      writeFileSync(mine, self);
      return undefined;
    }
    if (errorCode(error) !== 'EEXIST') throw error;
  }
  const held = readText(lock);
  if (held === undefined) return undefined;
  const owner = parseOwner(held);
  if (owner === undefined || !processAlive(owner.pid, owner.start)) {
    if (!existsSync(mine)) writeFileSync(mine, self);
    return reclaim(dist, lock, held, mine);
  }
  return holderOf(owner, lock);
}

/**
 * Runs `work` holding `dist/.publish.lock`. The holder's text (`lockText`: pid, start time, id) is written to a private
 * `.publish-<uuid>` file first and hard-linked to the lock name, so the lock appears whole or not at all. A build that
 * finds a live holder tells `onWait` once and waits for it; a lock whose holder is gone, or whose pid now names a
 * process that started at another time, is reclaimed.
 */
async function withPublishLock(dist: string, work: () => Promise<void>, onWait?: (holder: Holder) => void): Promise<void> {
  const lock = join(dist, '.publish.lock');
  const self = lockText(process.pid, processStart(process.pid));
  const mine = join(dist, `.publish-${randomUUID()}`);
  writeFileSync(mine, self);
  try {
    for (let waiting = false; ;) {
      const result = attempt(dist, lock, mine, self);
      if (result === true) break;
      if (result === undefined) continue;
      if (!waiting) {
        waiting = true;
        onWait?.(result);
      }
      await delay(50);
    }
  } finally {
    rmSync(mine, { force: true });
  }
  try {
    await work();
  } finally {
    // Only a dead holder's lock is ever reclaimed, so a lock that still holds this text is still this build's.
    if (readText(lock) === self) unlinkSync(lock);
  }
}

/**
 * Under the lock: removes every staging directory (no live build has one, since a build stages only while it holds the
 * lock) and every `.publish-*` waiter file or reclaim marker whose writer is gone.
 */
function sweep(dist: string): void {
  for (const name of readdirSync(dist)) {
    if (name.startsWith('.next-')) rmSync(join(dist, name), { recursive: true, force: true });
    else if (name.startsWith('.publish-') && !ownerAlive(readText(join(dist, name)))) rmSync(join(dist, name), { force: true });
  }
}

/**
 * Builds into a fresh staging directory inside `dist` (same filesystem) and publishes it, all under the publish lock,
 * so builds of different sources publish one after another. `onWait` runs once, with the holder, if this build waits.
 */
export async function stageAndPublish(dist: string, build: (staging: string) => Promise<void>, options: { onStep?: Step; onWait?: (holder: Holder) => void } = {}): Promise<void> {
  mkdirSync(dist, { recursive: true });
  await withPublishLock(dist, async () => {
    sweep(dist);
    const staging = mkdtempSync(join(dist, '.next-'));
    try {
      await build(staging);
      await publishDist(staging, dist, options.onStep);
    } finally {
      rmSync(staging, { recursive: true, force: true });
    }
  }, options.onWait);
}
