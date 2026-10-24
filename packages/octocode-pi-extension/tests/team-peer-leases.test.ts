import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { afterEach, describe, expect, it } from 'vitest';
import { loadSqlite } from '../src/shared/util.js';
import { peerDbPath, peerRegistryDir } from '../src/shared/home.js';
import { PEER_APPLICATION_ID, PEER_SCHEMA_VERSION, PeerLeases } from '../src/team/peer-leases.js';
import { PEER_REGISTRY_GRACE_MS, PEER_REGISTRY_VERSION, PeerRegistry } from '../src/team/peer-registry.js';
import { processStartTime } from '../src/shared/process.js';
import { Team } from '../src/team/session.js';
import { registerCollab } from '../src/team/tools.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { deadPid, tmp } from './helpers.js';

const PACKAGE = path.resolve(import.meta.dirname, '../../octocode-agents-communication');
const SCHEMA = fs.readFileSync(path.join(PACKAGE, 'assets/schema.sql'), 'utf8');
const NOW = 1_800_000_000_000;

interface Lease { path: string; kind?: 'file' | 'tree'; owner?: string; expiresAt?: number; reason?: string; key?: string }

/** A database built by the package's own `schema.sql`, holding sessions `s1` (claude, "cc") and `s2` (pi, bound to Pi session `pi-2`) and the given leases. */
function peerDb(file: string, leases: Lease[], options: { sessionExpiresAt?: number } = {}): void {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const { DatabaseSync } = loadSqlite();
  const db = new DatabaseSync(file);
  db.exec(SCHEMA);
  const session = db.prepare('INSERT INTO sessions (id, workspace, name, vendor, vendorSession, expiresAt) VALUES (?, ?, ?, ?, ?, ?)');
  const alive = options.sessionExpiresAt ?? NOW + 300_000;
  session.run('s1', '/w', 'cc', 'claude', null, alive);
  session.run('s2', '/w', 'pi-peer', 'pi', 'pi-2', alive);
  const insert = db.prepare('INSERT INTO leases (workspace, path, kind, owner, acquiredAt, refreshedAt, expiresAt, reasoning, pathKey) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)');
  for (const lease of leases) {
    // The package keys absolute paths by folded components; unless a test gives the key, the fixtures are ASCII, where folding is lower-casing.
    const key = lease.key ?? lease.path.split('/').filter(Boolean).map((part) => `/${part.toLowerCase()}`).join('');
    insert.run('/w', lease.path, lease.kind ?? 'file', lease.owner ?? 's1', NOW - 5_000, NOW - 5_000, lease.expiresAt ?? NOW + 300_000, lease.reason ?? 'refactor', key);
  }
  db.close();
}

const opened: PeerLeases[] = [];
const peers = (file: string, options: ConstructorParameters<typeof PeerLeases>[1] = {}) => {
  const found = new PeerLeases(file, { now: () => NOW, ...options });
  opened.push(found);
  return found;
};
afterEach(() => {
  for (const found of opened.splice(0)) found.close();
});

const home = () => fs.realpathSync(tmp());

