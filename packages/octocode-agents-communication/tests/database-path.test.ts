// SQLite's unix VFS opens at most 504 database-path bytes (512-byte buffer, `-journal` suffix); longer paths get an actionable error.
// Measured on Node 24.15.0 (SQLite 3.51.3) and Node 26.4.0 (SQLite 3.53.2): 504 opens and writes, 505 fails, in rollback-journal and WAL mode.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, symlinkSync } from 'node:fs';
import { join } from 'node:path';
import { binary, jsonCall, spawnSync, tempWorkspace, tempDir } from './helpers.ts';
import { database as runtimeDatabase } from '../dist/runtime.js';

const LIMIT = runtimeDatabase.SQLITE_UNIX_DATABASE_PATH_BYTES;
const bytes = (path: string) => Buffer.byteLength(path);

/** A database path of exactly `total` bytes below `base`: directory names of 100 bytes, then a file name (each component <= 255). */
function pathOf(base: string, total: number): string {
  const parts: string[] = [];
  let used = bytes(base);
  while (total - used - 1 > 200) { parts.push('d'.repeat(100)); used += 101; }
  const file = join(base, ...parts, 'f'.repeat(total - used - 1));
  assert.equal(bytes(file), total);
  return file;
}

const join_ = (workspace: string, database: string) => spawnSync(binary, ['join', JSON.stringify({ name: 'agent', vendor: 'generic' }), '--workspace', workspace, '--database', database], { encoding: 'utf8' });

test('Given a database path at the Unix SQLite limit, when an agent joins, then the database opens and takes writes', { skip: process.platform === 'win32' }, t => {
  const workspace = tempWorkspace(t, 'communication-path-', { real: true });
  const database = pathOf(tempWorkspace(t, 'communication-path-db-', { real: true }), LIMIT);
  const call = jsonCall(binary, workspace, database, { stdio: 'pipe' });
  const id = call('join', { name: 'agent', vendor: 'generic' }).id;
  assert.equal(call('heartbeat', {}, id).expiresAt > Date.now(), true);
  assert.equal(existsSync(database), true);
});

test('Given a database path one byte over the Unix limit, when an agent joins, then the error names the limit and the length and nothing is created', { skip: process.platform === 'win32' }, t => {
  const workspace = tempWorkspace(t, 'communication-path-', { real: true });
  const base = tempWorkspace(t, 'communication-path-db-', { real: true });
  const database = pathOf(base, LIMIT + 1);
  const run = join_(workspace, database);
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, new RegExp(`Database path is ${LIMIT + 1} bytes after resolving symlinks; SQLite opens at most ${LIMIT} bytes\\. Use a shorter --database or --workspace path\\.`));
  assert.doesNotMatch(run.stderr, /unable to open database file/);
  assert.equal(existsSync(join(base, 'd'.repeat(100))), false, 'no directory is created for a path SQLite cannot open');
});

test('Given a short symlink to a long directory, when the resolved path passes the Unix limit, then the resolved length decides', { skip: process.platform === 'win32' }, t => {
  const workspace = tempWorkspace(t, 'communication-path-', { real: true });
  const base = tempWorkspace(t, 'communication-path-db-', { real: true });
  const target = join(base, 'real', 'd'.repeat(100), 'd'.repeat(100), 'd'.repeat(100));
  mkdirSync(target, { recursive: true });
  const link = join(tempDir('communication-path-link-', { real: true }), 'link');
  symlinkSync(target, link);
  const name = (total: number) => 'f'.repeat(total - bytes(target) - 1);
  assert.ok(bytes(join(link, name(LIMIT + 1))) < LIMIT - 100, 'the unresolved path is well below the limit');
  assert.equal(join_(workspace, join(link, name(LIMIT))).status, 0);
  const run = join_(workspace, join(link, name(LIMIT + 1)));
  assert.notEqual(run.status, 0);
  assert.match(run.stderr, new RegExp(`Database path is ${LIMIT + 1} bytes after resolving symlinks`));
});

test('Windows delegates path limits to its VFS rather than using the Unix byte limit', t => {
  const base = tempWorkspace(t, 'communication-vfs-policy-', { real: true });
  const long = pathOf(base, LIMIT + 1);
  assert.doesNotThrow(() => runtimeDatabase.checkPathLength(long, 'win32'));
  for (const platform of ['darwin', 'linux'] as const) assert.throws(() => runtimeDatabase.checkPathLength(long, platform), /Database path/);
});
