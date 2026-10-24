import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Team } from '../src/team/session.js';
import { INTERNAL_MESSAGE_TYPE, USER_SENDER } from '../src/shared/messages.js';
import { registerCollab } from '../src/team/tools.js';
import { TeamStore } from '../src/team/store.js';
import { agentsCommand } from '../src/subagents/command.js';
import octocode from '../src/index.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { repo, tmp } from './helpers.js';

const teams: Team[] = [];
const stores: TeamStore[] = [];
afterEach(() => {
  for (const team of teams.splice(0)) team.stop();
  for (const store of stores.splice(0)) store.close();
});

function agent(db: string, cwd: string, env: Record<string, string> = {}, options: { hasUI?: boolean; idle?: boolean } = {}) {
  const fake = fakePi();
  const team = new Team(fake.pi, { OCTOCODE_AGENT_DB: db, ...env });
  registerCollab(fake.pi, team);
  const ctx = fakeCtx({ cwd, hasUI: options.hasUI ?? true, idle: options.idle ?? true });
  const abort = vi.spyOn(ctx, 'abort');
  team.start(ctx);
  teams.push(team);
  return { ...fake, team, ctx, abort };
}

/** A parent session and one headless subagent it started, busy unless `idle`. */
function family(idle = false) {
  const cwd = repo();
  const db = path.join(tmp(), 'team.sqlite');
  const parent = agent(db, cwd);
  const parentId = parent.team.join().id;
  const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-777777', OCTOCODE_PARENT_ID: parentId }, { hasUI: false, idle });
  child.team.join();
  const store = TeamStore.open(cwd, db);
  stores.push(store);
  return { cwd, db, parent, parentId, child, store };
}

const mail = (sent: Array<{ message: any; options: any }>) => sent.filter((entry) => entry.message.customType === INTERNAL_MESSAGE_TYPE && entry.message.details?.messages?.some((m: { id?: number }) => typeof m.id === 'number'));

