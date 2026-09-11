import {
  chmod,
  lstat,
  open,
  readFile,
  rename as nodeRename,
  unlink,
  writeFile,
} from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { randomUUID } from 'node:crypto';

export interface TransactionFile {
  absolutePath: string;
  before: Buffer;
  after: Buffer;
  mode: number;
}

export interface TransactionReceipt {
  id: string;
  committed: true;
  files: number;
  cleanupWarnings?: string[];
}

export type TransactionResult =
  | { ok: true; receipt: TransactionReceipt }
  | {
      ok: false;
      error: string;
      rollback: { restored: boolean; files: number; errors: string[] };
    };

let applyTail: Promise<void> = Promise.resolve();

/** Serialize applies in this process so overlapping rewrites cannot race. */
export function serializeApply<T>(operation: () => Promise<T>): Promise<T> {
  const result = applyTail.then(operation, operation);
  applyTail = result.then(
    () => undefined,
    () => undefined
  );
  return result;
}

async function safeUnlink(path: string): Promise<void> {
  try {
    await unlink(path);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
}

async function syncPath(path: string): Promise<void> {
  const handle = await open(path, 'r');
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

export async function applyTransaction(
  files: TransactionFile[],
  renameFile: (from: string, to: string) => Promise<void> = nodeRename
): Promise<TransactionResult> {
  const id = randomUUID();
  const prepared = files.map((file, index) => ({
    ...file,
    stage: join(dirname(file.absolutePath), `.octocode-${id}.stage-${index}`),
    backup: join(dirname(file.absolutePath), `.octocode-${id}.backup-${index}`),
    backedUp: false,
    promoted: false,
  }));

  try {
    for (const file of prepared) {
      await writeFile(file.stage, file.after, { flag: 'wx', mode: file.mode });
      await chmod(file.stage, file.mode);
      await syncPath(file.stage);
    }

    // Recheck bytes and file identity after all staged outputs exist. No target is
    // changed until the complete transaction has passed this final gate.
    for (const file of prepared) {
      const info = await lstat(file.absolutePath);
      if (!info.isFile() || info.isSymbolicLink()) {
        throw new Error(`Target identity changed: ${file.absolutePath}`);
      }
      const current = await readFile(file.absolutePath);
      if (!current.equals(file.before)) {
        throw new Error(`Target bytes changed: ${file.absolutePath}`);
      }
    }

    for (const file of prepared) {
      await renameFile(file.absolutePath, file.backup);
      file.backedUp = true;
      const backupInfo = await lstat(file.backup);
      const backedUpBytes = await readFile(file.backup);
      if (
        !backupInfo.isFile() ||
        backupInfo.isSymbolicLink() ||
        !backedUpBytes.equals(file.before)
      ) {
        throw new Error(`Target changed during commit: ${file.absolutePath}`);
      }
      await renameFile(file.stage, file.absolutePath);
      file.promoted = true;
      await syncPath(file.absolutePath);
    }
    for (const directory of new Set(
      prepared.map(file => dirname(file.absolutePath))
    )) {
      try {
        await syncPath(directory);
      } catch {
        // Directory fsync is not supported uniformly (notably on Windows).
      }
    }
    // Backup cleanup happens after commit and must not turn a committed rewrite
    // into a misleading rollback attempt if the filesystem denies cleanup.
    const cleanupWarnings: string[] = [];
    for (const file of prepared) {
      try {
        await safeUnlink(file.backup);
      } catch (cleanupError) {
        cleanupWarnings.push(
          cleanupError instanceof Error
            ? cleanupError.message
            : String(cleanupError)
        );
      }
    }
    return {
      ok: true,
      receipt: {
        id,
        committed: true,
        files: files.length,
        ...(cleanupWarnings.length > 0 ? { cleanupWarnings } : {}),
      },
    };
  } catch (error) {
    const errors: string[] = [];
    let restored = 0;
    for (const file of [...prepared].reverse()) {
      try {
        if (file.promoted) await safeUnlink(file.absolutePath);
        if (file.backedUp) {
          await renameFile(file.backup, file.absolutePath);
          restored += 1;
        }
      } catch (rollbackError) {
        errors.push(
          rollbackError instanceof Error
            ? rollbackError.message
            : String(rollbackError)
        );
      }
      try {
        await safeUnlink(file.stage);
      } catch (cleanupError) {
        errors.push(
          cleanupError instanceof Error
            ? cleanupError.message
            : String(cleanupError)
        );
      }
    }
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
      rollback: {
        restored: errors.length === 0,
        files: restored,
        errors,
      },
    };
  }
}
