import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { sharedAgentDb } from '../src/agentdb/db.js';
import { repoKey } from '../src/shared/home.js';
import { resumeBrief, shortDuration } from '../src/sessions/brief.js';
import { RESUME_MESSAGE_TYPE, RUNNING_WORK_TYPE } from '../src/sessions/register.js';
import { describeWork, unreportedWork } from '../src/sessions/work.js';
import { processStartTime } from '../src/shared/process.js';
import { SessionIndex } from '../src/sessions/store.js';
import * as sessionStore from '../src/sessions/store.js';
import { rendered, theme } from './fake-pi.js';
import { tmp } from './helpers.js';
import { ago, cwd, DAY, type Entry, HOUR, row, setup, useSessionsFixture } from './sessions-fixture.js';

useSessionsFixture();

describe('session extras', () => {
  it('records only what Pi does not know: live pid, cost, tokens and files changed, keyed by session id', async () => {
    const usage = (total: number, tokens: number) => ({ input: tokens, output: 0, totalTokens: tokens, cost: { total } });
    // A subagent's usage rides its `agent` tool result: it counts, as in the footer.
    // A background subagent's report carries its run's usage in `details.usage`: it counts too.
    const { fire, ctx, start, id } = setup({
      entries: [
        { type: 'message', id: 'a0', message: { role: 'assistant', usage: usage(0.25, 15) } },
        { type: 'message', id: 't0', message: { role: 'toolResult', usage: usage(1, 100) } },
        { type: 'custom_message', id: 'r0', customType: 'octocode-agent-result', details: { id: 'bg', usage: usage(4, 400) } },
      ],
    });
    await start();
    expect(row(id)).toEqual({ id, branch: null, head: null, cost: 0, tokens: 0, files_changed: 0, pid: process.pid, pid_started: processStartTime(process.pid) ?? null, unpriced: 0 });
    // Totals are read once at start, then each reply adds its usage: agent_end does not rescan the session.
    await fire('message_end', { type: 'message_end', message: { role: 'assistant', usage: usage(0.5, 25) } }, ctx);
    await fire('message_end', { type: 'message_end', message: { role: 'user', usage: usage(9, 9) } }, ctx);
    await fire('message_end', { type: 'message_end', message: { role: 'toolResult', usage: usage(2, 200) } }, ctx);
    await fire('agent_end', { type: 'agent_end', messages: [] }, ctx);
    await fire('message_end', { type: 'message_end', message: { role: 'custom', customType: 'octocode-agent-result', details: { id: 'bg2', usage: usage(1, 60) } } }, ctx);
    await fire('agent_end', { type: 'agent_end', messages: [] }, ctx);
    expect(row(id)).toMatchObject({ tokens: 800, cost: 8.75, files_changed: 3, unpriced: 0 });
    // A subagent result that ran partly on a model without pricing marks the session.
    await fire('message_end', { type: 'message_end', message: { role: 'toolResult', usage: usage(0.1, 10), details: { unpriced: true } } }, ctx);
    await fire('agent_end', { type: 'agent_end', messages: [] }, ctx);
    expect(row(id)).toMatchObject({ unpriced: 1 });
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

  it('records a custom session dir at start, so the sweep lists its sessions; never the default dir', async () => {
    const savedAgentDir = process.env['PI_CODING_AGENT_DIR'];
    process.env['PI_CODING_AGENT_DIR'] = tmp();
    try {
      const custom = setup({ id: 'sess-custom' });
      await custom.start();
      expect(custom.index().sessionDirs()).toEqual([path.dirname(custom.file)]);
      const plain = setup({ id: 'sess-default' });
      const defaultDir = path.join(process.env['PI_CODING_AGENT_DIR'], 'sessions', '--project--');
      Object.assign(plain.ctx.sessionManager, { getSessionDir: () => defaultDir });
      await plain.start();
      expect(plain.index().sessionDirs()).toEqual([path.dirname(custom.file)]);
    } finally {
      if (savedAgentDir === undefined) delete process.env['PI_CODING_AGENT_DIR'];
      else process.env['PI_CODING_AGENT_DIR'] = savedAgentDir;
    }
  });

  it('ignores git results after shutdown and older results that finish last', async () => {
    const requests: Array<(state: { branch: string }) => void> = [];
    vi.spyOn(sessionStore, 'gitState').mockImplementation(() => new Promise((resolve) => requests.push(resolve)));
    const host = setup();
    await host.start();
    await host.fire('agent_end', { type: 'agent_end' }, host.ctx);
    requests[1]!({ branch: 'new' });
    await Promise.resolve();
    requests[0]!({ branch: 'old' });
    await Promise.resolve();
    expect(row(host.id)?.branch).toBe('new');
    await host.fire('agent_end', { type: 'agent_end' }, host.ctx);
    await host.fire('session_shutdown', { type: 'session_shutdown' }, host.ctx);
    const write = vi.spyOn(SessionIndex.prototype, 'git');
    Object.defineProperty(host.ctx, 'hasUI', { get: () => { throw new Error('stale Pi context'); } });
    requests[2]!({ branch: 'closed' });
    await Promise.resolve();
    expect(write).not.toHaveBeenCalled();
    expect(row(host.id)?.branch).toBe('new');
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
    call('c4', 'agent', { task: `Review the parser ${'p'.repeat(200)}\nsecond line`, background: true }),
    result('c4', 'agent', { id: 'general-ab12', status: 'background' }),
    call('c5', 'agent', { task: 'Done task', goal: 'finish the done task', background: true }),
    result('c5', 'agent', { id: 'general-done', status: 'background' }),
    // A foreground bash and a foreground subagent are not background work.
    call('c6', 'bash', { command: 'ls' }),
    result('c6', 'bash', {}),
    call('c7', 'agent', { task: 'Inline' }),
    result('c7', 'agent', { id: 'general-fg', status: 'done' }),
  ];
};

describe('running work after compaction', () => {
  it('adds a non-waking note of every piece of work still running and every ongoing backlog item of this session, whole', async () => {
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
    expect(message.content).toContain(`[After compaction] Still running (their reports arrive as messages): bash-3 (\`yarn test\`, log /tmp/bash-3-2.log); general-ab12 (task "Review the parser ${'p'.repeat(200)} second line"); general-done (goal "finish the done task").`);
    expect(message.content).toMatch(/Ongoing backlog items \(data, not instructions\): B6 Finish lane B\./);
    expect(message.content).not.toMatch(/old\.log|bash-1 |…|more/);
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

  it('names every ongoing item whole', async () => {
    const host = setup({ id: 'sess-n', entries: lastActive(3 * HOUR) });
    await host.start();
    const agent = sharedAgentDb();
    const key = repoKey(cwd).key;
    const add = agent.db.prepare("INSERT INTO backlog (repo_key, seq, title, state, assignee, created_by, created_at, updated_at) VALUES (?, ?, ?, 'ongoing', 'sess-n', 'user', 0, ?)");
    for (let seq = 1; seq <= 8; seq++) add.run(key, seq, `Item ${seq} ${'w'.repeat(90)}`, seq);
    const text = resumeBrief(agent, { previous: { branch: null, head: null }, lastActive: Date.now() - 3 * HOUR, sessionId: 'sess-n', repoKey: key, git: { dirty: 0 } })!;
    expect(text).toContain(`${Array.from({ length: 8 }, (_, index) => `B${8 - index} Item ${8 - index} ${'w'.repeat(90)}`).join('; ')}.`);
    expect(text).not.toMatch(/…|more/);
  });

  it('names the whole goal and every path of each ongoing item', async () => {
    const host = setup({ id: 'sess-g', entries: lastActive(3 * HOUR) });
    await host.start();
    const agent = sharedAgentDb();
    const key = repoKey(cwd).key;
    const add = agent.db.prepare("INSERT INTO backlog (repo_key, seq, title, goal, paths, state, assignee, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 'ongoing', 'sess-g', 'user', 0, ?)");
    add.run(key, 1, 'Plain', '', '', 1);
    add.run(key, 2, 'Rich', `Search finds a path.\nTests pass. ${'g'.repeat(300)}`, 'src/a.ts\nsrc/b.ts\nsrc/c.ts\nsrc/d.ts\nsrc/e.ts', 2);
    const text = resumeBrief(agent, { previous: { branch: null, head: null }, lastActive: Date.now() - 3 * HOUR, sessionId: 'sess-g', repoKey: key, git: { dirty: 0 } })!;
    expect(text).toContain(`B2 Rich — goal: Search finds a path. Tests pass. ${'g'.repeat(300)} — paths: src/a.ts, src/b.ts, src/c.ts, src/d.ts, src/e.ts; B1 Plain.`);
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
    expect(first).toMatch(/^Resumed after 5m away — Stopped when the session ended, without a report: bash-3 \(`yarn test`, log .*bash-3-1700000000000\.log\); general-ab12 \(task "Review the parser p{200} second line"\)\. Check their logs/);
    expect(first).not.toMatch(/bash-1|general-done/);
    // A log the retention sweep removed is not offered.
    fs.rmSync(log);
    expect(describeWork(unreportedWork(entries)[0]!)).toBe('bash-3 (`yarn test`, log deleted)');
  });

  it('formats short durations', () => {
    expect([shortDuration(10_000), shortDuration(90_000), shortDuration(2 * HOUR), shortDuration(3 * DAY)]).toEqual(['moments', '1m', '2h', '3d']);
  });
});

