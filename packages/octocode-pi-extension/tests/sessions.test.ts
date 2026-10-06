import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { SessionManager, type SessionInfo } from '@earendil-works/pi-coding-agent';
import { closeSharedAgentDb, repoKey, sharedAgentDb } from '../src/agentdb/db.js';
import { Subcommands } from '../src/shared/commands.js';
import { sessionDir, sessionOutputDir, setCurrentSession } from '../src/shared/home.js';
import { shortDuration } from '../src/sessions/brief.js';
import { registerSessions, RESUME_MESSAGE_TYPE, RUNNING_WORK_TYPE } from '../src/sessions/register.js';
import { describeWork, unreportedWork, workList } from '../src/sessions/work.js';
import { SessionIndex } from '../src/sessions/store.js';
import { sweepSessions } from '../src/sessions/sweep.js';
import { vi } from 'vitest';
import { fakeCtx, fakePi, rendered, theme } from './fake-pi.js';
import { tmp } from './helpers.js';

const HOUR = 3_600_000;
const DAY = 24 * HOUR;
const DEAD_PID = 9_999_999;

let saved: { home?: string; db?: string };
let home: string;
let cwd: string;

beforeEach(() => {
  saved = { home: process.env['OCTOCODE_HOME'], db: process.env['OCTOCODE_AGENT_DB'] };
  home = tmp();
  cwd = tmp();
  process.env['OCTOCODE_HOME'] = home;
  delete process.env['OCTOCODE_AGENT_DB'];
  // Pi's own session listing would read the real ~/.pi: tests give it per case.
  piSessions = [];
  vi.spyOn(SessionManager, 'list').mockImplementation(async () => piSessions);
  vi.spyOn(SessionManager, 'listAll').mockImplementation(async () => piSessions);
});

let piSessions: SessionInfo[] = [];

afterEach(() => {
  vi.restoreAllMocks();
  closeSharedAgentDb();
  setCurrentSession(undefined);
  for (const [key, value] of [['OCTOCODE_HOME', saved.home], ['OCTOCODE_AGENT_DB', saved.db]] as const) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
});

type Entry = { type: string; id: string; timestamp?: string; message?: Record<string, unknown>; customType?: string; details?: unknown };

/** A Pi host with the sessions feature and a session the test can shape. */
function setup(options: { id?: string; entries?: Entry[]; hasUI?: boolean; name?: string; subagent?: boolean } = {}) {
  const fake = fakePi();
  const names: string[] = [];
  const switched: string[] = [];
  Object.assign(fake.pi, { setSessionName: (name: string) => names.push(name), getSessionName: () => names.at(-1) ?? options.name });
  const commands = new Subcommands();
  registerSessions(fake.pi, { commands, isSubagent: options.subagent ?? false, filesChanged: () => 3 });
  const id = options.id ?? 'sess-1';
  const file = path.join(tmp(), `${id}.jsonl`);
  fs.writeFileSync(file, '{}\n');
  const entries = options.entries ?? [];
  const ctx = fakeCtx({ cwd, hasUI: options.hasUI ?? true });
  Object.assign(ctx, {
    sessionManager: {
      getSessionId: () => id,
      getSessionFile: () => file,
      getSessionDir: () => path.dirname(file),
      getSessionName: () => options.name,
      getEntries: () => entries,
      getBranch: () => entries,
    },
    switchSession: async (target: string) => (switched.push(target), { cancelled: false }),
  });
  const start = async (reason = 'startup') => {
    setCurrentSession(id);
    await fake.fire('session_start', { type: 'session_start', reason }, ctx);
  };
  return { ...fake, piCommands: fake.commands, commands, ctx, id, file, entries, names, switched, start, index: () => new SessionIndex(sharedAgentDb()) };
}

const row = (id: string) => new SessionIndex(sharedAgentDb()).get(id);
const ago = (ms: number) => new Date(Date.now() - ms).toISOString();

