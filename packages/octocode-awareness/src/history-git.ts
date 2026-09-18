import { lstat, opendir, realpath, stat } from 'node:fs/promises';
import { relative, resolve, sep } from 'node:path';
import * as git from 'isomorphic-git';
import { inspectOrphanObjects, reclaimOrphanObjects, type HistoryMaintenanceResult } from './history-git-maintenance.js';
import { ensureHistoryIgnoreMarker } from './history-ignore.js';
import { readHistoryObject, parseHistoryTree, parseHistoryCommit, MAX_HISTORY_OBJECT_BYTES } from './history-git-object.js';
import { createPrivateHistoryIo } from './history-git-storage.js';
import type { FileDurability } from '@octocodeai/octocode-extension-rust';

export function historyGitBackend() {
  return { name: 'isomorphic-git', version: git.version(), bundled: true, system_git_required: false };
}

export type HistoryFileMode = '100644' | '100755' | '120000';

export interface HistoryTreeEntry {
  path: string;
  oid: string;
  mode: HistoryFileMode;
}

export interface HistoryBlob {
  oid: string;
  size: number;
}

export interface HistoryCommitInput {
  tree: string;
  parents?: string[];
  message: string;
  timestampMs?: number;
}

export interface HistoryCommit {
  oid: string;
  tree: string;
  parents: string[];
  message: string;
  timestampMs: number;
}

export interface HistoryGitStore {
  readonly rootDir: string;
  readonly gitdir: string;
  writeBlob(bytes: Uint8Array): Promise<HistoryBlob>;
  readBlob(oid: string, maxBytes?: number, signal?: AbortSignal): Promise<Uint8Array>;
  writeTree(entries: HistoryTreeEntry[]): Promise<string>;
  readTree(oid: string, signal?: AbortSignal): Promise<HistoryTreeEntry[]>;
  writeCommit(input: HistoryCommitInput): Promise<string>;
  readCommit(oid: string, signal?: AbortSignal): Promise<HistoryCommit>;
  verifyObject(oid: string, expectedType?: 'blob' | 'tree' | 'commit'): Promise<boolean>;
  publishRef(ref: string, oid: string): Promise<void>;
  resolveRef(ref: string): Promise<string | null>;
  flush(): Promise<FileDurability>;
  inspectOrphanObjects(options: { retainedOids: string[]; graceMs: number; limit: number; cursor?: string }): Promise<HistoryMaintenanceResult>;
  /** Caller must hold the Awareness metadata maintenance handshake. */
  reclaimOrphanObjects(options: { retainedOids: string[]; graceMs: number; limit: number; cursor?: string }): Promise<HistoryMaintenanceResult>;
}

export interface OpenHistoryGitStoreOptions {
  historyRoot: string;
  storeId: string;
  workspaceId: string;
  boundaryRoot?: string;
  ignoreMarkerPath?: string;
  readOnly?: boolean;
}

const OID = /^[0-9a-f]{40}$/;
const HISTORY_REF = /^refs\/octocode\/(?:[0-9a-f]{64}\/(before|after)|experiences\/[0-9a-f]{64})$/;
const initializationLocks = new Map<string, Promise<void>>();

function validateIdentity(label: string, value: string): void {
  if (!/^(?:[a-z][a-z0-9-]{0,63}|[0-9a-f]{64})$/.test(value)) throw new Error(`invalid ${label}: ${JSON.stringify(value)}`);
}

function validateOid(oid: string): void {
  if (!OID.test(oid)) throw new Error(`invalid object id: ${JSON.stringify(oid)}`);
}