describe('peer leases (agents-communication database, read-only)', () => {
  it('relies on the schema this reads', () => {
    expect(SCHEMA).toContain(`PRAGMA application_id=${PEER_APPLICATION_ID}`);
    expect(SCHEMA).toContain(`PRAGMA user_version=${PEER_SCHEMA_VERSION}`);
  });

  it('is a no-op while the database does not exist, and notices it once it does', () => {
    const file = path.join(home(), 'communication.sqlite');
    const warnings: string[] = [];
    const found = peers(file, { warn: (text) => (warnings.push(text), true) });
    expect(found.conflict('/w/src/a.ts')).toBeUndefined();
    peerDb(file, [{ path: '/w/src/a.ts' }]);
    expect(found.conflict('/w/src/a.ts')).toMatchObject({ name: 'cc', vendor: 'claude', owner: 's1', path: '/w/src/a.ts', reason: 'refactor' });
    expect(warnings).toEqual([]);
  });

  it('refuses a file another session holds, by the shared folding and overlap rules', () => {
    const file = path.join(home(), 'communication.sqlite');
    // `Straße.ts` and `STRASSE.ts` are one file: the package stores the folded key, here its literal value.
    peerDb(file, [{ path: '/w/Src/Straße.ts', key: '/w/src/strasse.ts' }, { path: '/w/docs', kind: 'tree' }, { path: '/w/lib/x.ts', owner: 's2' }]);
    const found = peers(file, { self: () => 'pi-2' });
    expect(found.conflict('/w/SRC/STRASSE.ts')?.owner).toBe('s1');
    expect(found.conflict('/w/docs/deep/readme.md')?.owner).toBe('s1');
    expect(found.conflict('/w/docs')?.owner).toBe('s1');
    // A sibling, a path that only shares a prefix with the tree, and a file the asker's own package session holds.
    expect(found.conflict('/w/src/other.ts')).toBeUndefined();
    expect(found.conflict('/w/documents/a.md')).toBeUndefined();
    expect(found.conflict('/w/lib/x.ts')).toBeUndefined();
    // A file lease does not cover paths beneath its key.
    expect(found.conflict('/w/src/Straße.ts/inner')).toBeUndefined();
  });

  it('ignores a lapsed lease and one whose session has expired', () => {
    const file = path.join(home(), 'communication.sqlite');
    peerDb(file, [{ path: '/w/old.ts', expiresAt: NOW - 1 }]);
    expect(peers(file).conflict('/w/old.ts')).toBeUndefined();
    const gone = path.join(home(), 'gone.sqlite');
    peerDb(gone, [{ path: '/w/old.ts' }], { sessionExpiresAt: NOW - 1 });
    expect(peers(gone).conflict('/w/old.ts')).toBeUndefined();
  });

  it('sees a lease taken after it first looked, and never writes the database', () => {
    const file = path.join(home(), 'communication.sqlite');
    peerDb(file, []);
    const before = fs.readFileSync(file);
    const found = peers(file);
    expect(found.conflict('/w/a.ts')).toBeUndefined();
    expect(fs.readFileSync(file).equals(before)).toBe(true);
    expect(fs.readdirSync(path.dirname(file))).toEqual(['communication.sqlite']);
    const raw = new (loadSqlite().DatabaseSync)(file);
    raw.prepare("INSERT INTO leases (workspace, path, kind, owner, refreshedAt, expiresAt, reasoning, pathKey) VALUES ('/w', '/w/a.ts', 'file', 's1', ?, ?, 'late', '/w/a.ts')").run(NOW, NOW + 1_000);
    raw.close();
    expect(found.conflict('/w/a.ts')?.reason).toBe('late');
  });

  it('skips a database it does not know, with one warning, and tries again later', () => {
    const file = path.join(home(), 'communication.sqlite');
    peerDb(file, [{ path: '/w/a.ts' }]);
    const raw = new (loadSqlite().DatabaseSync)(file);
    raw.exec(`PRAGMA user_version=${PEER_SCHEMA_VERSION + 1}`);
    raw.close();
    const warnings: string[] = [];
    let now = NOW;
    const found = peers(file, { warn: (text) => (warnings.push(text), true), now: () => now });
    expect(found.conflict('/w/a.ts')).toBeUndefined();
    now += 31_000;
    expect(found.conflict('/w/a.ts')).toBeUndefined();
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain(`schema version ${PEER_SCHEMA_VERSION + 1}`);
    expect(warnings[0]).toContain('path leases are not checked');
    // Once the file is the version this reads, the next attempt uses it.
    const fixed = new (loadSqlite().DatabaseSync)(file);
    fixed.exec(`PRAGMA user_version=${PEER_SCHEMA_VERSION}`);
    fixed.close();
    now += 31_000;
    expect(found.conflict('/w/a.ts')?.owner).toBe('s1');
  });

  it('skips a file that is not the package\'s database, and an uninitialized one silently', () => {
    const other = path.join(home(), 'other.sqlite');
    const { DatabaseSync } = loadSqlite();
    const raw = new DatabaseSync(other);
    raw.exec('CREATE TABLE unrelated (x)');
    raw.exec('PRAGMA application_id=1');
    raw.close();
    const warnings: string[] = [];
    expect(peers(other, { warn: (text) => (warnings.push(text), true) }).conflict('/w/a.ts')).toBeUndefined();
    expect(warnings[0]).toContain('application_id 1');
    const fresh = path.join(home(), 'fresh.sqlite');
    new DatabaseSync(fresh).close();
    const quiet: string[] = [];
    expect(peers(fresh, { warn: (text) => (quiet.push(text), true) }).conflict('/w/a.ts')).toBeUndefined();
    expect(quiet).toEqual([]);
    const garbage = path.join(home(), 'garbage.sqlite');
    fs.writeFileSync(garbage, 'this is not a database'.repeat(40));
    const bad: string[] = [];
    expect(peers(garbage, { warn: (text) => (bad.push(text), false) }).conflict('/w/a.ts')).toBeUndefined();
    expect(bad[0]).toContain('could not');
  });

  it('offers its warning again while the session cannot show it', () => {
    const file = path.join(home(), 'communication.sqlite');
    fs.writeFileSync(file, 'x'.repeat(2000));
    let accept = false;
    const warnings: string[] = [];
    let now = NOW;
    const found = peers(file, { warn: (text) => (warnings.push(text), accept), now: () => now });
    found.conflict('/w/a.ts');
    now += 31_000;
    accept = true;
    found.conflict('/w/a.ts');
    now += 31_000;
    found.conflict('/w/a.ts');
    expect(warnings).toHaveLength(2);
  });
});