describe('session extras', () => {
  it('records only what Pi does not know: live pid, cost, tokens and files changed, keyed by session id', async () => {
    const usage = (total: number, tokens: number) => ({ input: tokens, output: 0, totalTokens: tokens, cost: { total } });
    const { fire, ctx, start, id } = setup({ entries: [{ type: 'message', id: 'a0', message: { role: 'assistant', usage: usage(0.25, 15) } }] });
    await start();
    expect(row(id)).toEqual({ id, branch: null, head: null, cost: 0, tokens: 0, files_changed: 0, pid: process.pid });
    // Totals are read once at start, then each reply adds its usage: agent_end does not rescan the session.
    await fire('message_end', { type: 'message_end', message: { role: 'assistant', usage: usage(0.5, 25) } }, ctx);
    await fire('message_end', { type: 'message_end', message: { role: 'user', usage: usage(9, 9) } }, ctx);
    await fire('agent_end', { type: 'agent_end', messages: [] }, ctx);
    expect(row(id)).toMatchObject({ tokens: 40, cost: 0.75, files_changed: 3 });
    await fire('session_shutdown', { type: 'session_shutdown', reason: 'quit' }, ctx);
    expect(row(id)?.pid).toBeNull();
    // A fresh start says nothing.
    expect(await fire('before_agent_start', { type: 'before_agent_start', prompt: 'x' }, ctx)).toBeUndefined();
  });

  it('never names the session: Pi owns names', async () => {
    const { fire, ctx, start, names, entries } = setup();
    await start();
    entries.push({ type: 'message', id: 'u1', message: { role: 'user' } });
    await fire('agent_end', { type: 'agent_end', messages: [] }, ctx);
    expect(names).toEqual([]);
    expect(fire).toBeDefined();
  });

  it('records the git branch and HEAD without blocking the event', async () => {
    const run = (...args: string[]) => execFileSync('git', args, { cwd, stdio: 'pipe', encoding: 'utf8' });
    run('init', '-q', '-b', 'main');
    run('-c', 'user.email=t@t', '-c', 'user.name=t', 'commit', '-q', '--allow-empty', '-m', 'one');
    const host = setup({ id: 'sess-git' });
    await host.start();
    await vi.waitFor(() => expect(row('sess-git')).toMatchObject({ branch: 'main', head: run('rev-parse', 'HEAD').trim() }));
    run('checkout', '-q', '-b', 'feature');
    await host.fire('agent_end', { type: 'agent_end', messages: [] }, host.ctx);
    await vi.waitFor(() => expect(row('sess-git')?.branch).toBe('feature'));
  });

  it('keeps no rows for a subagent and registers only the after-compaction running-work note', async () => {
    const { handlers, commands } = setup({ subagent: true });
    expect([...handlers.keys()].sort()).toEqual(['session_compact', 'session_start']);
    expect(commands.get('sessions')).toBeUndefined();
  });

  it('warns once and keeps the session going when the agent database is not ours', async () => {
    const foreign = path.join(tmp(), 'foreign.db');
    const { DatabaseSync } = await import('node:sqlite');
    const db = new DatabaseSync(foreign);
    db.exec('CREATE TABLE other (x)');
    db.close();
    process.env['OCTOCODE_AGENT_DB'] = foreign;
    const { start, fire, ctx } = setup();
    await start();
    await fire('agent_end', { type: 'agent_end', messages: [] }, ctx);
    expect(ctx.ui.notes.filter((note) => note.type === 'warning')).toHaveLength(1);
    expect(ctx.ui.notes[0]!.message).toMatch(/not an Octocode agent database/);
  });
});

/** A background bash job (bash-1 and bash-3) and background subagents (general-done, general-ab12), all started. */
const backgroundWork = (log: string, timestamp: string): Entry[] => {
  const call = (id: string, name: string, args: Record<string, unknown>): Entry => ({ type: 'message', id: `a-${id}`, timestamp, message: { role: 'assistant', content: [{ type: 'toolCall', id, name, arguments: args }] } });
  const result = (id: string, toolName: string, details: Record<string, unknown>): Entry => ({ type: 'message', id: `r-${id}`, timestamp, message: { role: 'toolResult', toolCallId: id, toolName, content: [], details } });
  return [
    call('c1', 'bash', { command: 'yarn build', background: true }),
    result('c1', 'bash', { job: 'bash-1', pid: 1, log: '/tmp/bash-1-1.log' }),
    call('c3', 'bash', { command: 'yarn test', background: true }),
    result('c3', 'bash', { job: 'bash-3', pid: 3, log }),
    call('c4', 'agent', { task: 'Review the parser', background: true }),
    result('c4', 'agent', { id: 'general-ab12', status: 'background' }),
    call('c5', 'agent', { task: 'Done task', background: true }),
    result('c5', 'agent', { id: 'general-done', status: 'background' }),
    // A foreground bash and a foreground subagent are not background work.
    call('c6', 'bash', { command: 'ls' }),
    result('c6', 'bash', {}),
    call('c7', 'agent', { task: 'Inline' }),
    result('c7', 'agent', { id: 'general-fg', status: 'done' }),
  ];
};

