/** No-clobber, integrity-checked snapshots of a communication database. */
import { createHash, randomBytes } from 'node:crypto';
import { closeSync, fsyncSync, linkSync, lstatSync, openSync, readSync, statSync, unlinkSync } from 'node:fs';
import { backup, DatabaseSync } from 'node:sqlite';
import { basename, dirname, isAbsolute, join } from 'node:path';
import type { CommandResults } from './catalog-types.generated.js';
import { checkPathLength, openDatabase, query, schema } from './database.js';
import { ValidationError } from './errors.js';
import { realpath } from './paths.js';

/** Whether anything, a symlink included, exists at `path`. */
function exists(path: string): boolean {
  try {
    lstatSync(path);
    return true;
  } catch {
    return false;
  }
}

function sameFile(a: { dev: bigint; ino: bigint }, b: { dev: bigint; ino: bigint }): boolean {
  return a.dev === b.dev && a.ino === b.ino;
}

/** A consistent, integrity-checked, never-overwriting copy of `source` at the absolute path `destination`. */
export async function exportDatabase(source: string, destination: string): Promise<CommandResults['db export']> {
  if (!isAbsolute(destination)) throw new ValidationError('Export path must be absolute');
  const parent = realpath(dirname(destination), true);
  destination = join(parent, basename(destination));
  if (exists(destination)) throw new ValidationError('Export destination already exists; never overwrite a file or symlink');
  const initial = lstatSync(source, { bigint: true });
  if (!initial.isFile()) throw new ValidationError('Export source must be a regular database file, not a symlink');
  source = realpath(source, true);
  const temp = join(parent, '.communication-export-' + randomBytes(6).toString('hex'));
  checkPathLength(temp);
  const db = openDatabase(source, true, false);
  closeSync(openSync(temp, 'wx', 0o600));
  try {
    await backup(db.raw, temp);
    const target = new DatabaseSync(temp);
    try {
      target.exec('PRAGMA journal_mode=DELETE');
      target.exec('VACUUM');
    } finally {
      target.close();
    }
    const current = lstatSync(source, { bigint: true });
    if (!current.isFile() || !sameFile(initial, current)) throw new ValidationError('Source database was replaced during export');
    const snapshot = openDatabase(temp, true, false);
    try {
      const integrity = query(snapshot, 'PRAGMA integrity_check');
      if (integrity.length !== 1 || integrity[0].integrity_check !== 'ok') throw new ValidationError('Export integrity check failed');
      if (query(snapshot, 'PRAGMA foreign_key_check').length) throw new ValidationError('Export foreign key check failed');
    } finally {
      snapshot.close();
    }
    // Windows FlushFileBuffers requires a writable handle; 'r+' never truncates.
    const digest = createHash('sha256');
    const descriptor = openSync(temp, 'r+');
    try {
      fsyncSync(descriptor);
      const block = Buffer.alloc(65536);
      for (let length = readSync(descriptor, block, 0, block.length, null); length > 0; length = readSync(descriptor, block, 0, block.length, null)) digest.update(block.subarray(0, length));
    } finally {
      closeSync(descriptor);
    }
    const size = statSync(temp).size;
    linkSync(temp, destination);
    let synced = false;
    try {
      const directory = openSync(parent, 'r');
      try {
        fsyncSync(directory);
        synced = true;
      } finally {
        closeSync(directory);
      }
    } catch {
      /* directory fsync is unsupported on some platforms */
    }
    return {
      path: destination,
      source,
      schemaVersion: schema().schemaVersion,
      sha256: digest.digest('hex'),
      bytes: size,
      scope: 'all-workspaces',
      includesWorkspaceDocuments: false,
      documents: 'Preserve referenced workspace and Git common-directory document files separately; this snapshot contains their audit metadata only.',
      integrity: 'ok',
      directorySynced: synced,
    };
  } finally {
    db.close();
    for (const suffix of ['', '-wal', '-shm', '-journal']) {
      try {
        unlinkSync(temp + suffix);
      } catch {
        /* absent */
      }
    }
  }
}
