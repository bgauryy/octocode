import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { AgentDbError, closeSharedAgentDb, openAgentDb, repoKey, sharedAgentDb } from '../src/agentdb/db.js';
import { redactSecrets, secretProblem } from '../src/shared/sanitize.js';
import { AGENT_APPLICATION_ID, AGENT_SCHEMA_VERSION, ensureSchema } from '../src/agentdb/schema.js';
import { agentDbPath, sessionDir, sessionOutputDir, setCurrentSession } from '../src/shared/home.js';
import { loadSqlite } from '../src/shared/util.js';
import { tmp } from './helpers.js';

const modeOf = (file: string) => fs.statSync(file).mode & 0o777;

afterEach(() => {
  closeSharedAgentDb();
  setCurrentSession(undefined);
});

describe('agent state paths', () => {
  it('keeps the database and session folders under <home>/agent/pi, never in agent/ itself', () => {
    const home = tmp();
    expect(agentDbPath({ OCTOCODE_HOME: home })).toBe(path.join(home, 'agent', 'pi', 'octocode.db'));
    expect(agentDbPath({ OCTOCODE_HOME: home, OCTOCODE_AGENT_DB: 'x/y.db' })).toBe(path.resolve('x/y.db'));
    expect(sessionDir('a/b c:1', { OCTOCODE_HOME: home })).toBe(path.join(home, 'agent', 'pi', 'sessions', 'a_b_c_1'));
    expect(fs.existsSync(path.join(home, 'agent'))).toBe(false);
  });

  it.skipIf(process.platform === 'win32')('creates the current session output folders owner-only', () => {
    const home = tmp();
    const env = { OCTOCODE_HOME: home };
    expect(sessionOutputDir('bash', env)).toBe(path.join(home, 'agent', 'pi', 'sessions', `_pid-${process.pid}`, 'bash'));
    setCurrentSession('s1');
    const dir = sessionOutputDir('output', env);
    expect(dir).toBe(path.join(home, 'agent', 'pi', 'sessions', 's1', 'output'));
    for (const folder of [dir, path.dirname(dir), path.join(home, 'agent', 'pi')]) expect(modeOf(folder)).toBe(0o700);
  });
});