async function rejectSymlink(path: string, label: string): Promise<void> {
  try {
    if ((await lstat(path)).isSymbolicLink()) throw new Error(`${label} must not be a symlink: ${path}`);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
}

async function withInitializationLock(key: string, task: () => Promise<void>): Promise<void> {
  const previous = initializationLocks.get(key) ?? Promise.resolve();
  let release!: () => void;
  const current = new Promise<void>(resolveLock => { release = resolveLock; });
  const next = previous.then(() => current);
  initializationLocks.set(key, next);
  await previous;
  try {
    await task();
  } finally {
    release();
    if (initializationLocks.get(key) === next) initializationLocks.delete(key);
  }
}

async function assertDirectoryChain(boundaryRoot: string, target: string, label: string): Promise<void> {
  const boundary = resolve(boundaryRoot);
  const destination = resolve(target);
  const remainder = relative(boundary, destination);
  if (remainder === '..' || remainder.startsWith(`..${sep}`) || resolve(boundary, remainder) !== destination) {
    throw new Error(`${label} escaped its boundary`);
  }
  let cursor = boundary;
  for (const part of remainder ? remainder.split(sep) : []) {
    await rejectSymlink(cursor, label);
    try {
      if (!(await stat(cursor)).isDirectory()) throw new Error(`${label} must be a directory: ${cursor}`);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
    cursor = resolve(cursor, part);
  }
  await rejectSymlink(cursor, label);
  try {
    if (!(await stat(cursor)).isDirectory()) throw new Error(`${label} must be a directory: ${cursor}`);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
  }
}

function normalizeHistoryPath(value: string): string {
  if (!value || value.includes('\0') || value.includes('\\') || value.startsWith('/') || value.endsWith('/')) {
    throw new Error(`invalid history path: ${JSON.stringify(value)}`);
  }
  const parts = value.split('/');
  if (parts.some(part => !part || part === '.' || part === '..')) throw new Error(`invalid history path: ${JSON.stringify(value)}`);
  return parts.join('/');
}

type TreeNode = Map<string, TreeNode | HistoryTreeEntry>;

export async function openHistoryGitStore(options: OpenHistoryGitStoreOptions): Promise<HistoryGitStore> {
  validateIdentity('store id', options.storeId);
  validateIdentity('workspace id', options.workspaceId);
  const historyRoot = resolve(options.historyRoot);
  const boundaryRoot = options.boundaryRoot ? resolve(options.boundaryRoot) : undefined;
  const ignoreMarkerPath = options.ignoreMarkerPath ? resolve(options.ignoreMarkerPath) : undefined;
  if (ignoreMarkerPath && ignoreMarkerPath !== resolve(historyRoot, '..', '.gitignore')) {
    throw new Error('history ignore marker must be the .gitignore beside its private history root');
  }
  if (boundaryRoot && historyRoot !== boundaryRoot && !historyRoot.startsWith(`${boundaryRoot}${sep}`)) throw new Error('history store escaped its boundary');
  const rootDir = resolve(historyRoot, options.storeId, options.workspaceId);
  if (rootDir !== historyRoot && !rootDir.startsWith(`${historyRoot}${sep}`)) throw new Error('history store escaped its root');
  const storeDir = resolve(historyRoot, options.storeId);
  const gitdir = resolve(rootDir, 'repo.git');
  if (boundaryRoot) await assertDirectoryChain(boundaryRoot, historyRoot, 'history boundary ancestor');
  await rejectSymlink(historyRoot, 'history root');
  await rejectSymlink(storeDir, 'history store directory');
  await rejectSymlink(rootDir, 'history workspace directory');
  // Resolve the existing anchor once (including OS aliases such as /var on macOS).
  // Native code creates every missing component without following symlinks.
  let anchor = resolve(historyRoot, '..');
  for (;;) {
    try { await lstat(anchor); break; }
    catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
      const parent = resolve(anchor, '..');
      if (parent === anchor) throw error;
      anchor = parent;
    }
  }
  const canonicalRoot = resolve(await realpath(anchor), relative(anchor, historyRoot));
  const canonicalStore = resolve(canonicalRoot, options.storeId, options.workspaceId);
  const canonicalGitdir = resolve(canonicalStore, 'repo.git');
  const { privateFs, readPrivateFile, writePrivateFile, privateObject, assertWritable, flush } = await createPrivateHistoryIo(rootDir, canonicalStore, canonicalGitdir, options.readOnly);
  if (!canonicalStore.startsWith(`${canonicalRoot}${sep}`)) throw new Error('history store resolved outside its root');
  const assertMetadataSafe = async (ref?: string): Promise<void> => {
    if (ignoreMarkerPath && !options.readOnly) await ensureHistoryIgnoreMarker(ignoreMarkerPath, boundaryRoot, assertDirectoryChain, rejectSymlink);
    if (boundaryRoot) await assertDirectoryChain(boundaryRoot, historyRoot, 'history boundary ancestor');
    for (const [candidate, label] of [[historyRoot, 'history root'], [storeDir, 'history store directory'], [rootDir, 'history workspace directory'], [gitdir, 'Git directory'], [resolve(gitdir, 'objects'), 'Git objects directory'], [resolve(gitdir, 'refs'), 'Git refs directory']] as const) {
      await rejectSymlink(candidate, label);
    }
    try {
      let count = 0;
      for await (const entry of await opendir(resolve(gitdir, 'objects'))) {
        if (++count > 1024) throw new Error('HISTORY_OBJECT_LIMIT: unexpected object directory fanout');
        if (entry.isSymbolicLink()) throw new Error(`Git object ancestor must not be a symlink: ${entry.name}`);
      }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
    if (ref) {
      let cursor = gitdir;
      for (const part of ref.split('/').slice(0, -1)) {
        cursor = resolve(cursor, part);
        await rejectSymlink(cursor, 'Git ref ancestor');
      }
    }
  };
  const markerPath = resolve(rootDir, 'history-store.json');
  const marker = { formatVersion: 1, storeId: options.storeId, workspaceId: options.workspaceId, objectFormat: 'sha1' } as const;
  const validateMarker = async (): Promise<void> => {
    const existing = JSON.parse(await readPrivateFile(markerPath, 'utf8') as string) as unknown;
    if (JSON.stringify(existing) !== JSON.stringify(marker)) throw new Error('history store identity marker does not match requested store');
  };
  const ensureMarker = async (): Promise<void> => {
    try {
      await validateMarker();
      return;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
    try {
      await writePrivateFile(markerPath, `${JSON.stringify(marker)}\n`, true);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST' && !(error instanceof Error && /PRECONDITION_FAILED/.test(error.message))) throw error;
      await validateMarker();
    }
  };
  await withInitializationLock(rootDir, async () => {
    if (options.readOnly) {
      await assertMetadataSafe();
      try { await validateMarker(); }
      catch (error) {
        if ((error as NodeJS.ErrnoException).code === 'ENOENT') throw new Error('HISTORY_STORE_UNAVAILABLE: source history store is missing');
        throw error;
      }
      return;
    }
    if (ignoreMarkerPath) await ensureHistoryIgnoreMarker(ignoreMarkerPath, boundaryRoot, assertDirectoryChain, rejectSymlink);
    await assertMetadataSafe();
    await ensureMarker();
    await rejectSymlink(gitdir, 'Git directory');
    await git.init({ fs: privateFs, dir: rootDir, gitdir, bare: true, defaultBranch: 'octocode' });
    await assertMetadataSafe();
  });

  const writeTreeNode = async (node: TreeNode): Promise<string> => {
    await assertMetadataSafe();
    const tree: Array<{ mode: string; path: string; oid: string; type: 'blob' | 'tree' }> = [];
    for (const name of [...node.keys()].sort()) {
      const value = node.get(name)!;
      if (value instanceof Map) {
        tree.push({ mode: '040000', path: name, oid: await writeTreeNode(value), type: 'tree' });
      } else {
        tree.push({ mode: value.mode, path: name, oid: value.oid, type: 'blob' });
      }
    }
    const oid = await git.writeTree({ fs: privateFs, dir: rootDir, gitdir, tree });
    await privateObject(oid);
    return oid;
  };

  const resolveHistoryRef = async (ref: string): Promise<string | null> => {
    if (!HISTORY_REF.test(ref)) throw new Error(`invalid history ref: ${JSON.stringify(ref)}`);
    await assertMetadataSafe(ref);
    try {
      const value = (await readPrivateFile(resolve(gitdir, ref), 'utf8') as string).trim();
      validateOid(value);
      return value;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') return null;
      throw error;
    }
  };

  return {
    rootDir,
    gitdir,
    async writeBlob(bytes) {
      assertWritable();
      await assertMetadataSafe();
      if (bytes.byteLength > MAX_HISTORY_OBJECT_BYTES) throw new Error('HISTORY_OBJECT_LIMIT: blob exceeds write limit');
      const copy = Uint8Array.from(bytes);
      const oid = await git.writeBlob({ fs: privateFs, dir: rootDir, gitdir, blob: copy });
      await privateObject(oid);
      return { oid, size: copy.byteLength };
    },
    async readBlob(oid, maxBytes = 16 * 1024 * 1024, signal) {
      signal?.throwIfAborted();
      await assertMetadataSafe();
      validateOid(oid);
      const object = await readHistoryObject(canonicalGitdir, oid, maxBytes, true, signal);
      if (object.objectType !== 'blob') throw new Error('HISTORY_OBJECT_INVALID: expected blob');
      return object.content!;
    },
    async writeTree(entries) {
      assertWritable();
      const root: TreeNode = new Map();
      const seen = new Set<string>();
      for (const raw of entries) {
        const entry = { ...raw, path: normalizeHistoryPath(raw.path) };
        validateOid(entry.oid);
        if (!['100644', '100755', '120000'].includes(entry.mode)) throw new Error(`invalid history mode: ${entry.mode}`);
        if (seen.has(entry.path)) throw new Error(`duplicate history path: ${entry.path}`);
        seen.add(entry.path);
        const parts = entry.path.split('/');
        let node = root;
        for (const part of parts.slice(0, -1)) {
          const existing = node.get(part);
          if (existing && !(existing instanceof Map)) throw new Error(`history path conflicts with file: ${entry.path}`);
          if (!existing) node.set(part, new Map());
          node = node.get(part) as TreeNode;
        }
        const leaf = parts.at(-1)!;
        if (node.has(leaf)) throw new Error(`history path conflicts with directory: ${entry.path}`);
        node.set(leaf, entry);
      }
      const oid = await writeTreeNode(root);
      await privateObject(oid);
      return oid;
    },
    async readTree(oid, signal) {
      signal?.throwIfAborted();
      await assertMetadataSafe();
      validateOid(oid);
      const result: HistoryTreeEntry[] = [];
      const deadline = Date.now() + 10_000;
      let visited = 0;
      let bytes = 0;
      let pathBytes = 0;
      const visit = async (treeOid: string, prefix: string, depth = 0): Promise<void> => {
        if (++visited > 10_000 || depth > 128 || Date.now() >= deadline) throw new Error('HISTORY_OBJECT_LIMIT: tree traversal exceeds bounded work');
        const object = await readHistoryObject(canonicalGitdir, treeOid, Math.min(MAX_HISTORY_OBJECT_BYTES, 64 * 1024 * 1024 - bytes), true, signal, Math.max(0, deadline - Date.now()));
        bytes += object.size;
        const tree = parseHistoryTree(object);
        for (const entry of tree) {
          if (prefix.length + entry.path.length + 1 > 32768) throw new Error('HISTORY_OBJECT_LIMIT: tree path exceeds length limit');
          const entryPath = prefix ? `${prefix}/${entry.path}` : entry.path;
          if (entry.type === 'tree') await visit(entry.oid, entryPath, depth + 1);
          else if (entry.type === 'blob' && ['100644', '100755', '120000'].includes(entry.mode)) {
            pathBytes += Buffer.byteLength(entryPath);
            if (pathBytes > 16 * 1024 * 1024) throw new Error('HISTORY_OBJECT_LIMIT: expanded tree paths exceed byte limit');
            result.push({ path: entryPath, oid: entry.oid, mode: entry.mode as HistoryFileMode });
            if (result.length > 100_000) throw new Error('HISTORY_OBJECT_LIMIT: tree traversal exceeds 100000 entries');
          }
        }
      };
      await visit(oid, '');
      return result.sort((a, b) => a.path.localeCompare(b.path));
    },
    async writeCommit(input) {
      assertWritable();
      await assertMetadataSafe();
      validateOid(input.tree);
      for (const parent of input.parents ?? []) validateOid(parent);
      const timestamp = Math.floor((input.timestampMs ?? Date.now()) / 1000);
      const person = { name: 'Octocode Awareness', email: 'awareness@localhost', timestamp, timezoneOffset: 0 };
      const oid = await git.writeCommit({ fs: privateFs, dir: rootDir, gitdir, commit: { message: input.message, tree: input.tree, parent: input.parents ?? [], author: person, committer: person } });
      await privateObject(oid);
      return oid;
    },
    async readCommit(oid, signal) {
      signal?.throwIfAborted();
      await assertMetadataSafe();
      validateOid(oid);
      return { oid, ...parseHistoryCommit(await readHistoryObject(canonicalGitdir, oid, MAX_HISTORY_OBJECT_BYTES, true, signal)) };
    },
    async verifyObject(oid, expectedType) {
      try {
        await assertMetadataSafe();
        validateOid(oid);
        const object = await readHistoryObject(canonicalGitdir, oid, MAX_HISTORY_OBJECT_BYTES, false);
        return expectedType ? object.objectType === expectedType : true;
      } catch {
        return false;
      }
    },
    async inspectOrphanObjects({ retainedOids, graceMs, limit, cursor }) {
      return inspectOrphanObjects({ gitdir: canonicalGitdir, retainedOids, graceMs, limit, cursor, assertMetadataSafe });
    },
    async reclaimOrphanObjects({ retainedOids, graceMs, limit, cursor }) {
      assertWritable();
      return reclaimOrphanObjects({ gitdir: canonicalGitdir, retainedOids, graceMs, limit, cursor, assertMetadataSafe });
    },
    async publishRef(ref, oid) {
      assertWritable();
      if (!HISTORY_REF.test(ref)) throw new Error(`invalid history ref: ${JSON.stringify(ref)}`);
      validateOid(oid);
      await assertMetadataSafe(ref);
      if (await resolveHistoryRef(ref)) throw new Error(`history ref already exists: ${ref}`);
      const refPath = resolve(gitdir, ...ref.split('/'));
      try {
        await writePrivateFile(refPath, `${oid}\n`, true);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === 'EEXIST' || (error instanceof Error && /PRECONDITION_FAILED/.test(error.message))) throw new Error(`history ref already exists or changed: ${ref}`);
        throw error;
      }
    },
    flush,
    resolveRef: resolveHistoryRef,
  };
}
