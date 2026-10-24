import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { SessionManager, type SessionInfo } from '@earendil-works/pi-coding-agent';
import { closeSharedAgentDb, sharedAgentDb } from '../src/agentdb/db.js';
import { sessionDir, sessionsRoot, setCurrentSession } from '../src/shared/home.js';
import { tmp } from './helpers.js';
import { cwd, DAY, HOUR, piSessions, row, setPiSessions, setup, useSessionsFixture } from './sessions-fixture.js';

useSessionsFixture();

const info = (id: string, fields: Partial<SessionInfo> = {}): SessionInfo => ({
  path: path.join(cwd, `${id}.jsonl`), id, cwd, created: new Date(Date.now() - DAY), modified: new Date(), messageCount: 2, firstMessage: '', allMessagesText: '', ...fields,
});

/** Pi lists `cur` (this session), `other` (two hours old, with extras) and `elsewhere` (another directory). */
async function withSessions() {
  const current = setup({ id: 'cur', name: 'Current work' });
  await current.start();
  const agent = sharedAgentDb();
  agent.db.prepare("INSERT INTO sessions (id, cost, files_changed, branch, unpriced) VALUES ('other', 1.5, 2, 'feat/x', 1)").run();
  agent.db.prepare("INSERT INTO sessions (id, cost, unpriced) VALUES ('elsewhere', 0, 1)").run();
  setPiSessions([
    info('cur', { path: current.file, name: 'Current work', firstMessage: 'hello', messageCount: 0 }),
    info('other', { firstMessage: 'Old task', allMessagesText: 'Old task and more', modified: new Date(Date.now() - 2 * HOUR), messageCount: 4 }),
    info('elsewhere', { cwd: '/y/elsewhere', firstMessage: 'Far away', modified: new Date(Date.now() - 3 * HOUR) }),
  ]);
  vi.spyOn(SessionManager, 'list').mockImplementation(async (dir: string) => piSessions.filter((each) => each.cwd === dir));
  setCurrentSession('cur');
  return { current, other: piSessions[1] };
}

describe('/octocode sessions', () => {
  it('lists Pi\'s sessions of this directory newest first with their extras, all directories with `all`, and filters by a query', async () => {
    const { current } = await withSessions();
    const ctx = Object.assign(current.ctx, { hasUI: false });
    await current.commands.get('sessions')!.handler('', ctx);
    const text = ctx.ui.notes.at(-1)!.message;
    expect(text.split('\n')[1]).toMatch(/^● Current work · moments ago · 0 messages · cur \(current\)$/);
    expect(text).toMatch(/○ Old task · feat\/x · 2h ago · ≈\$1\.50 \+ n\/a · 2 files · 4 messages · other/);
    expect(text).not.toMatch(/elsewhere/);
    await current.commands.get('sessions')!.handler('all', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/○ Far away · elsewhere · 3h ago · cost n\/a/);
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
    vi.spyOn(SessionManager, 'list').mockRejectedValueOnce(new Error('disk gone'));
    await current.commands.get('sessions')!.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/Pi could not list its sessions: disk gone/);
  });
});

describe('/octocode sessions picker', () => {
  it('resumes, shows, renames and forgets a picked session', async () => {
    const { current, other } = await withSessions();
    const ctx = current.ctx;
    const command = current.commands.get('sessions')!;
    const pick = async (index: number, action: string) => {
      const lines: string[][] = [];
      Object.assign(ctx.ui, { select: async (_title: string, options: string[]) => (lines.push(options), lines.length === 1 ? options[index] : action) });
      await command.handler('', ctx);
      return lines[0];
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
      await command.handler('', ctx);
      return seen[1];
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

  it.each(['..', '.', ''])('refuses to forget a session whose id (%j) would name the sessions folder or its parent', async (bad) => {
    const { current } = await withSessions();
    const ctx = current.ctx;
    piSessions.push(info(bad, { firstMessage: 'Odd id', modified: new Date(Date.now() - HOUR) }));
    const root = sessionsRoot();
    const parent = path.dirname(root);
    const [inside, outside] = [path.join(root, 'keep', 'x.txt'), path.join(parent, 'marker.txt')] as [string, string];
    fs.mkdirSync(path.dirname(inside), { recursive: true });
    fs.writeFileSync(inside, 'x');
    fs.writeFileSync(outside, 'x');
    const lines: string[][] = [];
    Object.assign(ctx.ui, { select: async (_title: string, options: string[]) => (lines.push(options), lines.length === 1 ? options.find((line) => line.includes('Odd id')) : 'Forget Octocode data') });
    ctx.ui.confirms.push(true);
    await current.commands.get('sessions')!.handler('', ctx);
    expect(ctx.ui.notes.at(-1)!.message).toMatch(/Refusing to forget "Odd id"/);
    expect(fs.readFileSync(inside, 'utf8')).toBe('x');
    expect(fs.readFileSync(outside, 'utf8')).toBe('x');
  });
});

describe('/octocode sessions with other processes and forks', () => {
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
});

describe('/octocode sessions without a UI', () => {
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
});
