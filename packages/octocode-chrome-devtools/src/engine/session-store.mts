// Durable research sessions under <workspace>/.octocode/chromedevtools/<session>/.
// One owner for path construction, manifest versions, atomic replacement and the
// session writer lease. Not yet the default layout: producers move here in S2.
import {
  closeSync,
  existsSync,
  fsyncSync,
  mkdirSync,
  openSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  writeSync,
} from 'node:fs';
import { randomBytes } from 'node:crypto';
import { hostname } from 'node:os';
import { join, resolve } from 'node:path';
import { chromeSessionStore } from './chrome-contract.mjs';

export const SESSION_STORE_VERSION = chromeSessionStore.version;
const ID = new RegExp(chromeSessionStore.idPattern);
type IdKind = 's' | 'p' | 'd' | 'c' | 'r';

/** An opaque identifier that is always one safe path segment. */
export function newId(kind: IdKind): string {
  return `${kind}-${randomBytes(8).toString('hex')}`;
}

/** Rejects anything but an identifier this store generated for `kind`. */
export function assertId(id: unknown, kind: IdKind): string {
  if (typeof id !== 'string' || !ID.test(id) || id[0] !== kind)
    throw Error(`invalid ${kind} identifier: ${JSON.stringify(id)}`);
  return id;
}

export function sessionsRoot(workspace: string): string {
  return join(resolve(workspace), chromeSessionStore.root);
}

export function sessionDir(workspace: string, session: string): string {
  return join(sessionsRoot(workspace), assertId(session, 's'));
}

/** Writes `bytes` beside `path`, syncs it, then renames it over `path`. */
export function writeAtomic(path: string, bytes: string | Uint8Array): void {
  const staged = `${path}.tmp-${randomBytes(6).toString('hex')}`;
  const fd = openSync(staged, 'wx', 0o600);
  try {
    writeSync(fd, typeof bytes === 'string' ? Buffer.from(bytes) : bytes);
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
  try {
    renameSync(staged, path);
  } catch (error) {
    rmSync(staged, { force: true });
    throw error;
  }
}

/** A parsed JSON file, still to be checked by its reader. */
export function readJson(path: string): unknown {
  const value: unknown = JSON.parse(readFileSync(path, 'utf8'));
  return value;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

const isString = (value: unknown): value is string => typeof value === 'string';
const isStrings = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every(isString);

export type SessionManifest = {
  version: number;
  id: string;
  workspace: string;
  browser: { incarnation: string; endpoint: string };
  lifecycle: 'active' | 'closed';
  createdAt: string;
  updatedAt: string;
  pages: string[];
};

/** Starts a session bound to one browser incarnation and endpoint. */
export function createSession(
  workspace: string,
  browser: { incarnation: string; endpoint: string },
  now = new Date()
): SessionManifest {
  const id = newId('s');
  const dir = sessionDir(workspace, id);
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  for (const sub of ['_runs', '_events', '_indexes'])
    mkdirSync(join(dir, sub), { mode: 0o700 });
  const manifest: SessionManifest = {
    version: SESSION_STORE_VERSION,
    id,
    workspace: resolve(workspace),
    browser,
    lifecycle: 'active',
    createdAt: now.toISOString(),
    updatedAt: now.toISOString(),
    pages: [],
  };
  writeAtomic(join(dir, 'session.json'), JSON.stringify(manifest, null, 2));
  return manifest;
}

function isSessionManifest(value: unknown): value is SessionManifest {
  return (
    isRecord(value) &&
    typeof value.version === 'number' &&
    isString(value.id) &&
    isString(value.workspace) &&
    isRecord(value.browser) &&
    isString(value.browser.incarnation) &&
    isString(value.browser.endpoint) &&
    (value.lifecycle === 'active' || value.lifecycle === 'closed') &&
    isString(value.createdAt) &&
    isString(value.updatedAt) &&
    isStrings(value.pages)
  );
}

/** A session's manifest, checked against its location and this version. */
export function openSession(workspace: string, id: string): SessionManifest {
  const manifest = readJson(join(sessionDir(workspace, id), 'session.json'));
  if (!isSessionManifest(manifest))
    throw Error(`session ${id} manifest is malformed`);
  if (manifest.version !== SESSION_STORE_VERSION)
    throw Error(
      `session ${id} uses store version ${manifest.version}; this reader supports ${SESSION_STORE_VERSION}`
    );
  if (manifest.id !== id || manifest.workspace !== resolve(workspace))
    throw Error(`session ${id} manifest does not describe its location`);
  return manifest;
}

export function updateSession(
  workspace: string,
  id: string,
  change: (manifest: SessionManifest) => void,
  now = new Date()
): SessionManifest {
  const manifest = openSession(workspace, id);
  change(manifest);
  manifest.updatedAt = now.toISOString();
  writeAtomic(
    join(sessionDir(workspace, id), 'session.json'),
    JSON.stringify(manifest, null, 2)
  );
  return manifest;
}

export function listSessions(workspace: string): string[] {
  const root = sessionsRoot(workspace);
  if (!existsSync(root)) return [];
  return readdirSync(root)
    .filter(name => ID.test(name) && name.startsWith('s-'))
    .toSorted();
}

export type Lease = {
  token: string;
  pid: number;
  host: string;
  incarnation: string;
  acquiredAt: string;
};

const LEASE = '_lease.json';

function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error instanceof Error && 'code' in error && error.code === 'EPERM';
  }
}