describe('openAgentDb', () => {
  it('creates an owner-only WAL database at schema v1 with every table', () => {
    const file = path.join(tmp(), 'deep', 'octocode.db');
    const agent = openAgentDb(file);
    try {
      expect(agent.file).toBe(file);
      const tables = (agent.db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all() as Array<{ name: string }>).map((row) => row.name);
      expect(tables).toEqual(expect.arrayContaining(['sessions', 'backlog', 'backlog_notes', 'memories', 'meta']));
      expect(agent.db.prepare('PRAGMA user_version').get()).toEqual({ user_version: AGENT_SCHEMA_VERSION });
      expect(agent.db.prepare('PRAGMA application_id').get()).toEqual({ application_id: AGENT_APPLICATION_ID });
      expect(agent.db.prepare('PRAGMA journal_mode').get()).toEqual({ journal_mode: 'wal' });
      if (process.platform !== 'win32') {
        expect(modeOf(file)).toBe(0o600);
        expect(modeOf(path.dirname(file))).toBe(0o700);
      }
    } finally {
      agent.close();
    }
    // Reopening migrates nothing and keeps rows.
    const again = openAgentDb(file);
    again.db.prepare("INSERT INTO meta (key, value) VALUES ('k', 'v')").run();
    again.close();
    const third = openAgentDb(file);
    expect(third.db.prepare("SELECT value FROM meta WHERE key = 'k'").get()).toEqual({ value: 'v' });
    third.close();
  });

  it('indexes memories for full-text search when FTS5 is there, following inserts, updates and deletes', () => {
    const agent = openAgentDb(path.join(tmp(), 'a.db'));
    try {
      if (!agent.fts) return;
      const insert = agent.db.prepare("INSERT INTO memories (scope, title, body, keywords, author, created_at, updated_at) VALUES ('global', ?, ?, ?, 'me', 1, 1)");
      const id = Number(insert.run('Use yarn', 'never npm install', 'package manager').lastInsertRowid);
      const match = (query: string) => (agent.db.prepare('SELECT rowid FROM memories_fts WHERE memories_fts MATCH ?').all(query) as Array<{ rowid: number }>).map((row) => row.rowid);
      expect(match('installing')).toEqual([id]);
      agent.db.prepare('UPDATE memories SET body = ? WHERE id = ?').run('pnpm only', id);
      expect(match('installing')).toEqual([]);
      expect(match('pnpm')).toEqual([id]);
      agent.db.prepare('DELETE FROM memories WHERE id = ?').run(id);
      expect(match('pnpm')).toEqual([]);
    } finally {
      agent.close();
    }
  });

  it('enforces the backlog states and priorities', () => {
    const agent = openAgentDb(path.join(tmp(), 'a.db'));
    try {
      const add = agent.db.prepare("INSERT INTO backlog (repo_key, seq, title, state, priority, created_by, created_at, updated_at) VALUES ('r', ?, 't', ?, ?, 'me', 1, 1)");
      for (const [seq, state] of ['backlog', 'todo', 'ongoing', 'done'].entries()) add.run(seq + 1, state, 2);
      expect(() => add.run(9, 'blocked', 2)).toThrow();
      expect(() => add.run(10, 'todo', 4)).toThrow();
      expect(() => add.run(1, 'todo', 1)).toThrow(/UNIQUE/);
    } finally {
      agent.close();
    }
  });

  it('refuses another app\'s database and a newer schema, touching neither', () => {
    const { DatabaseSync } = loadSqlite();
    const foreign = path.join(tmp(), 'agent.sqlite3');
    const other = new DatabaseSync(foreign);
    other.exec('CREATE TABLE sessions (id TEXT); INSERT INTO sessions VALUES (\'theirs\');');
    other.close();
    expect(() => openAgentDb(foreign)).toThrow(AgentDbError);
    // Not even its journal mode changes: no WAL switch before the ownership check.
    expect(fs.existsSync(`${foreign}-wal`) || fs.existsSync(`${foreign}-shm`)).toBe(false);
    const check = new DatabaseSync(foreign);
    expect(check.prepare('SELECT id FROM sessions').all()).toEqual([{ id: 'theirs' }]);
    expect(check.prepare('PRAGMA journal_mode').get()).toEqual({ journal_mode: 'delete' });
    check.close();

    const newer = path.join(tmp(), 'newer.db');
    openAgentDb(newer).close();
    const bump = new DatabaseSync(newer);
    bump.exec(`PRAGMA user_version=${AGENT_SCHEMA_VERSION + 1}`);
    bump.close();
    expect(() => openAgentDb(newer)).toThrow(/newer Octocode/);
  });

  it('refuses a file that is not SQLite with AgentDbError', () => {
    const junk = path.join(tmp(), 'junk.db');
    fs.writeFileSync(junk, 'not sqlite at all, not even close to a header of one'.repeat(20));
    expect(() => openAgentDb(junk)).toThrow(AgentDbError);
    expect(() => openAgentDb(junk)).toThrow(/not an SQLite database/);
  });

  it.skipIf(process.platform === 'win32')('tightens an existing database file and its default folder to owner-only', () => {
    const home = tmp();
    const dir = path.join(home, 'agent', 'pi');
    fs.mkdirSync(dir, { recursive: true, mode: 0o755 });
    fs.chmodSync(dir, 0o755);
    const file = path.join(dir, 'octocode.db');
    openAgentDb(undefined, { OCTOCODE_HOME: home }).close();
    fs.chmodSync(file, 0o644);
    openAgentDb(undefined, { OCTOCODE_HOME: home }).close();
    expect(fs.statSync(dir).mode & 0o777).toBe(0o700);
    expect(fs.statSync(file).mode & 0o777).toBe(0o600);
  });

  it('runs migrations in order inside the caller transaction and rolls a failed step back', () => {
    const { DatabaseSync } = loadSqlite();
    const db = new DatabaseSync(':memory:');
    const steps = [(d: typeof db) => d.exec('CREATE TABLE a (x)'), (d: typeof db) => d.exec('CREATE TABLE b (y)')];
    db.exec('BEGIN');
    ensureSchema(db, 'mem', steps, 2);
    db.exec('COMMIT');
    expect(db.prepare('PRAGMA user_version').get()).toEqual({ user_version: 2 });
    const broken = [...steps, () => {
      throw new Error('step 3 failed');
    }];
    db.exec('BEGIN');
    expect(() => ensureSchema(db, 'mem', broken, 3)).toThrow('step 3 failed');
    db.exec('ROLLBACK');
    expect(db.prepare('PRAGMA user_version').get()).toEqual({ user_version: 2 });
    db.close();
  });

  it('joins nested transactions and rolls the whole one back on error', () => {
    const agent = openAgentDb(path.join(tmp(), 'a.db'));
    try {
      const put = (key: string) => agent.db.prepare('INSERT INTO meta (key, value) VALUES (?, ?)').run(key, 'v');
      expect(() =>
        agent.transaction(() => {
          put('a');
          agent.transaction(() => put('b'));
          throw new Error('boom');
        }),
      ).toThrow('boom');
      expect(agent.db.prepare('SELECT COUNT(*) AS n FROM meta').get()).toEqual({ n: 0 });
      agent.transaction(() => put('c'));
      expect(agent.db.prepare('SELECT key FROM meta').all()).toEqual([{ key: 'c' }]);
    } finally {
      agent.close();
    }
  });
});

