import { createHash, randomUUID } from 'node:crypto';
import {
  lstat,
  mkdir,
  open,
  readFile,
  readdir,
  realpath,
  rename as nodeRename,
  rm,
  unlink,
  writeFile,
} from 'node:fs/promises';
import { dirname, join, relative, sep } from 'node:path';
import { tmpdir } from 'node:os';

export type JournalPhase = 'staging' | 'prepared' | 'committing' | 'committed';
export type JournalFileState = 'planned' | 'staged' | 'backed-up' | 'promoted';

export interface JournalFile {
  target: string;
  stage: string;
  backup: string;
  beforeHash: string;
  afterHash: string;
  mode: number;
  state: JournalFileState;
}

export interface TransactionJournal {
  version: 1;
  id: string;
  root: string;
  phase: JournalPhase;
  createdAt: string;
  files: JournalFile[];
  path: string;
}

export interface RecoveryResult {
  ok: boolean;
  recovered: number;
  errors: string[];
}

type RenameFile = (from: string, to: string) => Promise<void>;

const JOURNAL_HOME = join(tmpdir(), 'octocode-ast-rewrite-transactions-v1');

function sha256(value: string | Buffer): string {
  return createHash('sha256').update(value).digest('hex');
}

function journalDirectory(root: string): string {
  return join(JOURNAL_HOME, sha256(root));
}

function within(root: string, target: string): boolean {
  const relation = relative(root, target);
  return (
    relation === '' || (!relation.startsWith(`..${sep}`) && relation !== '..')
  );
}

async function exists(path: string): Promise<boolean> {
  try {
    await lstat(path);
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false;
    throw error;
  }
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

async function hashAt(path: string): Promise<string | undefined> {
  try {
    return sha256(await readFile(path));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return undefined;
    throw error;
  }
}

function plainJournal(
  journal: TransactionJournal
): Omit<TransactionJournal, 'path'> {
  const { path: _path, ...plain } = journal;
  return plain;
}

export async function persistJournal(
  journal: TransactionJournal
): Promise<void> {
  const directory = dirname(journal.path);
  await mkdir(directory, { recursive: true, mode: 0o700 });
  const temporary = `${journal.path}.${randomUUID()}.tmp`;
  await writeFile(temporary, JSON.stringify(plainJournal(journal)), {
    flag: 'wx',
    mode: 0o600,
  });
  await syncPath(temporary);
  await nodeRename(temporary, journal.path);
  try {
    await syncPath(directory);
  } catch {
    // Directory fsync is not supported uniformly.
  }
}

export async function createJournal(
  root: string,
  id: string,
  files: Array<{
    target: string;
    stage: string;
    backup: string;
    beforeHash: string;
    afterHash: string;
    mode: number;
  }>
): Promise<TransactionJournal> {
  const journal: TransactionJournal = {
    version: 1,
    id,
    root,
    phase: 'staging',
    createdAt: new Date().toISOString(),
    files: files.map(file => ({ ...file, state: 'planned' })),
    path: join(journalDirectory(root), `${id}.json`),
  };
  await persistJournal(journal);
  return journal;
}

function validateJournal(
  value: unknown,
  path: string,
  expectedRoot: string
): TransactionJournal | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const candidate = value as Partial<TransactionJournal>;
  if (
    candidate.version !== 1 ||
    typeof candidate.id !== 'string' ||
    candidate.root !== expectedRoot ||
    !['staging', 'prepared', 'committing', 'committed'].includes(
      candidate.phase ?? ''
    ) ||
    !Array.isArray(candidate.files)
  ) {
    return undefined;
  }
  const files: JournalFile[] = [];
  for (let index = 0; index < candidate.files.length; index += 1) {
    const file = candidate.files[index] as Partial<JournalFile>;
    if (
      typeof file.target !== 'string' ||
      typeof file.stage !== 'string' ||
      typeof file.backup !== 'string' ||
      typeof file.beforeHash !== 'string' ||
      typeof file.afterHash !== 'string' ||
      typeof file.mode !== 'number' ||
      !['planned', 'staged', 'backed-up', 'promoted'].includes(
        file.state ?? ''
      ) ||
      !within(expectedRoot, file.target) ||
      file.stage !==
        join(
          dirname(file.target),
          `.octocode-${candidate.id}.stage-${index}`
        ) ||
      file.backup !==
        join(dirname(file.target), `.octocode-${candidate.id}.backup-${index}`)
    ) {
      return undefined;
    }
    files.push(file as JournalFile);
  }
  return {
    version: 1,
    id: candidate.id,
    root: expectedRoot,
    phase: candidate.phase as JournalPhase,
    createdAt: String(candidate.createdAt ?? ''),
    files,
    path,
  };
}

