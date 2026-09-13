import {
  chmod,
  lstat,
  open,
  readFile,
  realpath,
  rename as nodeRename,
  writeFile,
} from 'node:fs/promises';
import { createHash, randomUUID } from 'node:crypto';
import { dirname, join } from 'node:path';
import {
  createJournal,
  persistJournal,
  recoverTransactions,
  type TransactionJournal,
} from './journal.js';

export { recoverTransactions } from './journal.js';

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

export type TransactionFaultPoint =
  | 'after-journal'
  | `after-stage:${number}`
  | 'after-prepared'
  | `after-backup:${number}`
  | `after-promote:${number}`
  | 'after-commit';

export interface TransactionOptions {
  rootBoundary?: string;
  fault?: (point: TransactionFaultPoint) => void | Promise<void>;
}

export type TransactionResult =
  | { ok: true; receipt: TransactionReceipt }
  | {
      ok: false;
      error: string;
      interrupted?: boolean;
      journalPath?: string;
      rollback: { restored: boolean; files: number; errors: string[] };
    };

/** Test-only process-death surrogate: state is deliberately left for recovery. */
export class SimulatedTransactionCrash extends Error {
  constructor(point: string) {
    super(`Simulated process interruption at ${point}`);
    this.name = 'SimulatedTransactionCrash';
  }
}

let applyTail: Promise<void> = Promise.resolve();

/** Serialize applies in this process; filesystem locks coordinate processes. */
export function serializeApply<T>(operation: () => Promise<T>): Promise<T> {
  const result = applyTail.then(operation, operation);
  applyTail = result.then(
    () => undefined,
    () => undefined
  );
  return result;
}

function sha256(value: Buffer): string {
  return createHash('sha256').update(value).digest('hex');
}

async function syncPath(path: string): Promise<void> {
  const handle = await open(path, 'r');
  try {
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function invokeFault(
  options: TransactionOptions,
  point: TransactionFaultPoint
): Promise<void> {
  await options.fault?.(point);
}

async function rootFor(
  files: TransactionFile[],
  explicit?: string
): Promise<string> {
  if (files.length === 0)
    throw new Error('A transaction requires at least one file.');
  return realpath(explicit ?? dirname(files[0]?.absolutePath ?? ''));
}

async function stageFiles(
  files: TransactionFile[],
  journal: TransactionJournal,
  options: TransactionOptions
): Promise<void> {
  for (let index = 0; index < files.length; index += 1) {
    const file = files[index];
    const journalFile = journal.files[index];
    if (!file || !journalFile) throw new Error('Transaction journal mismatch.');
    await writeFile(journalFile.stage, file.after, {
      flag: 'wx',
      mode: file.mode,
    });
    await chmod(journalFile.stage, file.mode);
    await syncPath(journalFile.stage);
    journalFile.state = 'staged';
    await persistJournal(journal);
    await invokeFault(options, `after-stage:${index}`);
  }
  journal.phase = 'prepared';
  await persistJournal(journal);
  await invokeFault(options, 'after-prepared');
}

async function validateTargets(files: TransactionFile[]): Promise<void> {
  for (const file of files) {
    const info = await lstat(file.absolutePath);
    if (!info.isFile() || info.isSymbolicLink()) {
      throw new Error(`Target identity changed: ${file.absolutePath}`);
    }
    if (!(await readFile(file.absolutePath)).equals(file.before)) {
      throw new Error(`Target bytes changed: ${file.absolutePath}`);
    }
  }
}

async function promoteFiles(
  files: TransactionFile[],
  journal: TransactionJournal,
  renameFile: (from: string, to: string) => Promise<void>,
  options: TransactionOptions
): Promise<void> {
  journal.phase = 'committing';
  await persistJournal(journal);
  for (let index = 0; index < files.length; index += 1) {
    const file = files[index];
    const journalFile = journal.files[index];
    if (!file || !journalFile) throw new Error('Transaction journal mismatch.');
    await renameFile(file.absolutePath, journalFile.backup);
    journalFile.state = 'backed-up';
    await persistJournal(journal);
    await invokeFault(options, `after-backup:${index}`);
    const backupInfo = await lstat(journalFile.backup);
    const backupBytes = await readFile(journalFile.backup);
    if (
      !backupInfo.isFile() ||
      backupInfo.isSymbolicLink() ||
      !backupBytes.equals(file.before)
    ) {
      throw new Error(`Target changed during commit: ${file.absolutePath}`);
    }
    await renameFile(journalFile.stage, file.absolutePath);
    journalFile.state = 'promoted';
    await syncPath(file.absolutePath);
    await persistJournal(journal);
    await invokeFault(options, `after-promote:${index}`);
  }
  for (const directory of new Set(
    files.map(file => dirname(file.absolutePath))
  )) {
    try {
      await syncPath(directory);
    } catch {
      // Directory fsync is not supported uniformly.
    }
  }
  journal.phase = 'committed';
  await persistJournal(journal);
}

export async function applyTransaction(
  files: TransactionFile[],
  renameFile: (from: string, to: string) => Promise<void> = nodeRename,
  options: TransactionOptions = {}
): Promise<TransactionResult> {
  const id = randomUUID();
  const rootBoundary = await rootFor(files, options.rootBoundary);
  const transactionFiles = await Promise.all(
    files.map(async file => ({
      ...file,
      absolutePath: await realpath(file.absolutePath),
    }))
  );
  let journal: TransactionJournal | undefined;
  let committed = false;
  try {
    journal = await createJournal(
      rootBoundary,
      id,
      transactionFiles.map((file, index) => ({
        target: file.absolutePath,
        stage: join(
          dirname(file.absolutePath),
          `.octocode-${id}.stage-${index}`
        ),
        backup: join(
          dirname(file.absolutePath),
          `.octocode-${id}.backup-${index}`
        ),
        beforeHash: sha256(file.before),
        afterHash: sha256(file.after),
        mode: file.mode,
      }))
    );
    await invokeFault(options, 'after-journal');
    await stageFiles(transactionFiles, journal, options);
    await validateTargets(transactionFiles);
    await promoteFiles(transactionFiles, journal, renameFile, options);
    committed = true;
    await invokeFault(options, 'after-commit');
    const finalized = await recoverTransactions(rootBoundary, renameFile);
    if (!finalized.ok) throw new Error(finalized.errors.join('; '));
    return {
      ok: true,
      receipt: { id, committed: true, files: transactionFiles.length },
    };
  } catch (error) {
    if (error instanceof SimulatedTransactionCrash) {
      return {
        ok: false,
        error: error.message,
        interrupted: true,
        journalPath: journal?.path,
        rollback: { restored: false, files: 0, errors: [] },
      };
    }
    if (committed) {
      return {
        ok: true,
        receipt: {
          id,
          committed: true,
          files: transactionFiles.length,
          cleanupWarnings: [
            error instanceof Error ? error.message : String(error),
          ],
        },
      };
    }
    const recovered = await recoverTransactions(rootBoundary, renameFile);
    return {
      ok: false,
      error: error instanceof Error ? error.message : String(error),
      journalPath: journal?.path,
      rollback: {
        restored: recovered.ok,
        files: recovered.recovered,
        errors: recovered.errors,
      },
    };
  }
}