describe('sharedAgentDb', () => {
  it('opens once per file named by the env and remembers a refused file', () => {
    const home = tmp();
    const first = sharedAgentDb({ OCTOCODE_HOME: home });
    expect(first.file).toBe(path.join(home, 'agent', 'pi', 'octocode.db'));
    expect(sharedAgentDb({ OCTOCODE_HOME: home })).toBe(first);
    const moved = path.join(tmp(), 'b.db');
    const second = sharedAgentDb({ OCTOCODE_AGENT_DB: moved });
    expect(second.file).toBe(moved);
    expect(second).not.toBe(first);

    const foreign = path.join(tmp(), 'foreign.db');
    fs.writeFileSync(foreign, 'not sqlite at all, not even close to a header of one'.repeat(20));
    expect(() => sharedAgentDb({ OCTOCODE_AGENT_DB: foreign })).toThrow(AgentDbError);
    fs.rmSync(foreign);
    // Cached: not retried even though the junk is gone.
    expect(() => sharedAgentDb({ OCTOCODE_AGENT_DB: foreign })).toThrow(/not an SQLite database/);
    const { DatabaseSync } = loadSqlite();
    const theirs = path.join(tmp(), 'theirs.db');
    const other = new DatabaseSync(theirs);
    other.exec('CREATE TABLE t (x)');
    other.close();
    expect(() => sharedAgentDb({ OCTOCODE_AGENT_DB: theirs })).toThrow(AgentDbError);
    fs.rmSync(theirs);
    // Refused once: not retried within the process, even though the file is now gone.
    expect(() => sharedAgentDb({ OCTOCODE_AGENT_DB: theirs })).toThrow(AgentDbError);
  });
});

describe('repoKey', () => {
  const git = (cwd: string, ...args: string[]) => execFileSync('git', args, { cwd, stdio: 'ignore' });

  it('keys a repository by its git common dir so worktrees share it, and a plain folder by itself', () => {
    const repo = fs.realpathSync(tmp());
    git(repo, 'init', '-q');
    git(repo, '-c', 'user.email=a@b', '-c', 'user.name=a', 'commit', '-q', '--allow-empty', '-m', 'init');
    const sub = path.join(repo, 'src');
    fs.mkdirSync(sub);
    const main = repoKey(sub);
    expect(main).toEqual({ key: path.join(repo, '.git'), root: repo, name: path.basename(repo) });
    const tree = path.join(fs.realpathSync(tmp()), 'wt');
    git(repo, 'worktree', 'add', '-q', tree);
    const worktree = repoKey(tree);
    expect(worktree.key).toBe(main.key);
    expect(worktree.root).toBe(tree);

    const plain = fs.realpathSync(tmp());
    expect(repoKey(plain)).toEqual({ key: plain, root: plain, name: path.basename(plain) });
  });
});

describe('redactSecrets', () => {
  it('replaces every credential-looking span and leaves the rest', () => {
    expect(redactSecrets(`a ghp_${'a'.repeat(36)} b sk-${'c'.repeat(24)} c`)).toBe('a [redacted] b [redacted] c');
    expect(redactSecrets('yarn test --run')).toBe('yarn test --run');
  });
});

describe('secretProblem', () => {
  it('flags credential-looking text and passes ordinary notes', () => {
    const samples = [
      'AKIAABCDEFGHIJKLMNOP',
      `ghp_${'a'.repeat(36)}`,
      `github_pat_${'a'.repeat(40)}`,
      `sk-${'a'.repeat(32)}`,
      'xoxb-1234567890-abcdef',
      '-----BEGIN RSA PRIVATE KEY-----',
      'eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NSJ9.c2lnbmF0dXJlLXZhbHVl',
    ];
    for (const sample of samples) expect(secretProblem(`token: ${sample}`), sample).toMatch(/never store secrets/);
    expect(secretProblem('Use yarn, not npm; tests live in tests/. The sk- prefix is for OpenAI keys.')).toBeUndefined();
  });

  it('flags Stripe, Google, npm, GitLab, URL passwords, key assignments and bearer tokens', () => {
    const samples = [
      `sk_live_${'a1'.repeat(12)}`,
      `rk_test_${'Z9'.repeat(10)}`,
      `AIza${'b'.repeat(35)}`,
      `npm_${'c'.repeat(36)}`,
      `glpat-${'d'.repeat(20)}`,
      'postgres://admin:hunter22@db.example.com/app',
      'password=Tr0ub4dor&3',
      'API_KEY: "a8f5f167f44f4964e6c998dee827110c"',
      `Authorization: Bearer ${'e'.repeat(24)}`,
    ];
    for (const sample of samples) expect(secretProblem(sample), sample).toMatch(/never store secrets/);
  });

  it('passes ordinary prose and code about credentials', () => {
    const clean = [
      'the token budget is 2000',
      'password field validation',
      'token = getToken(1)',
      'password: process.env.DB_PASSWORD',
      'api_key=${API_KEY}',
      'connect with postgres://user:${PASS}@localhost or https://<user>:<password>@host',
      'send Authorization: Bearer <token> with each request',
      'The secret is kept in 1Password; rotate tokens every 90 days.',
      'token: string; secret?: boolean',
      'Fetch https://example.com/docs:8080 and read the api key section',
    ];
    for (const text of clean) expect(secretProblem(text), text).toBeUndefined();
  });
});