describe('running work after compaction', () => {
  it('adds a short non-waking note of work still running and this session\'s ongoing backlog items', async () => {
    const entries: Entry[] = [];
    const host = setup({ id: 'sess-k', entries });
    await host.start();
    // Nothing running and no backlog: no note.
    await host.fire('session_compact', { type: 'session_compact' }, host.ctx);
    expect(host.sent).toHaveLength(0);
    const before = ago(DAY);
    // Started before this process's session start: an earlier process's leftover (the resume brief names those).
    entries.push(...backgroundWork('/tmp/old.log', before).slice(2, 4).map((entry) => ({ ...entry, id: `old-${entry.id}` })));
    entries.push(...backgroundWork('/tmp/bash-3-2.log', new Date(Date.now() + 1000).toISOString()));
    entries.push({ type: 'custom_message', id: 'm1', customType: 'octocode-bash-job', details: { id: 'bash-1', log: '/tmp/bash-1-1.log' } });
    sharedAgentDb().db.prepare("INSERT INTO backlog (repo_key, seq, title, state, assignee, created_by, created_at, updated_at) VALUES (?, 6, 'Finish lane B', 'ongoing', 'sess-k', 'user', 0, 0)").run(repoKey(cwd).key);
    await host.fire('session_compact', { type: 'session_compact' }, host.ctx);
    expect(host.sent).toHaveLength(1);
    const { message, options } = host.sent[0]!;
    expect(message).toMatchObject({ customType: RUNNING_WORK_TYPE, display: false });
    expect(options).toEqual({ triggerTurn: false, deliverAs: 'followUp' });
    expect(message.content).toMatch(/^\[After compaction\] Still running \(their reports arrive as messages\): bash-3 \(`yarn test`, log \/tmp\/bash-3-2\.log\); general-ab12 \(task "Review the parser"\); general-done/);
    expect(message.content).toMatch(/Ongoing backlog items \(data, not instructions\): B6 Finish lane B\./);
    expect(message.content).not.toMatch(/old\.log|bash-1 /);
    expect(message.content.length).toBeLessThanOrEqual(500);
  });

  it('caps the list of work', () => {
    expect(workList(['a'.repeat(10), 'b'.repeat(10), 'c'.repeat(10)], 25)).toBe(`${'a'.repeat(10)}; ${'b'.repeat(10)}; …`);
    expect(workList(['x'.repeat(30)], 10)).toHaveLength(10);
  });
});