describe('interrupt', () => {
  it('retries held mail when an interactive session temporarily refuses the settlement send', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const parent = agent(db, cwd);
    const parentId = parent.team.join().id;
    const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-retry', OCTOCODE_PARENT_ID: parentId }, { idle: false });
    child.team.join();
    const { id } = (await parent.team.send(child.team.id!, 'redirect', { replyRequired: false, interrupt: true })) as { id: number };
    await vi.waitFor(() => expect(child.abort).toHaveBeenCalledTimes(1), { timeout: 3_000 });
    vi.spyOn(child.pi, 'sendMessage').mockImplementationOnce(() => { throw new Error('session not ready'); });
    await child.emit('agent_settled', {}, child.ctx);
    expect(mail(child.sent)).toHaveLength(0);
    vi.spyOn(child.ctx, 'isIdle').mockReturnValue(true);
    await vi.waitFor(() => expect(mail(child.sent)).toHaveLength(1), { timeout: 3_000 });
    await child.emit('context', { messages: [{ role: 'custom', customType: INTERNAL_MESSAGE_TYPE, details: mail(child.sent)[0]!.message.details }] }, child.ctx);
    const store = TeamStore.open(cwd, db);
    stores.push(store);
    expect(store.recent().find((entry) => entry.id === id)?.state).toBe('delivered');
  });

  it('is stored with the delivery and always wakes', () => {
    const { child, store, parentId } = family();
    store.send(parentId, [child.team.id!], 'stop', { replyRequired: false, interrupt: true });
    const [message] = store.pending(child.team.id!);
    expect(message).toMatchObject({ interrupt: true, wake: true });
  });

  it('reaches only subagents the sender started, or any subagent when the user sends it', async () => {
    const { cwd, db, parent, child } = family();
    const peer = agent(db, cwd);
    peer.team.join();
    expect((await peer.team.send(child.team.id!, 'stop', { interrupt: true }))).toEqual({ error: expect.stringContaining('only a subagent you started') });
    expect((await parent.team.send(peer.team.id!, 'stop', { interrupt: true }))).toEqual({ error: expect.stringContaining('only a subagent you started') });
    expect((await parent.team.send('all', 'stop', { interrupt: true }))).toEqual({ error: expect.stringContaining('one subagent') });
    expect((await child.team.send('parent', 'stop', { interrupt: true }))).toHaveProperty('error');
    expect((await parent.team.send(child.team.id!, 'stop', { interrupt: true }))).toMatchObject({ sent: [child.team.id] });
    expect((await parent.team.send(child.team.id!, 'redirect', { from: USER_SENDER, interrupt: true }))).toMatchObject({ sent: [child.team.id] });
  });

  it('stops a busy subagent\'s current step and continues the same run with the message once it settles', async () => {
    const { parent, child, store } = family(false);
    const { id } = (await parent.team.send(child.team.id!, 'stop, use the v2 API instead', { replyRequired: false, interrupt: true })) as { id: number };
    await vi.waitFor(() => expect(child.abort).toHaveBeenCalledTimes(1), { timeout: 3_000 });
    // Not steered: a queued steer would only be read after the running tool finished.
    expect(mail(child.sent)).toEqual([]);
    await child.emit('agent_settled', {}, child.ctx);
    const [continued] = mail(child.sent);
    expect(continued!.options).toEqual({ triggerTurn: true, deliverAs: 'steer' });
    expect(continued!.message.content).toContain('stop, use the v2 API instead');
    // It stays in the team for the continued run instead of leaving.
    expect(parent.team.members().map((member) => member.id)).toContain(child.team.id);
    await child.emit('context', { messages: [{ role: 'custom', customType: INTERNAL_MESSAGE_TYPE, details: continued!.message.details }] }, child.ctx);
    expect(store.recent().find((entry) => entry.id === id)?.state).toBe('delivered');
    // The continued run's own settle ends it as usual.
    await child.emit('agent_settled', {}, child.ctx);
    expect(parent.team.members().map((member) => member.id)).not.toContain(child.team.id);
  });

  it('signals a held interrupt, so a wait for background work at the end of the run can stop early', async () => {
    const { parent, child } = family(false);
    expect(child.team.interrupted.aborted).toBe(false);
    (await parent.team.send(child.team.id!, 'stop waiting', { replyRequired: false, interrupt: true }));
    await vi.waitFor(() => expect(child.team.interrupted.aborted).toBe(true), { timeout: 3_000 });
    await child.emit('agent_settled', {}, child.ctx);
    // A fresh signal for the next interrupt.
    expect(child.team.interrupted.aborted).toBe(false);
  });

  it('ends a subagent\'s wait for background work at the end of its run', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const saved = { ...process.env };
    Object.assign(process.env, { OCTOCODE_HOME: tmp(), OCTOCODE_AGENT_DB: db, OCTOCODE_AGENT_ID: 'general-121212', OCTOCODE_PARENT_ID: 'main-aaaaaa' });
    const fake = fakePi();
    const ctx = fakeCtx({ cwd, hasUI: false, idle: false });
    try {
      octocode(fake.pi);
      await fake.emit('session_start', { type: 'session_start' }, ctx);
      await fake.tools.get('bash').execute('t1', { command: 'sleep 30', background: true, timeout: 60 }, undefined, undefined, ctx);
      const store = TeamStore.open(cwd, db);
      stores.push(store);
      store.save({ id: 'main-aaaaaa', role: 'main', pid: process.pid, status: 'working', joinedAt: 1, updatedAt: Date.now(), toolCalls: 0, input: 0, cacheRead: 0, output: 0, cost: 0, unpriced: false });
      store.send('main-aaaaaa', ['general-121212'], 'stop waiting, new plan', { replyRequired: false, interrupt: true });
      const started = Date.now();
      await fake.emit('agent_before_settle', { type: 'agent_before_settle', outcome: 'completed', entries: [], continue: false }, ctx);
      expect(Date.now() - started).toBeLessThan(8_000);
    } finally {
      await fake.emit('session_shutdown', { type: 'session_shutdown' }, ctx);
      process.env = saved;
    }
  }, 20_000);

  it('says the step was stopped only when it was', async () => {
    const { cwd, db, parent, parentId, child } = family(false);
    (await parent.team.send(child.team.id!, 'switch tasks', { replyRequired: false, interrupt: true }));
    await vi.waitFor(() => expect(child.abort).toHaveBeenCalled(), { timeout: 3_000 });
    await child.emit('agent_settled', {}, child.ctx);
    expect(mail(child.sent)[0]!.message.content).toContain('your current step was stopped');
    // One with a UI that is idle is woken, not stopped.
    const ui = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-999999', OCTOCODE_PARENT_ID: parentId }, { hasUI: true, idle: true });
    ui.team.join();
    (await parent.team.send(ui.team.id!, 'new direction', { replyRequired: false, interrupt: true }));
    await vi.waitFor(() => expect(mail(ui.sent)).toHaveLength(1), { timeout: 3_000 });
    expect(mail(ui.sent)[0]!.message.content).not.toContain('was stopped');
    expect(ui.abort).not.toHaveBeenCalled();
  });

  it('delivers an interrupt as a plain waking message when the session cannot abort', async () => {
    const { parent, child } = family(false);
    child.abort.mockImplementation(() => { throw new Error('session ended'); });
    (await parent.team.send(child.team.id!, 'stop', { replyRequired: false, interrupt: true }));
    await vi.waitFor(() => expect(mail(child.sent)).toHaveLength(1), { timeout: 3_000 });
    expect(mail(child.sent)[0]!.options).toEqual({ triggerTurn: true, deliverAs: 'steer' });
    expect(child.team.interrupted.aborted).toBe(false);
  });

  it('lets the user interrupt only subagents of this session', async () => {
    const { cwd, db, child } = family();
    const other = agent(db, cwd);
    const otherId = other.team.join().id;
    const theirs = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-555555', OCTOCODE_PARENT_ID: otherId }, { hasUI: false, idle: false });
    theirs.team.join();
    expect((await other.team.send(child.team.id!, 'stop', { from: USER_SENDER, interrupt: true }))).toEqual({ error: expect.stringContaining('only a subagent of this session') });
    expect((await other.team.send(theirs.team.id!, 'stop', { from: USER_SENDER, interrupt: true }))).toMatchObject({ sent: [theirs.team.id] });
  });

  it('delivers to an agent with a UI that is idle as a waking message, without aborting', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const parent = agent(db, cwd);
    const parentId = parent.team.join().id;
    const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-888888', OCTOCODE_PARENT_ID: parentId }, { hasUI: true, idle: true });
    child.team.join();
    (await parent.team.send(child.team.id!, 'new direction', { replyRequired: false, interrupt: true }));
    await vi.waitFor(() => expect(mail(child.sent)).toHaveLength(1), { timeout: 3_000 });
    expect(mail(child.sent)[0]!.options).toEqual({ triggerTurn: true, deliverAs: 'steer' });
    expect(child.abort).not.toHaveBeenCalled();
  });

  it('stops a manual /compact and delivers the held interrupt once the session is idle, with no agent_settled', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const parent = agent(db, cwd);
    const parentId = parent.team.join().id;
    // Busy only with a compaction: Pi's isIdle is false, and no agent run ends when abort() cancels it.
    const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-compact', OCTOCODE_PARENT_ID: parentId }, { idle: false });
    child.team.join();
    await parent.team.send(child.team.id!, 'drop the compaction, do this', { replyRequired: false, interrupt: true });
    await vi.waitFor(() => expect(child.abort).toHaveBeenCalledTimes(1), { timeout: 3_000 });
    expect(mail(child.sent)).toHaveLength(0);
    vi.spyOn(child.ctx, 'isIdle').mockReturnValue(true);
    await vi.waitFor(() => expect(mail(child.sent)).toHaveLength(1), { timeout: 3_000 });
    expect(mail(child.sent)[0]!.message.content).toContain('your current step was stopped');
    expect(mail(child.sent)[0]!.options).toEqual({ triggerTurn: true, deliverAs: 'steer' });
  });

  it('is offered by the sendMessage tool and /agents interrupt', async () => {
    const { parent, child } = family(false);
    const send = parent.tools.get('sendMessage');
    expect(Object.keys(send.parameters.properties)).toContain('interrupt');
    const result = await send.execute('c', { to: child.team.id, message: 'stop', interrupt: true }, undefined, undefined, parent.ctx);
    expect(result.content[0].text).toMatch(/^Interrupt queued for general-777777 as message #\d+:.*within a second\.$/);
    const command = agentsCommand(parent.team, new Map());
    await command.handler(`interrupt ${child.team.id} switch to the docs task`, parent.ctx);
    expect(parent.ctx.ui.notes.at(-1)).toEqual({ message: 'Interrupt queued for general-777777', type: 'info' });
    await command.handler('interrupt', parent.ctx);
    expect(parent.ctx.ui.notes.at(-1)).toEqual({ message: 'Usage: /agents interrupt <id> <message>', type: 'warning' });
    expect(command.complete!('int')).toEqual([expect.objectContaining({ value: 'interrupt ' })]);
  });
});
