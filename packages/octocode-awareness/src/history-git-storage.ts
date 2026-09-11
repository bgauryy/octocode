import fs from 'node:fs';
import { relative, resolve, sep } from 'node:path';
import type { FileDurability } from '@octocodeai/octocode-extension-rust';
import { loadNativeFiles } from './native-files.js';
import { readHistoryObject, MAX_HISTORY_OBJECT_BYTES } from './history-git-object.js';

/** Private isomorphic-git IO: serialization stays in Git, filesystem publication stays native. */
export async function createPrivateHistoryIo(rootDir: string, canonicalStore: string, canonicalGitdir: string, readOnly = false) {
  const native = await loadNativeFiles();
  if (!readOnly) await native.ensurePrivateDirectory(canonicalStore);
  const assertWritable = (): void => {
    if (readOnly) throw new Error('HISTORY_STORE_READ_ONLY: this history handle cannot publish or flush');
  };
  const pendingFlush = new Set<string>();
  const metadataLimit = 17 * 1024 * 1024;
  const ownedPath = (path: string): string => {
    const rel = relative(rootDir, path);
    if (rel === '..' || rel.startsWith(`..${sep}`)) throw new Error('private Git write escaped store');
    return resolve(canonicalStore, rel);
  };
  const stableSnapshot = async (path: string, maximum: number) => {
    for (let attempt = 0; ; attempt++) {
      try { return await native.snapshotFile(path, maximum, true); }
      catch (error) {
        if (attempt >= 3 || native.nativeErrorCode(error) !== native.NativeErrorCodes.PRECONDITION_FAILED) throw error;
      }
    }
  };
  const readPrivateFile = async (path: string, encoding?: string): Promise<Buffer | string> => {
    const relativePath = relative(rootDir, path).split(sep).join('/');
    const maximum = relativePath === 'history-store.json' ? 1024 : relativePath.startsWith('repo.git/refs/') ? 42 : relativePath.startsWith('repo.git/objects/') ? metadataLimit : 64 * 1024;
    const snapshot = await stableSnapshot(ownedPath(path), maximum).catch((error: Error) => { throw new Error(`${error.message} [history metadata read: ${relativePath}]`, { cause: error }); });
    if (!snapshot.exists) throw Object.assign(new Error('private Git file missing'), { code: 'ENOENT' });
    return encoding ? snapshot.content!.toString(encoding as BufferEncoding) : snapshot.content!;
  };
  const writePrivateFile = async (path: string, value: Uint8Array | string, createOnly = false, attempt = 0): Promise<void> => {
    assertWritable();
    const canonical = ownedPath(path);
    const bytes = Buffer.from(value);
    // Complete shared parent creation before taking the file's expected token.
    await native.ensurePrivateDirectory(resolve(canonical, '..'));
    const snapshot = await stableSnapshot(canonical, metadataLimit).catch((error: Error) => { throw new Error(`${error.message} [history metadata preflight: ${relative(rootDir, path)}]`, { cause: error }); });
    const objectPath = relative(canonicalGitdir, canonical).split(sep).join('/').match(/^objects\/([0-9a-f]{2})\/([0-9a-f]{38})$/);
    const existingOid = objectPath ? `${objectPath[1]}${objectPath[2]}` : undefined;
    if (snapshot.exists && existingOid) {
      await readHistoryObject(canonicalGitdir, existingOid, MAX_HISTORY_OBJECT_BYTES, false);
      pendingFlush.add(canonical);
      return;
    }
    if (snapshot.exists && createOnly) throw Object.assign(new Error('private Git file already exists'), { code: 'EEXIST' });
    if (snapshot.exists && snapshot.content!.equals(bytes)) { pendingFlush.add(canonical); return; }
    if (attempt > 0 && snapshot.exists) throw new Error('PRECONDITION_FAILED: differing metadata appeared during missing-target retry');
    let receipt;
    try {
      receipt = await native.replaceFile(canonical, bytes, snapshot.version, metadataLimit, 0o600, undefined, 0o700);
    } catch (error) {
      if (native.nativeErrorCode(error) !== native.NativeErrorCodes.PRECONDITION_FAILED) throw error;
      const detail = error instanceof Error ? error.message : String(error);
      const winner = await stableSnapshot(canonical, metadataLimit);
      if (!snapshot.exists && !winner.exists && attempt < 3) {
        // A peer may create a shared missing ancestor while this distinct target
        // remains absent. Acquire a fresh no-follow token; never reuse the old one.
        return writePrivateFile(path, value, createOnly, attempt + 1);
      }
      if (existingOid) {
        await readHistoryObject(canonicalGitdir, existingOid, MAX_HISTORY_OBJECT_BYTES, false);
      } else {
        // Independent initializers may publish the same HEAD/config. Accept only
        // an identical stable winner, never a changed value or a create-only ref.
        if (createOnly) throw error;
        if (!winner.exists || !winner.content!.equals(bytes)) throw new Error(`${detail} [history metadata publication: ${relative(rootDir, path)}]`, { cause: error });
      }
      pendingFlush.add(canonical);
      return;
    }
    pendingFlush.add(canonical);
    if (!receipt.durable && process.platform !== 'win32') throw new Error(`HISTORY_DURABILITY_FAILED: ${receipt.warnings.join('; ')}`);
  };
  // isomorphic-git owns object serialization/compression. Every publication uses
  // the same descriptor-relative, private-at-creation native writer as restore.
  const privateFs = { promises: {
    ...fs.promises,
    mkdir: async (path: string) => native.ensurePrivateDirectory(ownedPath(path)),
    writeFile: async (path: string, value: Uint8Array | string) => writePrivateFile(path, value),
    readFile: readPrivateFile,
  } } as unknown as typeof fs;
  const privateObject = async (oid: string): Promise<void> => {
    await readHistoryObject(canonicalGitdir, oid, MAX_HISTORY_OBJECT_BYTES, false);
    pendingFlush.add(resolve(canonicalGitdir, 'objects', oid.slice(0, 2), oid.slice(2)));
  };
  const flush = async (): Promise<FileDurability> => {
      assertWritable();
      const paths = [...pendingFlush];
      let durable = process.platform !== 'win32';
      const warnings = new Set<string>();
      for (const path of paths) {
        const result = await native.flushFile(path).catch((error: Error) => { throw new Error(`${error.message} [history metadata flush: ${relative(canonicalStore, path)}]`, { cause: error }); });
        durable &&= result.durable;
        for (const warning of result.warnings) warnings.add(warning);
      }
      for (const path of paths) pendingFlush.delete(path);
      if (!durable && process.platform === 'win32') warnings.add('Private history file data is flushed; Windows cannot guarantee directory-entry persistence.');
      return { durable, warnings: [...warnings] };
  };
  return { privateFs, readPrivateFile, writePrivateFile, privateObject, assertWritable, flush };
}