async function readJournal(
  path: string,
  root: string
): Promise<TransactionJournal | undefined> {
  try {
    return validateJournal(
      JSON.parse(await readFile(path, 'utf8')),
      path,
      root
    );
  } catch {
    return undefined;
  }
}

async function restoreBefore(
  journal: TransactionJournal,
  renameFile: RenameFile
): Promise<void> {
  for (const file of [...journal.files].reverse()) {
    const targetHash = await hashAt(file.target);
    const backupHash = await hashAt(file.backup);
    if (backupHash !== undefined) {
      if (backupHash !== file.beforeHash) {
        // The target may have changed in the narrow interval between the last
        // validation and the target->backup rename. When the target is now
        // absent, the backup is the only copy of those newer user bytes. Put it
        // back instead of deleting or marooning the concurrent edit.
        if (targetHash === undefined) {
          await renameFile(file.backup, file.target);
          await safeUnlink(file.stage);
          continue;
        }
        throw new Error(`Backup hash mismatch: ${file.target}`);
      }
      if (
        targetHash !== undefined &&
        targetHash !== file.beforeHash &&
        targetHash !== file.afterHash
      ) {
        throw new Error(
          `External target change blocks recovery: ${file.target}`
        );
      }
      if (targetHash === file.beforeHash) {
        await safeUnlink(file.backup);
      } else {
        await safeUnlink(file.target);
        await renameFile(file.backup, file.target);
      }
    } else if (targetHash !== file.beforeHash) {
      throw new Error(
        `Original bytes unavailable for recovery: ${file.target}`
      );
    }
    await safeUnlink(file.stage);
  }
}

async function finishAfter(journal: TransactionJournal): Promise<void> {
  for (const file of journal.files) {
    if ((await hashAt(file.target)) !== file.afterHash) {
      throw new Error(`Committed target hash mismatch: ${file.target}`);
    }
    await safeUnlink(file.stage);
    await safeUnlink(file.backup);
  }
}

async function removeJournal(journal: TransactionJournal): Promise<void> {
  await safeUnlink(journal.path);
  const directory = dirname(journal.path);
  try {
    if ((await readdir(directory)).length === 0)
      await rm(directory, { recursive: true });
  } catch {
    // A concurrent transaction may still own the directory.
  }
}

export async function recoverTransactions(
  root: string,
  renameFile: RenameFile = nodeRename
): Promise<RecoveryResult> {
  let canonicalRoot: string;
  try {
    canonicalRoot = await realpath(root);
  } catch (error) {
    return { ok: false, recovered: 0, errors: [String(error)] };
  }
  const directory = journalDirectory(canonicalRoot);
  let paths: string[];
  try {
    paths = (await readdir(directory))
      .filter(name => name.endsWith('.json'))
      .sort()
      .map(name => join(directory, name));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      return { ok: true, recovered: 0, errors: [] };
    }
    return { ok: false, recovered: 0, errors: [String(error)] };
  }

  let recovered = 0;
  const errors: string[] = [];
  for (const path of paths) {
    const journal = await readJournal(path, canonicalRoot);
    if (!journal) {
      errors.push(`Invalid transaction journal: ${path}`);
      continue;
    }
    try {
      if (journal.phase === 'committed') await finishAfter(journal);
      else await restoreBefore(journal, renameFile);
      await removeJournal(journal);
      recovered += 1;
    } catch (error) {
      errors.push(error instanceof Error ? error.message : String(error));
    }
  }
  return { ok: errors.length === 0, recovered, errors };
}

export async function journalExists(
  journal: TransactionJournal
): Promise<boolean> {
  return exists(journal.path);
}
