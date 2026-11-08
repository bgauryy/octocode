/**
 * Delivery-owner exclusivity for one session. Node has no flock, so the owner listens on a Unix-domain socket
 * (Windows: a named pipe) whose address derives from the database and session. Holding the lock means the listen
 * succeeded; the OS releases it when the owner dies. A leftover socket file whose owner is gone is stale: it is replaced.
 *
 * Liveness is the owner's process, not the socket: the owner writes `{pid, start, ino}` to a sidecar file beside the
 * socket (written atomically right after the listen). A probe is `kill(pid, 0)` plus two reuse guards: the sidecar's
 * socket inode must be the socket's current inode, and the process start time must be unchanged; a zombie (dead,
 * not yet reaped by its parent) counts as dead. Nothing connects, so
 * a frozen (SIGSTOP) owner stays live however often it is probed; a connect probe would fill its accept backlog.
 * The probe holds no state between calls.
 */
import { createHash } from 'node:crypto';
import { lstatSync, mkdirSync, readFileSync, renameSync, rmSync, unlinkSync, writeFileSync } from 'node:fs';
import { createServer, type Server } from 'node:net';
import { basename, dirname, join } from 'node:path';
import { processAlive, processStart } from './process-identity.ts';

const CLAIM_ATTEMPTS = 10;
const CLAIM_WAIT_MS = 25;
/** A socket this young with no sidecar for it belongs to an owner between `listen` and its first write: unproven, so live. */
const SOCKET_GRACE_MS = 1000;
/** Unix socket paths are limited to 103 bytes on macOS (107 on Linux); longer addresses move to a private directory. */
const MAX_SOCKET_PATH = 100;
const WINDOWS = process.platform === 'win32';

export const OWNED_ELSEWHERE = 'Another delivery owner (listen, dispatch or run) already runs for this session; stop it before starting another';

/** The lock file name of a session next to its database. It is the socket address unless the path is too long. */
export function ownerLockPath(database: string, session: string): string {
  return join(dirname(database), `${basename(database)}.owner-${createHash('sha256').update(session).digest('hex').slice(0, 16)}.lock`);
}

function fallbackDirectory(): string {
  const directory = join('/tmp', `octocode-agents-communication-${process.getuid?.() ?? 'user'}`);
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  const info = lstatSync(directory);
  if (!info.isDirectory() || info.uid !== process.getuid?.() || (info.mode & 0o077) !== 0) throw new Error(`Delivery owner socket directory must be a private directory owned by this OS user: ${directory}`);
  return directory;
}

/** The address the owner listens on; `prepare` creates the private directory a long path needs. */
function ownerAddress(database: string, session: string, prepare = false): string {
  const path = ownerLockPath(database, session);
  const digest = createHash('sha256').update(path).digest('hex').slice(0, 32);
  if (WINDOWS) return `\\\\.\\pipe\\octocode-agents-communication-${digest}`;
  if (Buffer.byteLength(path) <= MAX_SOCKET_PATH) return path;
  return join(prepare ? fallbackDirectory() : join('/tmp', `octocode-agents-communication-${process.getuid?.() ?? 'user'}`), `${digest}.sock`);
}

/** Where the owner of `address` records its identity: beside the socket, or beside the database for a Windows pipe. */
function sidecarPath(database: string, session: string, address: string): string {
  return `${WINDOWS ? ownerLockPath(database, session) : address}.pid`;
}

/** What a probe needs to recognise the owner process: its pid, its start time (null when unobtainable) and the socket's inode. */
interface OwnerIdentity {
  pid: number;
  start: string | null;
  ino: string | null;
}

function readIdentity(sidecar: string): OwnerIdentity | null {
  try {
    const value = JSON.parse(readFileSync(sidecar, 'utf8'));
    return Number.isInteger(value?.pid) && typeof value.start !== 'undefined' && typeof value.ino !== 'undefined' ? (value as OwnerIdentity) : null;
  } catch {
    return null;
  }
}

/** Atomic: a probe sees the old file or the whole new one. */
function writeIdentity(sidecar: string, identity: OwnerIdentity): void {
  const temp = `${sidecar}.${process.pid}.tmp`;
  try {
    writeFileSync(temp, JSON.stringify(identity), { encoding: 'utf8', mode: 0o600 });
    renameSync(temp, sidecar);
  } catch (error) {
    rmSync(temp, { force: true });
    throw error;
  }
}