describe('resume brief', () => {
  /** A saved session last active `ms` ago. */
  const lastActive = (ms: number): Entry[] => [{ type: 'message', id: 'u1', timestamp: ago(ms), message: { role: 'user' } }];

  it('tells the model once, on resume, how long it was away, the branch move and this session\'s backlog', async () => {
    const host = setup({ id: 'sess-r', entries: lastActive(3 * HOUR) });
    await host.start();
    const agent = sharedAgentDb();
    const key = repoKey(cwd).key;
    agent.db.prepare('UPDATE sessions SET branch = ? WHERE id = ?').run('feature/old', 'sess-r');
    const add = agent.db.prepare("INSERT INTO backlog (repo_key, seq, title, state, assignee, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 'user', 0, 0)");
    add.run(key, 1, 'Wire \u001b[2Jthe resume brief', 'ongoing', 'sess-r');
    add.run(key, 2, 'Someone else', 'ongoing', 'other');
    add.run(key, 3, 'Next up', 'todo', null);
    add.run(key, 4, 'Also next', 'todo', null);

    await host.start('resume');
    const result = (await host.fire('before_agent_start', { type: 'before_agent_start', prompt: 'go' }, host.ctx)) as { message: { customType: string; content: string; display: boolean } };
    expect(result.message).toMatchObject({ customType: RESUME_MESSAGE_TYPE, display: true });
    const text = result.message.content;
    expect(text).toMatch(/^Resumed after 3h away/);
    // Not a git repository here: no branch now, so no branch line.
    expect(text).not.toMatch(/branch changed/);
    expect(text).toMatch(/B1 Wire the resume brief/);
    expect(text).not.toMatch(/Someone else|\u001b/);
    expect(text).toMatch(/2 backlog items to do/);
    expect(text.length).toBeLessThanOrEqual(1200);
    expect(await host.fire('before_agent_start', { type: 'before_agent_start', prompt: 'again' }, host.ctx)).toBeUndefined();

    const renderer = host.renderers.get(RESUME_MESSAGE_TYPE);
    expect(rendered(renderer({ content: text }, { expanded: false }, theme))).toMatch(/^\s*↻ Resumed after 3h away.* … \+1 line \(ctrl\+o to expand\)$/);
    expect(rendered(renderer({ content: text }, { expanded: true }, theme))).toMatch(/B1 Wire the resume brief/);
  });

  it('also briefs a startup that continues a session with messages (one from before Octocode too), and never a fresh one', async () => {
    const host = setup({ id: 'sess-c', entries: lastActive(2 * DAY) });
    await host.start();
    const result = (await host.fire('before_agent_start', { type: 'before_agent_start', prompt: 'go' }, host.ctx)) as { message: { content: string } };
    expect(result.message.content).toMatch(/^Resumed after 2d away/);
    const fresh = setup({ id: 'sess-f' });
    await fresh.start();
    expect(await fresh.fire('before_agent_start', { type: 'before_agent_start', prompt: 'go' }, fresh.ctx)).toBeUndefined();
  });

  it('stays quiet on a short break with nothing new', async () => {
    const host = setup({ id: 'sess-q', entries: lastActive(20 * 60_000) });
    await host.start('resume');
    expect(await host.fire('before_agent_start', { type: 'before_agent_start', prompt: 'go' }, host.ctx)).toBeUndefined();
  });

  it('stays quiet when only the to-do count and a dirty tree are there (they rarely change between resumes)', async () => {
    const run = (...args: string[]) => execFileSync('git', args, { cwd, stdio: 'pipe', encoding: 'utf8' });
    run('init', '-q', '-b', 'main');
    run('-c', 'user.email=t@t', '-c', 'user.name=t', 'commit', '-q', '--allow-empty', '-m', 'one');
    fs.writeFileSync(path.join(cwd, 'dirty.txt'), 'x');
    const host = setup({ id: 'sess-d', entries: lastActive(20 * 60_000) });
    await host.start();
    await vi.waitFor(() => expect(row('sess-d')?.head).toBeTruthy());
    const { key } = repoKey(cwd);
    sharedAgentDb().db.prepare("INSERT INTO backlog (repo_key, seq, title, body, state, priority, tags, created_by, created_at, updated_at) VALUES (?, 1, 't', '', 'todo', 2, '', 'user', 0, 0)").run(key);
    await host.start('resume');
    expect(await host.fire('before_agent_start', { type: 'before_agent_start', prompt: 'go' }, host.ctx)).toBeUndefined();
  });

  it('reads git when the brief is delivered and leads with the change, even after a short break', async () => {
    const run = (...args: string[]) => execFileSync('git', args, { cwd, stdio: 'pipe', encoding: 'utf8' });
    run('init', '-q', '-b', 'main');
    run('-c', 'user.email=t@t', '-c', 'user.name=t', 'commit', '-q', '--allow-empty', '-m', 'one');
    const host = setup({ id: 'sess-g', entries: lastActive(10 * 60_000) });
    await host.start();
    await vi.waitFor(() => expect(row('sess-g')).toMatchObject({ branch: 'main', head: run('rev-parse', 'HEAD').trim() }));
    await host.start('resume');
    // After session_start, before the first prompt: the brief must still see these.
    run('checkout', '-q', '-b', 'feature');
    run('-c', 'user.email=t@t', '-c', 'user.name=t', 'commit', '-q', '--allow-empty', '-m', 'two');
    fs.writeFileSync(path.join(cwd, 'new.txt'), 'x');
    const result = (await host.fire('before_agent_start', { type: 'before_agent_start', prompt: 'go' }, host.ctx)) as { message: { content: string } };
    const text = result.message.content;
    expect(text.split('\n')[0]).toBe('Resumed after 10m away — Git branch changed: main → feature.');
    expect(text).toMatch(/Git HEAD moved: [0-9a-f]{7} → [0-9a-f]{7} \(1 new commit\)\./);
    expect(text).toMatch(/1 uncommitted change in the working tree\./);
    const collapsed = rendered(host.renderers.get(RESUME_MESSAGE_TYPE)({ content: text }, { expanded: false }, theme));
    expect(collapsed).toMatch(/Git branch changed: main → feature\..* … \+2 lines \(ctrl\+o to expand\)/);
    expect(collapsed).not.toContain('(+2)');
  });

  it('names background work the earlier process never reported, and that alone is worth a brief', async () => {
    const log = path.join(tmp(), 'bash-3-1700000000000.log');
    fs.writeFileSync(log, 'partial\n');
    const entries: Entry[] = [
      ...lastActive(5 * 60_000),
      ...backgroundWork(log, ago(5 * 60_000)),
      // bash-1 and general-done reported back: not listed.
      { type: 'custom_message', id: 'm1', timestamp: ago(5 * 60_000), customType: 'octocode-bash-job', details: { id: 'bash-1', log: '/tmp/bash-1-1.log' } },
      { type: 'custom_message', id: 'm2', timestamp: ago(5 * 60_000), customType: 'octocode-agent-result', details: { id: 'general-done', status: 'done' } },
    ];
    const host = setup({ id: 'sess-w', entries });
    await host.start('resume');
    const result = (await host.fire('before_agent_start', { type: 'before_agent_start', prompt: 'go' }, host.ctx)) as { message: { content: string } };
    const first = result.message.content.split('\n')[0]!;
    expect(first).toMatch(/^Resumed after 5m away — Stopped when the session ended, without a report: bash-3 \(`yarn test`, log .*bash-3-1700000000000\.log\); general-ab12 \(task "Review the parser"\)/);
    expect(first).not.toMatch(/bash-1|general-done/);
    // A log the retention sweep removed is not offered.
    fs.rmSync(log);
    expect(describeWork(unreportedWork(entries)[0]!)).toBe('bash-3 (`yarn test`, log deleted)');
  });

  it('formats short durations', () => {
    expect([shortDuration(10_000), shortDuration(90_000), shortDuration(2 * HOUR), shortDuration(3 * DAY)]).toEqual(['moments', '1m', '2h', '3d']);
  });
});

