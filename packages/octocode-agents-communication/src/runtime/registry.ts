/**
 * Registry of the non-default databases that live processes use, so tools that read only the default database can find them.
 *
 * A process started with `--database <file>` writes one small JSON file, `<pid>.json`, under
 * `<Octocode home>/agents-communication/active-databases/`. One file per process needs no shared lock and no read-modify-write.
 * The format is a contract with the Pi extension, which reads it; docs/DB.md#active-database-registry owns it.
 */
import { getOctocodeHome } from '@octocodeai/config';
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { databasePath } from './config.js';
import { LEASE_TTL_MAX_MS, now } from './timing.js';
import { compact } from './json-text.js';

const VERSION = 1;
const DIRECTORY = 'active-databases';
// How long a dead process's entry stays valid: a lease lives at most 10 minutes past the write that took it, and an
// entry is refreshed at most REFRESH_MS behind the writes of its process.
const REFRESH_MS = 60000;
const GRACE_MS = LEASE_TTL_MAX_MS + REFRESH_MS;
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

function directory(): string {
  return join(getOctocodeHome(), 'agents-communication', DIRECTORY);
}

/**
 * Whole seconds since the epoch at which `pid` started, as a decimal string: `ps` prints it in the C locale and UTC, so
 * every reader in every time zone agrees. Null where it is unknown (Windows, no `ps`, no such process).
 */
function processStarted(pid: number): string | null {
  if (process.platform === 'win32') return null;
  const result = spawnSync('ps', ['-o', 'lstart=', '-p', String(pid)], { encoding: 'utf8', timeout: 2000, stdio: ['ignore', 'pipe', 'ignore'], env: { ...process.env, LC_ALL: 'C', TZ: 'UTC0' } });
  if (result.error || result.stdout === null) return null;
  // Mon Jan  2 15:04:05 2006
  const text = result.stdout.split(/\s+/).filter(Boolean);
  const month = MONTHS.indexOf(text[1] ?? '');
  const clock = (text[3] ?? '').split(':');
  const numbers = [text[4], text[2], ...clock];
  if (month < 0 || clock.length !== 3 || !numbers.every(part => part !== undefined && /^\d+$/.test(part))) return null;
  const [year, day, hour, minute, second] = numbers.map(Number) as [number, number, number, number, number];
  return String(Math.floor(Date.UTC(year, month, day, hour, minute, second) / 1000));
}

function processAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === 'EPERM';
  }
}

/** Whether the entry was written within the lease grace, or its process still runs and is the one that wrote it. */
function entryLive(entry: Record<string, unknown>, at: number): boolean {
  const updated = entry.updatedAt;
  if (Number.isInteger(updated) && at - (updated as number) <= GRACE_MS) return true;
  const { pid, started } = entry;
  if (!(Number.isInteger(pid) && (pid as number) > 0 && processAlive(pid as number))) return false;
  if (!started) return true;
  const actual = processStarted(pid as number);
  return actual === null || actual === started;
}

/** Removes entries that are unreadable or no longer live, and temporary files a crashed writer left; never the caller's own entry. */
function prune(folder: string, at: number): void {
  for (const name of readdirSync(folder)) {
    // A writer renames its temporary file at once; one older than the grace belongs to a process that died mid-write.
    if (name.startsWith('.entry-') && name.endsWith('.tmp')) {
      try {
        if (at - statSync(join(folder, name)).mtimeMs > GRACE_MS) rmSync(join(folder, name), { force: true });
      } catch {
        /* removed by its writer or another pruner */
      }
      continue;
    }
    if (!name.endsWith('.json') || name === `${process.pid}.json`) continue;
    let stale: boolean;
    try {
      const entry: unknown = JSON.parse(readFileSync(join(folder, name), 'utf8'));
      stale = typeof entry !== 'object' || entry === null || Array.isArray(entry) || !entryLive(entry as Record<string, unknown>, at);
    } catch {
      stale = true;
    }
    if (stale) rmSync(join(folder, name), { force: true });
  }
}

/** This process's entry for one database; `touch` keeps it fresh while the process keeps writing. */
export class Registration {
  readonly database: string;
  private readonly pid = process.pid;
  private readonly started = processStarted(process.pid);
  private written = 0;

  constructor(database: string) {
    this.database = database;
    this.write();
  }

  write(): void {
    const at = now();
    const entry = { version: VERSION, database: this.database, pid: this.pid, started: this.started, updatedAt: at };
    try {
      const folder = directory();
      mkdirSync(folder, { mode: 0o700, recursive: true });
      const temp = join(folder, `.entry-${process.pid}-${at}.tmp`);
      try {
        writeFileSync(temp, compact(entry), { encoding: 'utf8', mode: 0o600 });
        renameSync(temp, join(folder, `${this.pid}.json`));
      } catch (error) {
        rmSync(temp, { force: true });
        throw error;
      }
      prune(folder, at);
    } catch {
      // The registry only makes the database visible to other tools; it never blocks the coordination itself.
    }
    this.written = at;
  }

  touch(): void {
    if (now() - this.written >= REFRESH_MS) this.write();
  }
}

/** A Registration when `database` is not the default file (which readers find without it), else null. */
export function register(database: string): Registration | null {
  return resolve(database) === databasePath() ? null : new Registration(database);
}
