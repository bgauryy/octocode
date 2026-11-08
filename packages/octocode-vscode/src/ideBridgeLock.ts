import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';

import { errorMessage } from './errors';

/** Whether a process with this pid exists (EPERM: it exists, not ours). */
function pidAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code !== 'ESRCH';
  }
}

/**
 * Best-effort removal of locks that crashed hosts left behind: regular files
 * named `<pid>.json` in `directory`, owned by this user, whose pid is no
 * longer alive. Everything else (other names, links, directories, other
 * users' files, this process's lock) stays; a failure to list or delete is
 * left to the lock write that follows to surface.
 */
export async function pruneStaleLocks(directory: string): Promise<void> {
  let names: string[];
  try {
    names = await fs.promises.readdir(directory);
  } catch {
    return;
  }
  const uid = process.getuid?.();
  await Promise.all(
    names.map(async name => {
      const match = /^(\d+)\.json$/.exec(name);
      if (!match || Number(match[1]) === process.pid) return;
      if (pidAlive(Number(match[1]))) return;
      const file = path.join(directory, name);
      try {
        const stat = await fs.promises.lstat(file);
        if (!stat.isFile() || (uid !== undefined && stat.uid !== uid)) return;
        await fs.promises.unlink(file);
      } catch {
        // Raced with another pruner or not removable: leave it.
      }
    })
  );
}

/**
 * The owner-only lock file `<home>/ide/<pid>.json` (directory 0700, file 0600)
 * that tells the native client where the bridge listens. Writes go to a temp
 * file that is renamed over the lock, serialized so the last write wins.
 */
export class LockFile {
  private queue: Promise<void> = Promise.resolve();
  private removed = false;

  constructor(readonly file: string) {}

  write(content: object): Promise<void> {
    const write = () => this.writeNow(JSON.stringify(content));
    const next = this.queue.then(write, write);
    this.queue = next.catch(() => undefined);
    return next;
  }

  /**
   * Synchronous, so the lock is gone even if the host exits right after.
   * Returns why it could not be removed; a missing file is not an error.
   */
  remove(): string | undefined {
    this.removed = true;
    try {
      fs.unlinkSync(this.file);
      return undefined;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') return undefined;
      return errorMessage(error);
    }
  }

  /** Resolves once queued writes have finished. */
  settled(): Promise<void> {
    return this.queue;
  }

  private async writeNow(content: string): Promise<void> {
    const directory = path.dirname(this.file);
    await fs.promises.mkdir(directory, { recursive: true, mode: 0o700 });
    await fs.promises.chmod(directory, 0o700);
    const temp = `${this.file}.${crypto.randomBytes(6).toString('hex')}.tmp`;
    try {
      await fs.promises.writeFile(temp, content, { mode: 0o600, flag: 'wx' });
      // Stopped while writing: do not resurrect the lock.
      if (this.removed) await fs.promises.rm(temp, { force: true });
      else await fs.promises.rename(temp, this.file);
    } catch (error) {
      await fs.promises.rm(temp, { force: true });
      throw error;
    }
  }
}