/** The current lease, and whether its holder can no longer be running. */
export function readLease(
  workspace: string,
  session: string
): { lease: Lease; stale: boolean } | null {
  const path = join(sessionDir(workspace, session), LEASE);
  if (!existsSync(path)) return null;
  const lease = readJson(path);
  if (
    !isRecord(lease) ||
    !isString(lease.token) ||
    typeof lease.pid !== 'number' ||
    !isString(lease.host) ||
    !isString(lease.incarnation) ||
    !isString(lease.acquiredAt)
  )
    throw Error(`session ${session} lease is malformed`);
  // Only a holder on this host can be proven gone; a live PID may be reused,
  // so it stays held until its owner releases it or an operator removes it.
  const stale = lease.host === hostname() && !alive(lease.pid);
  return {
    lease: {
      token: lease.token,
      pid: lease.pid,
      host: lease.host,
      incarnation: lease.incarnation,
      acquiredAt: lease.acquiredAt,
    },
    stale,
  };
}

/**
 * Takes the session's single writer lease. A stale lease (its same-host
 * holder is gone) is replaced; a live one refuses with its holder.
 */
export function acquireLease(
  workspace: string,
  session: string,
  incarnation: string,
  now = new Date()
): Lease {
  const path = join(sessionDir(workspace, session), LEASE);
  const lease: Lease = {
    token: randomBytes(16).toString('hex'),
    pid: process.pid,
    host: hostname(),
    incarnation,
    acquiredAt: now.toISOString(),
  };
  const current = readLease(workspace, session);
  if (current && !current.stale)
    throw Error(
      `session ${session} is leased by pid ${current.lease.pid} on ${current.lease.host} since ${current.lease.acquiredAt}`
    );
  if (current) rmSync(path, { force: true });
  let fd: number;
  try {
    fd = openSync(path, 'wx', 0o600);
  } catch {
    throw Error(`session ${session} lease was taken concurrently`);
  }
  try {
    writeSync(fd, Buffer.from(JSON.stringify(lease)));
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
  return lease;
}

/** Releases the lease only when `lease` still holds it. */
export function releaseLease(
  workspace: string,
  session: string,
  lease: Lease
): boolean {
  const current = readLease(workspace, session);
  if (!current || current.lease.token !== lease.token) return false;
  rmSync(join(sessionDir(workspace, session), LEASE), { force: true });
  return true;
}