/** One registry entry as the package writes it (`src/runtime/registry.ts`). */
function entry(dir: string, name: string, fields: Record<string, unknown>): void {
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, `${name}.json`), JSON.stringify({ version: PEER_REGISTRY_VERSION, database: '/db.sqlite', pid: process.pid, started: processStartTime(process.pid) ?? null, updatedAt: NOW, ...fields }));
}
describe('registry of the package\'s non-default databases', () => {
  it('lists the database of an entry whose process runs, or that was written within the lease grace', () => {
    const dir = path.join(home(), 'active');
    entry(dir, 'alive', { database: '/alive.sqlite', updatedAt: NOW - 10 * PEER_REGISTRY_GRACE_MS });
    entry(dir, 'unstarted', { database: '/unstarted.sqlite', started: null, updatedAt: NOW - 10 * PEER_REGISTRY_GRACE_MS });
    entry(dir, 'recent', { database: '/recent.sqlite', pid: deadPid(), updatedAt: NOW - PEER_REGISTRY_GRACE_MS });
    entry(dir, 'same-database', { database: '/recent.sqlite', pid: deadPid() });
    expect(new PeerRegistry(dir).databases(NOW)).toEqual(['/alive.sqlite', '/recent.sqlite', '/unstarted.sqlite']);
  });

  it('drops an entry whose process is gone, or whose pid another process now holds', () => {
    const dir = path.join(home(), 'active');
    const old = NOW - PEER_REGISTRY_GRACE_MS - 1;
    entry(dir, 'dead', { pid: deadPid(), updatedAt: old });
    // The system gave the recorded pid to a process that started at another time.
    if (processStartTime(process.pid) !== undefined) entry(dir, 'reused', { started: '1', updatedAt: old });
    expect(new PeerRegistry(dir).databases(NOW)).toEqual([]);
  });

  it('ignores a missing folder, unreadable files, another version and malformed entries', () => {
    const dir = path.join(home(), 'active');
    expect(new PeerRegistry(dir).databases(NOW)).toEqual([]);
    entry(dir, 'version', { version: PEER_REGISTRY_VERSION + 1, database: '/future.sqlite' });
    entry(dir, 'relative', { database: 'relative.sqlite' });
    entry(dir, 'pid', { database: '/pid.sqlite', pid: 0 });
    entry(dir, 'started', { database: '/started.sqlite', started: 5 });
    entry(dir, 'time', { database: '/time.sqlite', updatedAt: 'now' });
    entry(dir, 'database', { database: 7 });
    fs.writeFileSync(path.join(dir, 'broken.json'), '{not json');
    fs.writeFileSync(path.join(dir, 'null.json'), 'null');
    fs.writeFileSync(path.join(dir, 'huge.json'), JSON.stringify({ version: PEER_REGISTRY_VERSION, database: '/huge.sqlite', pid: process.pid, started: null, updatedAt: NOW, pad: 'x'.repeat(5000) }));
    fs.writeFileSync(path.join(dir, 'notes.txt'), 'not an entry');
    fs.mkdirSync(path.join(dir, 'folder.json'));
    entry(dir, 'good', { database: '/good.sqlite' });
    expect(new PeerRegistry(dir).databases(NOW)).toEqual(['/good.sqlite']);
  });

  it('reads a lease from a registered database, without writing anything, and stops when its entry goes', () => {
    const root = home();
    const dir = path.join(root, 'active'), main = path.join(root, 'communication.sqlite'), custom = path.join(root, 'custom.sqlite');
    peerDb(main, [{ path: '/w/main.ts' }]);
    peerDb(custom, [{ path: '/w/custom.ts', owner: 's1' }]);
    const before = fs.readFileSync(custom);
    const found = peers(main, { registry: dir, self: () => 'pi-2' });
    expect(found.conflict('/w/main.ts')?.owner).toBe('s1');
    expect(found.conflict('/w/custom.ts')).toBeUndefined();
    entry(dir, 'one', { database: custom });
    entry(dir, 'default', { database: main });
    // A database registered after the first check is seen on the next one.
    expect(found.conflict('/w/custom.ts')).toMatchObject({ name: 'cc', vendor: 'claude', path: '/w/custom.ts' });
    expect(found.conflict('/w/main.ts')?.owner).toBe('s1');
    expect(fs.readFileSync(custom).equals(before)).toBe(true);
    fs.rmSync(path.join(dir, 'one.json'));
    expect(found.conflict('/w/custom.ts')).toBeUndefined();
    expect(found.conflict('/w/main.ts')?.owner).toBe('s1');
  });

  it('skips a registered database it cannot read with its own single warning, and still reads the others', () => {
    const root = home();
    const dir = path.join(root, 'active'), main = path.join(root, 'communication.sqlite'), custom = path.join(root, 'custom.sqlite'), broken = path.join(root, 'broken.sqlite');
    peerDb(main, [{ path: '/w/main.ts' }]);
    peerDb(custom, [{ path: '/w/custom.ts' }]);
    fs.writeFileSync(broken, 'x'.repeat(2000));
    entry(dir, 'a-broken', { database: broken });
    entry(dir, 'b-custom', { database: custom });
    entry(dir, 'c-missing', { database: path.join(root, 'missing.sqlite') });
    const warnings: string[] = [];
    let now = NOW;
    const found = peers(main, { registry: dir, now: () => now, warn: (text) => (warnings.push(text), true) });
    expect(found.conflict('/w/custom.ts')?.owner).toBe('s1');
    now += 31_000;
    expect(found.conflict('/w/main.ts')?.owner).toBe('s1');
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toContain(broken);
  });
});

