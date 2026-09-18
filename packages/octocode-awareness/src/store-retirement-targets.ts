import { lstatSync, realpathSync, renameSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, isAbsolute, parse, relative, resolve, sep } from 'node:path';
import { historyStoragePathsForIdentity } from './history-store.js';
import { StoreRetirementError } from './store-retirement-error.js';
import type { StoreRetirementReport, StoreRetirementTarget } from './store-retirement-types.js';

export const SQLITE_SUFFIXES = ['', '-wal', '-shm', '-journal'] as const;

/** Filesystem entry existence that treats dangling symlinks as present. */
export function pathEntryExists(path: string): boolean {
  try {
    lstatSync(path);
    return true;
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return false;
    throw error;
  }
}

export function exactDatabasePath(input: string): string {
  if (!input.trim() || input === ':memory:' || !isAbsolute(input) || resolve(input) !== input) {
    throw new StoreRetirementError('STORE_RETIREMENT_UNSAFE_DATABASE', 'Store retirement requires an exact absolute file-backed database path.');
  }
  let stat;
  try { stat = lstatSync(input); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      throw new StoreRetirementError('STORE_RETIREMENT_DATABASE_MISSING', `Awareness database does not exist: ${input}`);
    }
    throw error;
  }
  if (stat.isSymbolicLink() || !stat.isFile() || realpathSync(input) !== input) {
    throw new StoreRetirementError('STORE_RETIREMENT_UNSAFE_DATABASE', 'Store retirement requires an exact canonical regular database file, not a symlink or broad target.');
  }
  const root = parse(input).root;
  const home = realpathSync(homedir());
  if (input === root || input === home || dirname(input) === root || dirname(input) === home) {
    throw new StoreRetirementError('STORE_RETIREMENT_UNSAFE_DATABASE', 'Store retirement refuses a broad, filesystem-root, or home-directory database target.');
  }
  return input;
}

function assertWorkspaceShape(input: string): void {
  if (!input.trim() || !isAbsolute(input) || resolve(input) !== input) {
    throw new StoreRetirementError('STORE_RETIREMENT_UNSAFE_WORKSPACE', 'Store retirement requires exact absolute workspace paths.');
  }
  const root = parse(input).root;
  const home = realpathSync(homedir());
  if (input === root || input === home) {
    throw new StoreRetirementError('STORE_RETIREMENT_UNSAFE_WORKSPACE', 'Store retirement refuses a symlinked or broad root/home workspace target.');
  }
}

export function exactWorkspacePath(input: string): string {
  assertWorkspaceShape(input);
  let stat;
  try { stat = lstatSync(input); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      throw new StoreRetirementError('STORE_RETIREMENT_WORKSPACE_MISSING', `Recorded workspace does not exist: ${input}`);
    }
    throw error;
  }
  const canonical = realpathSync(input);
  if (stat.isSymbolicLink() || !stat.isDirectory() || canonical !== input) {
    throw new StoreRetirementError('STORE_RETIREMENT_UNSAFE_WORKSPACE', 'Store retirement refuses a symlinked or broad root/home workspace target.');
  }
  return canonical;
}

/** Recorded workspaces may have been deleted; their absent derived targets still belong in the bound report. */
export function recordedWorkspacePath(input: string): string {
  assertWorkspaceShape(input);
  if (!pathEntryExists(input)) return input;
  return exactWorkspacePath(input);
}

function assertDerivedHistoryTarget(workspace: string, storeId: string, target: string): void {
  const expected = resolve(workspace, '.octocode', '.localGit', storeId);
  if (target !== expected || relative(workspace, target).split(sep).slice(0, 2).join('/') !== '.octocode/.localGit') {
    throw new StoreRetirementError('STORE_RETIREMENT_UNSAFE_LOCAL_GIT', 'LocalGit retirement target was not derived from the exact workspace and store identity.');
  }
  let candidate = workspace;
  for (const component of relative(workspace, target).split(sep)) {
    candidate = resolve(candidate, component);
    if (!pathEntryExists(candidate)) continue;
    const stat = lstatSync(candidate);
    if (stat.isSymbolicLink() || !stat.isDirectory()) {
      throw new StoreRetirementError('STORE_RETIREMENT_UNSAFE_LOCAL_GIT', `LocalGit retirement target has an unsafe ancestor: ${candidate}`);
    }
  }
}