/** Whether the owner recorded in `sidecar` runs and owns the socket at `address`. Pure: no state survives the call. */
function ownerAlive(address: string, sidecar: string): boolean {
  const ino = inode(address);
  // Without a socket nobody owns the address (the OS drops a Windows pipe with its owner, so only POSIX can tell).
  if (!WINDOWS && ino === null) return false;
  const owner = readIdentity(sidecar);
  if (owner === null || owner.ino !== ino) return !WINDOWS && ino !== null && socketAge(address) < SOCKET_GRACE_MS;
  return processAlive(owner.pid, owner.start);
}

/** Whether an owner of this session is alive. Synchronous, so a SQLite function can call it. */
export function deliveryOwnerLive(database: string, session: string): boolean {
  const address = ownerAddress(database, session);
  return ownerAlive(address, sidecarPath(database, session, address));
}

/** Addresses locked by this process. */
const held = new Set<string>();

/** Whether this process holds the delivery-owner lock of `session`: expiry of its row is then no evidence of death. */
export function ownedHere(database: string, session: string): boolean {
  return held.has(ownerAddress(database, session));
}

function inode(address: string): string | null {
  if (WINDOWS) return null;
  try {
    return String(lstatSync(address, { bigint: true }).ino);
  } catch {
    return null;
  }
}

function socketAge(address: string): number {
  try {
    return Date.now() - Number(lstatSync(address, { bigint: true }).ctimeMs);
  } catch {
    return Infinity;
  }
}

function unlinkIf(address: string, identity: string | null): void {
  if (WINDOWS || identity === null || inode(address) !== identity) return;
  try {
    unlinkSync(address);
  } catch {
    /* already gone */
  }
}

/** A listener on the address, or null when the address is taken. */
async function listen(address: string): Promise<Server | null> {
  const server = createServer(connection => connection.destroy());
  return new Promise((resolve, reject) => {
    server.once('error', (error: NodeJS.ErrnoException) => (error.code === 'EADDRINUSE' ? resolve(null) : reject(error)));
    server.listen(address, () => {
      server.removeAllListeners('error');
      // A later server error must not crash the owner; liveness is judged by the owner process (see the file header).
      server.on('error', () => undefined);
      resolve(server);
    });
  });
}

/** Exclusive ownership of delivery for one session until `release()` or process death. */
class OwnerLock implements AsyncDisposable {
  private released = false;
  private readonly identity: string | null;
  private readonly cleanup = (): void => {
    held.delete(this.address);
    unlinkIf(this.address, this.identity);
    // Only remove a sidecar that still names this process: a successor may have replaced it.
    if (readIdentity(this.sidecar)?.pid === process.pid) rmSync(this.sidecar, { force: true });
  };

  /** Records this process as the owner; throws when the sidecar cannot be written (the caller then closes `server`). */
  constructor(private readonly server: Server, readonly address: string, private readonly sidecar: string) {
    this.identity = inode(address);
    writeIdentity(sidecar, { pid: process.pid, start: processStart(process.pid), ino: this.identity });
    server.unref();
    held.add(address);
    // A process.exit() skips server.close(); remove the socket so the next owner need not recover a stale one.
    process.once('exit', this.cleanup);
  }

  async release(): Promise<void> {
    if (this.released) return;
    this.released = true;
    process.removeListener('exit', this.cleanup);
    await new Promise<void>(resolve => this.server.close(() => resolve()));
    this.cleanup();
  }

  [Symbol.asyncDispose](): Promise<void> {
    return this.release();
  }
}

/**
 * Takes delivery ownership: listen succeeds, or a live owner holds it (a few short retries cover a hand-over) and
 * this throws. A socket nobody listens on is stale and is unlinked once its identity is re-checked.
 */
export async function claimDeliveryOwner(database: string, session: string): Promise<OwnerLock> {
  const address = ownerAddress(database, session, true);
  const sidecar = sidecarPath(database, session, address);
  for (let attempt = 0; attempt < CLAIM_ATTEMPTS; attempt += 1) {
    const server = await listen(address);
    if (server) {
      try {
        return new OwnerLock(server, address, sidecar);
      } catch (error) {
        server.close();
        unlinkIf(address, inode(address));
        throw error;
      }
    }
    const seen = inode(address);
    if (ownerAlive(address, sidecar)) await new Promise(resolve => setTimeout(resolve, CLAIM_WAIT_MS));
    else unlinkIf(address, seen);
  }
  throw new Error(OWNED_ELSEWHERE);
}