describe('session sweep', () => {
  const age = (dir: string, ms: number) => {
    const at = new Date(Date.now() - ms);
    for (const file of [dir, ...fs.readdirSync(dir, { recursive: true }).map((name) => path.join(dir, String(name)))]) fs.utimesSync(file, at, at);
  };
  const folder = (name: string, kinds = ['output']) => {
    const dir = path.join(home, 'agent', 'pi', 'sessions', name);
    for (const kind of kinds) fs.mkdirSync(path.join(dir, kind), { recursive: true });
    return dir;
  };

  it('removes data of sessions Pi no longer lists, keeps live and current ones, trims old output, and runs at most every 6 hours', async () => {
    const index = new SessionIndex(sharedAgentDb());
    const insert = (id: string, pid: number | null) => sharedAgentDb().db.prepare('INSERT INTO sessions (id, pid) VALUES (?, ?)').run(id, pid);
    const listed = async () => new Set(['kept-file', 'current']);
    insert('gone-file', null);
    insert('recent-gone-file', null);
    insert('kept-file', null);
    insert('gone-live', process.pid);
    insert('no-folder', null);
    insert('current', DEAD_PID);
    for (const name of ['gone-file', 'kept-file', 'gone-live', 'current']) age(folder(name), 8 * DAY);
    age(folder('recent-gone-file'), DAY);
    age(folder(`_pid-${DEAD_PID}`), 2 * DAY);
    age(folder(`_pid-${process.pid}`), 20 * DAY);
    age(folder('unindexed-old'), 15 * DAY);
    age(folder('unindexed-new'), 3 * DAY);
    const oldLog = path.join(folder('kept-file'), 'output', 'old.txt');
    const newLog = path.join(folder('kept-file'), 'output', 'new.txt');
    fs.writeFileSync(oldLog, 'x');
    fs.writeFileSync(newLog, 'x');
    fs.utimesSync(oldLog, new Date(Date.now() - 31 * DAY), new Date(Date.now() - 31 * DAY));

    const removed = await sweepSessions(index, 'current', { listed });
    expect(removed.sort()).toEqual(['_pid-9999999', 'gone-file', 'unindexed-old']);
    const left = fs.readdirSync(path.join(home, 'agent', 'pi', 'sessions')).sort();
    expect(left).toEqual(['_pid-' + process.pid, 'current', 'gone-live', 'kept-file', 'recent-gone-file', 'unindexed-new'].sort());
    expect(index.get('gone-file')).toBeUndefined();
    expect(index.get('no-folder')).toBeUndefined();
    expect(index.get('current')).toBeDefined();
    expect(index.get('kept-file')).toBeDefined();
    expect(fs.existsSync(oldLog)).toBe(false);
    expect(fs.existsSync(newLog)).toBe(true);

    age(folder('unindexed-new'), 30 * DAY);
    expect(await sweepSessions(index, 'current', { listed })).toEqual([]);
    expect(await sweepSessions(index, 'current', { listed, now: Date.now() + 7 * HOUR })).toEqual(['unindexed-new']);
  });

  it('removes nothing when Pi cannot list its sessions', async () => {
    const index = new SessionIndex(sharedAgentDb());
    age(folder('unindexed-old'), 15 * DAY);
    expect(await sweepSessions(index, 'current', { force: true, listed: async () => Promise.reject(new Error('no')) })).toEqual([]);
    expect(fs.existsSync(path.join(home, 'agent', 'pi', 'sessions', 'unindexed-old'))).toBe(true);
  });

  it('follows OCTOCODE_CLEANUP_DAYS: 0 turns the sweep off, N days sets how long output is kept', async () => {
    const index = new SessionIndex(sharedAgentDb());
    const listed = async () => new Set<string>();
    age(folder('unindexed-old'), 15 * DAY);
    const log = path.join(folder('current'), 'output', 'log.txt');
    fs.writeFileSync(log, 'x');
    fs.utimesSync(log, new Date(Date.now() - 3 * DAY), new Date(Date.now() - 3 * DAY));
    expect(await sweepSessions(index, 'current', { listed, force: true, env: { ...process.env, OCTOCODE_CLEANUP_DAYS: '0' } })).toEqual([]);
    expect(fs.existsSync(path.join(home, 'agent', 'pi', 'sessions', 'unindexed-old'))).toBe(true);
    await sweepSessions(index, 'current', { listed, force: true, env: { ...process.env, OCTOCODE_CLEANUP_DAYS: '5' } });
    expect(fs.existsSync(log)).toBe(true);
    await sweepSessions(index, 'current', { listed, force: true, env: { ...process.env, OCTOCODE_CLEANUP_DAYS: '2' } });
    expect(fs.existsSync(log)).toBe(false);
  });

  it('writes session output under the current session folder, owner-only', () => {
    setCurrentSession('a/b');
    const dir = sessionOutputDir('bash');
    expect(dir).toBe(path.join(sessionDir('a/b'), 'bash'));
    if (process.platform !== 'win32') for (const each of [dir, sessionDir('a/b'), path.dirname(sessionDir('a/b'))]) expect(fs.statSync(each).mode & 0o777).toBe(0o700);
  });
});

