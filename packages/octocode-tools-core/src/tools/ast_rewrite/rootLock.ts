import { createHash, randomUUID } from 'node:crypto';
import {
  mkdir,
  readFile,
  readdir,
  realpath,
  rename,
  rm,
  stat,
  writeFile,
} from 'node:fs/promises';
import { join, relative, resolve, sep } from 'node:path';
import { tmpdir } from 'node:os';

interface LockOwner {
  version: 1;
  token: string;
  pid: number;
  root: string;
  createdAt: string;
}

export interface RootLock {
  canonicalRoot: string;
  release(): Promise<void>;
}

export type RootLockResult =
  | { ok: true; lock: RootLock }
  | { ok: false; reason: 'timeout' | 'unavailable'; error: string };

interface RootLockOptions {
  timeoutMs?: number;
  pollMs?: number;
}

const LOCK_HOME = join(tmpdir(), 'octocode-ast-rewrite-locks-v1');
const GUARD = join(LOCK_HOME, '.guard');

function digest(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

function overlaps(left: string, right: string): boolean {
  const contains = (parent: string, child: string): boolean => {
    const relation = relative(parent, child);
    return (
      relation === '' || (!relation.startsWith(`..${sep}`) && relation !== '..')
    );
  };
  return contains(left, right) || contains(right, left);
}

function processIsAlive(pid: number): boolean {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === 'EPERM';
  }
}

async function readOwner(directory: string): Promise<LockOwner | undefined> {
  try {
    const value = JSON.parse(
      await readFile(join(directory, 'owner.json'), 'utf8')
    ) as Partial<LockOwner>;
    if (
      value.version !== 1 ||
      typeof value.token !== 'string' ||
      typeof value.pid !== 'number' ||
      typeof value.root !== 'string' ||
      typeof value.createdAt !== 'string'
    ) {
      return undefined;
    }
    return value as LockOwner;
  } catch {
    return undefined;
  }
}

async function removeStale(
  directory: string,
  ownerlessGraceMs = 0
): Promise<boolean> {
  const owner = await readOwner(directory);
  if (owner && processIsAlive(owner.pid)) return false;
  if (!owner && ownerlessGraceMs > 0) {
    try {
      const info = await stat(directory);
      if (Date.now() - info.mtimeMs < ownerlessGraceMs) return false;
    } catch {
      return false;
    }
  }
  const tombstone = `${directory}.stale-${randomUUID()}`;
  try {
    await rename(directory, tombstone);
    await rm(tombstone, { recursive: true, force: true });
    return true;
  } catch {
    return false;
  }
}

async function acquireGuard(
  deadline: number,
  pollMs: number
): Promise<boolean> {
  while (Date.now() <= deadline) {
    try {
      await mkdir(GUARD);
      const owner: LockOwner = {
        version: 1,
        token: randomUUID(),
        pid: process.pid,
        root: '',
        createdAt: new Date().toISOString(),
      };
      await writeFile(join(GUARD, 'owner.json'), JSON.stringify(owner), {
        flag: 'wx',
        mode: 0o600,
      });
      return true;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') return false;
      // mkdir and owner.json creation cannot be one filesystem operation. Give
      // the winning process time to publish ownership before treating a bare
      // guard directory as debris from a crashed process.
      await removeStale(GUARD, 1_000);
      await new Promise(resolveWait => setTimeout(resolveWait, pollMs));
    }
  }
  return false;
}

async function releaseOwned(directory: string, token: string): Promise<void> {
  const owner = await readOwner(directory);
  if (owner?.token === token)
    await rm(directory, { recursive: true, force: true });
}

export async function acquireRootLock(
  root: string,
  options: RootLockOptions = {}
): Promise<RootLockResult> {
  const timeoutMs = options.timeoutMs ?? 5_000;
  const pollMs = options.pollMs ?? 25;
  const deadline = Date.now() + timeoutMs;
  let canonicalRoot: string;
  try {
    canonicalRoot = await realpath(resolve(root));
    await mkdir(LOCK_HOME, { recursive: true, mode: 0o700 });
  } catch {
    return {
      ok: false,
      reason: 'unavailable',
      error: 'Lock root is unavailable.',
    };
  }

  while (Date.now() <= deadline) {
    if (!(await acquireGuard(deadline, pollMs))) break;
    let blocked = false;
    try {
      const entries = await readdir(LOCK_HOME, { withFileTypes: true });
      for (const entry of entries) {
        if (!entry.isDirectory() || !entry.name.startsWith('root-')) continue;
        const directory = join(LOCK_HOME, entry.name);
        const owner = await readOwner(directory);
        if (!owner || !processIsAlive(owner.pid)) {
          await removeStale(directory);
          continue;
        }
        if (overlaps(canonicalRoot, owner.root)) {
          blocked = true;
          break;
        }
      }
      if (!blocked) {
        const token = randomUUID();
        const directory = join(LOCK_HOME, `root-${digest(canonicalRoot)}`);
        try {
          await mkdir(directory);
          const owner: LockOwner = {
            version: 1,
            token,
            pid: process.pid,
            root: canonicalRoot,
            createdAt: new Date().toISOString(),
          };
          await writeFile(
            join(directory, 'owner.json'),
            JSON.stringify(owner),
            {
              flag: 'wx',
              mode: 0o600,
            }
          );
          return {
            ok: true,
            lock: {
              canonicalRoot,
              release: () => releaseOwned(directory, token),
            },
          };
        } catch {
          await rm(directory, { recursive: true, force: true });
        }
      }
    } finally {
      await rm(GUARD, { recursive: true, force: true });
    }
    await new Promise(resolveWait => setTimeout(resolveWait, pollMs));
  }
  return {
    ok: false,
    reason: 'timeout',
    error: `Timed out waiting for an overlapping astRewrite root lock: ${canonicalRoot}`,
  };
}