export function snapshotRetirementTarget(
  kind: StoreRetirementTarget['kind'],
  source: string,
  quarantine: string,
): StoreRetirementTarget {
  if (!pathEntryExists(source)) {
    return { kind, source, quarantine, exists: false, type: 'missing', device: null, inode: null, size: null, modified_ms: null };
  }
  const stat = lstatSync(source);
  if (stat.isSymbolicLink() || (!stat.isFile() && !stat.isDirectory())) {
    throw new StoreRetirementError('STORE_RETIREMENT_UNSAFE_TARGET', `Retirement target must be a regular file or directory, not a symlink: ${source}`);
  }
  if (realpathSync(source) !== source) {
    throw new StoreRetirementError('STORE_RETIREMENT_UNSAFE_TARGET', `Retirement target is not canonical: ${source}`);
  }
  const expectedDirectory = kind !== 'sqlite';
  if ((expectedDirectory && !stat.isDirectory()) || (!expectedDirectory && !stat.isFile())) {
    throw new StoreRetirementError(
      'STORE_RETIREMENT_UNSAFE_TARGET',
      `${kind === 'sqlite' ? 'SQLite' : 'LocalGit'} retirement target has the wrong filesystem type: ${source}`,
    );
  }
  return {
    kind,
    source,
    quarantine,
    exists: true,
    type: stat.isDirectory() ? 'directory' : 'file',
    device: stat.dev,
    inode: stat.ino,
    size: stat.size,
    modified_ms: stat.mtimeMs,
  };
}

export function buildRetirementTargets(
  database: string,
  storeId: string,
  workspaces: readonly string[],
  reportId: string,
): StoreRetirementTarget[] {
  const sources = new Map<string, StoreRetirementTarget['kind']>();
  for (const workspace of workspaces) {
    const storage = historyStoragePathsForIdentity({ workspace, dbPath: database }, { storeId, persisted: true })!;
    assertDerivedHistoryTarget(workspace, storeId, storage.history_root);
    sources.set(storage.history_root, 'local_git');
  }
  sources.set(resolve(`${database}.history`), 'legacy_local_git');
  for (const suffix of SQLITE_SUFFIXES) sources.set(`${database}${suffix}`, 'sqlite');
  return [...sources.entries()].map(([source, kind]) => snapshotRetirementTarget(kind, source, `${source}.retired-${reportId}`));
}

function sameTargetSnapshot(left: StoreRetirementTarget, right: StoreRetirementTarget): boolean {
  return left.kind === right.kind && left.source === right.source && left.quarantine === right.quarantine
    && left.exists === right.exists && left.type === right.type && left.device === right.device
    && left.inode === right.inode && left.size === right.size && left.modified_ms === right.modified_ms;
}

export function assertFreshRetirementTargets(report: StoreRetirementReport, targets: StoreRetirementTarget[]): void {
  if (targets.length !== report.targets.length || targets.some((target, index) => {
    const expected = report.targets[index]!;
    if (target.kind !== expected.kind || target.source !== expected.source || target.quarantine !== expected.quarantine) return true;
    return target.kind === 'sqlite' && target.source !== report.database.path
      ? false
      : !sameTargetSnapshot(target, expected);
  })) {
    throw new StoreRetirementError('STORE_RETIREMENT_STALE', 'Store retirement targets changed since report; run a new dry-run report.');
  }
  assertQuarantineDestinationsAbsent(targets);
}

export function assertRuntimeRetirementTargets(
  preflight: StoreRetirementTarget[],
  targets: StoreRetirementTarget[],
  database: string,
): void {
  if (targets.length !== preflight.length || targets.some((target, index) => {
    const expected = preflight[index]!;
    if (target.kind !== expected.kind || target.source !== expected.source || target.quarantine !== expected.quarantine) return true;
    return target.source === database || target.kind !== 'sqlite'
      ? !sameTargetSnapshot(target, expected)
      : false;
  })) {
    throw new StoreRetirementError('STORE_RETIREMENT_STALE', 'Store retirement targets changed while acquiring the writer fence; no targets were changed.');
  }
  assertQuarantineDestinationsAbsent(targets);
}

function assertQuarantineDestinationsAbsent(targets: StoreRetirementTarget[]): void {
  for (const target of targets) {
    if (pathEntryExists(target.quarantine)) {
      throw new StoreRetirementError('STORE_RETIREMENT_QUARANTINE_EXISTS', `Quarantine destination already exists: ${target.quarantine}`);
    }
  }
}

export function rollbackRetirementRenames(moved: StoreRetirementTarget[]): string[] {
  const failures: string[] = [];
  for (const target of [...moved].reverse()) {
    try {
      if (pathEntryExists(target.source) || !pathEntryExists(target.quarantine)) {
        failures.push(target.quarantine);
        continue;
      }
      renameSync(target.quarantine, target.source);
    } catch { failures.push(target.quarantine); }
  }
  return failures;
}