describe('peer leases in the team reservation gate', () => {
  function collaborate(git = false) {
    const cwd = home();
    // The package needs real git metadata; the team only a `.git` entry.
    if (git) spawnSync('git', ['init', '-q', cwd]);
    else fs.mkdirSync(path.join(cwd, '.git'));
    const fake = fakePi();
    const team = new Team(fake.pi, { OCTOCODE_AGENT_DB: path.join(tmp(), 'team.sqlite') });
    const { reservationGate } = registerCollab(fake.pi, team);
    const ctx = fakeCtx({ cwd });
    team.start(ctx);
    return { cwd, team, gate: reservationGate, ctx, fake };
  }
  afterEach(() => fs.rmSync(path.dirname(peerDbPath()), { recursive: true, force: true }));

  it('does nothing without the package database', async () => {
    const { cwd, team, gate, ctx } = collaborate();
    expect(team.reservation(path.join(cwd, 'a.ts'), 'a.ts')).toBeUndefined();
    await expect(gate({ toolName: 'write', input: { path: 'a.ts', content: '' } } as never, ctx)).resolves.toBeUndefined();
  });

  it('shows controls in a peer lease reason and a file name as visible escapes', () => {
    const { cwd, team } = collaborate();
    const target = path.join(cwd, 'src', 'a.ts');
    peerDb(peerDbPath(), [{ path: target, expiresAt: Date.now() + 60_000, reason: 'refactor\u001b[2J' }], { sessionExpiresAt: Date.now() + 60_000 });
    const reason = team.reservation(target, 'src/a\u202e.ts')!;
    expect(reason).toContain('src/a\\u{202e}.ts is reserved by cc (claude agent in agents-communication; refactor\\u{1b}[2J;');
    expect(reason).not.toMatch(/[\u001b\u202e]/u);
  });

  it('refuses a write to a file a package agent holds, naming the holder, until it unlocks', async () => {
    const { cwd, team, gate, ctx } = collaborate();
    const target = path.join(cwd, 'src', 'a.ts');
    // The gate reads the real clock, so the fixture's leases and sessions live relative to it.
    peerDb(peerDbPath(), [{ path: target, expiresAt: Date.now() + 60_000 }], { sessionExpiresAt: Date.now() + 60_000 });
    const reason = team.reservation(target, 'src/a.ts');
    expect(reason).toMatch(/^src\/a\.ts is reserved by cc \(claude agent in agents-communication; refactor; locked /);
    expect(reason).toContain('message cc through agents-communication');
    await expect(gate({ toolName: 'edit', input: { path: 'src/a.ts', edits: [] } } as never, ctx)).resolves.toEqual({ block: true, reason });
    expect(team.reservation(path.join(cwd, 'src', 'b.ts'), 'src/b.ts')).toBeUndefined();
    const free = new (loadSqlite().DatabaseSync)(peerDbPath());
    free.exec('DELETE FROM leases');
    free.close();
    expect(team.reservation(target, 'src/a.ts')).toBeUndefined();
  });

  const entry = path.join(PACKAGE, 'dist/communication.js');

  const runnable = fs.existsSync(entry);

  it.skipIf(!runnable)('sees a package session started with --database <file> through the registry the package writes, until it expires', () => {
    const { cwd, team } = collaborate(true);
    const custom = path.join(tmp(), 'custom.sqlite');
    const run = (command: string, input: object, session?: string) => {
      const args = [entry, command, JSON.stringify(input), '--workspace', cwd, '--database', custom, ...(session ? ['--session', session] : [])];
      const result = spawnSync(process.execPath, args, { encoding: 'utf8', env: { ...process.env } });
      expect(result.status, result.stderr).toBe(0);
      return JSON.parse(result.stdout) as { id?: string; ok?: boolean };
    };
    const target = path.join(cwd, 'src', 'a.ts');
    expect(team.reservation(target, 'src/a.ts')).toBeUndefined();
    const { id } = run('join', { name: 'codex-2', vendor: 'codex' });
    expect(run('lock', { path: 'src/a.ts', reasoning: 'custom database edit' }, id).ok).toBe(true);
    // The package's CLI process is gone already; its entry still counts for the lease grace.
    const [file] = fs.readdirSync(peerRegistryDir());
    const written = JSON.parse(fs.readFileSync(path.join(peerRegistryDir(), file!), 'utf8')) as { version: number; database: string; pid: number; started: string | null; updatedAt: number };
    expect(written).toMatchObject({ version: PEER_REGISTRY_VERSION, database: custom });
    expect(team.reservation(target, 'src/a.ts')).toMatch(/^src\/a\.ts is reserved by codex-2 \(codex agent in agents-communication; custom database edit; locked /);
    expect(team.reservation(path.join(cwd, 'src', 'b.ts'), 'src/b.ts')).toBeUndefined();
    run('leave', {}, id);
    expect(team.reservation(target, 'src/a.ts')).toBeUndefined();
  });

  it.skipIf(!runnable)('agrees with the package itself: a lease taken through its CLI refuses a case and Unicode alias', () => {
    const { cwd, team } = collaborate(true);
    const run = (command: string, input: object, session?: string) => {
      const args = [entry, command, JSON.stringify(input), '--workspace', cwd, '--database', peerDbPath(), ...(session ? ['--session', session] : [])];
      const result = spawnSync(process.execPath, args, { encoding: 'utf8', env: { ...process.env, OCTOCODE_HOME: process.env['OCTOCODE_HOME'] ?? '' } });
      expect(result.status, result.stderr).toBe(0);
      return JSON.parse(result.stdout) as { id?: string; ok?: boolean };
    };
    fs.mkdirSync(path.dirname(peerDbPath()), { recursive: true });
    const { id } = run('join', { name: 'codex-1', vendor: 'codex' });
    expect(run('lock', { path: 'Src/Straße.ts', reasoning: 'rename' }, id).ok).toBe(true);
    expect(team.reservation(path.join(cwd, 'src', 'STRASSE.ts'), 'src/STRASSE.ts')).toMatch(/reserved by codex-1 \(codex agent in agents-communication; rename; locked /);
    expect(team.reservation(path.join(cwd, 'src', 'other.ts'), 'src/other.ts')).toBeUndefined();
    run('leave', {}, id);
    expect(team.reservation(path.join(cwd, 'src', 'STRASSE.ts'), 'src/STRASSE.ts')).toBeUndefined();
  });
});