describe('/octocode sessions', () => {
  const info = (id: string, fields: Partial<SessionInfo> = {}): SessionInfo => ({
    path: path.join(cwd, `${id}.jsonl`), id, cwd, created: new Date(Date.now() - DAY), modified: new Date(), messageCount: 2, firstMessage: '', allMessagesText: '', ...fields,
  });

  /** Pi lists `cur` (this session), `other` (two hours old, with extras) and `elsewhere` (another directory). */
  async function withSessions() {
    const current = setup({ id: 'cur', name: 'Current work' });
    await current.start();
    const agent = sharedAgentDb();
    agent.db.prepare("INSERT INTO sessions (id, cost, files_changed, branch) VALUES ('other', 1.5, 2, 'feat/x')").run();
    piSessions = [
      info('cur', { path: current.file, name: 'Current work', firstMessage: 'hello', messageCount: 0 }),
      info('other', { firstMessage: 'Old task', allMessagesText: 'Old task and more', modified: new Date(Date.now() - 2 * HOUR), messageCount: 4 }),
      info('elsewhere', { cwd: '/y/elsewhere', firstMessage: 'Far away', modified: new Date(Date.now() - 3 * HOUR) }),
    ];
    vi.mocked(SessionManager.list).mockImplementation(async (dir: string) => piSessions.filter((each) => each.cwd === dir));
    setCurrentSession('cur');
    return { current, other: piSessions[1]! };
  }

  it('lists Pi\'s sessions of this directory newest first with their extras, all directories with `all`, and filters by a query', async () => {
    const { current } = await withSessions();
    const ctx = Object.assign(current.ctx, { hasUI: false });
    await current.commands.get('sessions')!.handler('', ctx);
    const text = ctx.ui.notes.at(-1)!.message;
    expect(text.split('\n')[1]).toMatch(/^● Current work · moments ago · 0 messages · cur \(current\)$/);
    expect(text).toMatch(/○ Old task · feat\/x · 2h ago · \$1\.50 · 2 files · 4 messages · other/);
    expect(text).not.toMatch(/elsewhere/);
    await current.commands.get('sessions')!.handler('all', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/○ Far away · elsewhere · 3h ago/);
    await current.commands.get('sessions')!.handler('MORE', ctx);
    expect(ctx.ui.notes.at(-1)!.message).not.toMatch(/Current work/);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/Old task/);
    expect(current.commands.get('sessions')!.complete!('a')).toEqual([{ value: 'all', label: 'all' }]);
  });

  it('still lists without the agent database, and says when Pi cannot list', async () => {
    const { current } = await withSessions();
    const foreign = path.join(tmp(), 'foreign.db');
    fs.writeFileSync(foreign, 'not sqlite at all, just text that is long enough to not be a header');
    closeSharedAgentDb();
    process.env['OCTOCODE_AGENT_DB'] = foreign;
    const ctx = Object.assign(current.ctx, { hasUI: false });
    await current.commands.get('sessions')!.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/○ Old task · 2h ago · 4 messages/);
    vi.mocked(SessionManager.list).mockRejectedValueOnce(new Error('disk gone'));
    await current.commands.get('sessions')!.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/Pi could not list its sessions: disk gone/);
  });

  it('resumes, shows, renames and forgets a picked session', async () => {
    const { current, other } = await withSessions();
    const ctx = current.ctx;
    const handler = current.commands.get('sessions')!.handler;
    const pick = async (index: number, action: string) => {
      const lines: string[][] = [];
      Object.assign(ctx.ui, { select: async (_title: string, options: string[]) => (lines.push(options), lines.length === 1 ? options[index] : action) });
      await handler('', ctx);
      return lines[0]!;
    };

    await pick(1, 'Resume');
    expect(current.switched).toEqual([other.path]);
    await pick(0, 'Resume');
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/current session/);

    await pick(1, 'Details');
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/id: other[\s\S]*file: .*other\.jsonl[\s\S]*live: no/);

    // Pi can only rename the session it has open.
    const actionsOf = async (index: number) => {
      const seen: string[][] = [];
      Object.assign(ctx.ui, { select: async (_title: string, options: string[]) => (seen.push(options), seen.length === 1 ? options[index] : undefined) });
      await handler('', ctx);
      return seen[1]!;
    };
    expect(await actionsOf(1)).toEqual(['Resume', 'Details', 'Forget Octocode data']);
    expect(await actionsOf(0)).toContain('Rename');
    ctx.ui.inputs.push('  Now \u001b[1mnamed ');
    await pick(0, 'Rename');
    expect(current.names).toEqual(['Now named']);

    await pick(0, 'Forget Octocode data');
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/cannot be forgotten/);
    fs.mkdirSync(path.join(sessionDir('other'), 'output'), { recursive: true });
    ctx.ui.confirms.push(false);
    await pick(1, 'Forget Octocode data');
    expect(row('other')).toBeDefined();
    ctx.ui.confirms.push(true);
    await pick(1, 'Forget Octocode data');
    expect(row('other')).toBeUndefined();
    expect(fs.existsSync(sessionDir('other'))).toBe(false);
  });

  it('asks before resuming a session another process has open', async () => {
    const { current } = await withSessions();
    const ctx = current.ctx;
    sharedAgentDb().db.prepare("UPDATE sessions SET pid = ? WHERE id = 'other'").run(process.pid);
    let calls = 0;
    Object.assign(ctx.ui, { select: async (_title: string, options: string[]) => (calls++ === 0 ? options[1] : 'Resume') });
    ctx.ui.confirms.push(false);
    await current.commands.get('sessions')!.handler('', ctx);
    expect(current.switched).toEqual([]);
  });

  it('maps identical-looking rows to the row picked, not the first', async () => {
    const { current } = await withSessions();
    const at = new Date(Date.now() - 5 * HOUR);
    piSessions.push(info('one-abcdef', { firstMessage: 'Same', modified: at }), info('two-abcdef', { firstMessage: 'Same', modified: at }));
    const ctx = current.ctx;
    let lines: string[] = [];
    for (const [index, id] of [[2, 'one-abcdef'], [3, 'two-abcdef']] as const) {
      let calls = 0;
      Object.assign(ctx.ui, { select: async (_title: string, options: string[]) => (calls++ === 0 ? ((lines = options), options[index]) : 'Details') });
      await current.commands.get('sessions')!.handler('', ctx);
      expect(ctx.ui.notes.at(-1)!.message).toContain(`id: ${id}`);
    }
    expect(new Set(lines).size).toBe(lines.length);
    expect(lines[3]).toMatch(/ · abcdef #2$/);
  });

  it('titles a fork without a prompt by its parent\'s first message', async () => {
    const { current, other } = await withSessions();
    piSessions.push(info('fork', { parentSessionPath: other.path, modified: new Date(Date.now() - 4 * HOUR) }));
    const ctx = Object.assign(current.ctx, { hasUI: false });
    await current.commands.get('sessions')!.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/↳ Old task · .*\n\s+fork/);
  });

  it('refuses to forget a session another live process has open', async () => {
    const { current } = await withSessions();
    sharedAgentDb().db.prepare("UPDATE sessions SET pid = ? WHERE id = 'other'").run(process.ppid);
    const ctx = current.ctx;
    let calls = 0;
    Object.assign(ctx.ui, { select: async (_title: string, options: string[]) => (calls++ === 0 ? options[1] : 'Forget Octocode data') });
    ctx.ui.confirms.push(true);
    await current.commands.get('sessions')!.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(new RegExp(`open in another Pi process \\(pid ${process.ppid}\\)`));
    expect(row('other')).toBeDefined();
  });

  it('prints the list to stderr without a UI (print and JSON modes, where notify is a no-op)', async () => {
    const { current } = await withSessions();
    const ctx = Object.assign(current.ctx, { hasUI: false });
    const written: string[] = [];
    const write = vi.spyOn(process.stderr, 'write').mockImplementation((chunk) => (written.push(String(chunk)), true));
    try {
      await current.commands.get('sessions')!.handler('all', ctx);
    } finally {
      write.mockRestore();
    }
    expect(written.at(-1)).toMatch(/^Sessions \(all directories\), newest first:\n[\s\S]*elsewhere\n$/);
  });

  it('registers /sessions as a shortcut', () => {
    const { piCommands } = setup();
    expect(piCommands.has('sessions')).toBe(true);
  });
});
