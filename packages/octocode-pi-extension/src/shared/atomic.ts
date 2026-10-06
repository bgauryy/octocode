import { createHash, randomBytes } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

/**
 * Crash-safe file replacement: write a sibling temp file, fsync it, check the target still holds what was read,
 * rename it over the target and fsync the directory. A reader sees the old bytes or the new ones, never a torn
 * file. Symlinks are written through (the link stays a link), and an existing file keeps its mode unless `mode` is set.
 * On any failure the target is untouched and no temp file stays.
 */

interface AtomicWriteOptions {
  /** File mode of the result (e.g. `0o600` for credentials), applied exactly rather than through the umask. */
  mode?: number;
}

export function sha256(data: string | Uint8Array, encoding: 'hex' | 'base64url' = 'hex'): string {
  return createHash('sha256').update(data).digest(encoding);
}

/** The SHA-256 of a regular file, or undefined when it does not exist (or is not a file). */
export function sha256File(file: string): string | undefined {
  try {
    return fs.statSync(file).isFile() ? sha256(fs.readFileSync(file)) : undefined;
  } catch {
    return undefined;
  }
}

export function changedOnDisk(display: string): Error {
  return new Error(`${display} changed on disk since it was read. Read it again before modifying it.`);
}

/** A unique hidden temp path next to `file` (same directory, so a rename is atomic). */
export function tempSibling(file: string): string {
  return path.join(path.dirname(file), `.${path.basename(file)}.${process.pid}.${randomBytes(4).toString('hex')}.tmp`);
}

async function realTarget(file: string): Promise<string> {
  try {
    return await fs.promises.realpath(file);
  } catch {
    return file;
  }
}

function realTargetSync(file: string): string {
  try {
    return fs.realpathSync(file);
  } catch {
    return file;
  }
}

async function syncDirectory(dir: string): Promise<void> {
  if (process.platform === 'win32') return;
  let handle: fs.promises.FileHandle | undefined;
  try {
    handle = await fs.promises.open(dir, 'r');
    await handle.sync();
  } catch {
    // Some filesystems cannot fsync a directory; the rename itself is still atomic.
  } finally {
    await handle?.close();
  }
}

/** Synchronous crash-safe write (temp file, fsync, rename) without a content check: for files only this process owns. */
export function atomicWriteFileSync(file: string, data: string | Uint8Array, options: AtomicWriteOptions = {}): void {
  const target = realTargetSync(file);
  const temp = tempSibling(target);
  let fd: number | undefined;
  try {
    fd = fs.openSync(temp, 'wx', options.mode ?? 0o666);
    fs.writeFileSync(fd, data);
    if (options.mode !== undefined) fs.fchmodSync(fd, options.mode);
    fs.fsyncSync(fd);
    fs.closeSync(fd);
    fd = undefined;
    fs.renameSync(temp, target);
  } catch (error) {
    if (fd !== undefined) fs.closeSync(fd);
    fs.rmSync(temp, { force: true });
    throw error;
  }
}

/**
 * Replace `file` with `data` atomically. `expectedSha`: a SHA-256 the current content must still have, `null` when
 * the file must still not exist, undefined for no check.
 */
export async function atomicWriteFile(file: string, data: string | Uint8Array, expectedSha?: string | null, options: AtomicWriteOptions = {}): Promise<void> {
  const target = await realTarget(file);
  const dir = path.dirname(target);
  const temp = tempSibling(target);
  const mode = options.mode ?? (await fs.promises.stat(target).then((stat) => stat.mode & 0o7777, () => undefined));
  let handle: fs.promises.FileHandle | undefined;
  try {
    handle = await fs.promises.open(temp, 'wx', mode ?? 0o666);
    await handle.writeFile(data);
    // open() applies the umask; an existing file (or an explicit mode) gets exactly its mode.
    if (mode !== undefined) await handle.chmod(mode);
    await handle.sync();
    await handle.close();
    handle = undefined;
    if (expectedSha !== undefined && (sha256File(target) ?? null) !== expectedSha) throw changedOnDisk(file);
    await fs.promises.rename(temp, target);
  } catch (error) {
    await handle?.close().catch(() => undefined);
    await fs.promises.rm(temp, { force: true });
    throw error;
  }
  await syncDirectory(dir);
}
