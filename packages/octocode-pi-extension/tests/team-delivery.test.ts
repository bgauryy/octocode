import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { INTERNAL_MESSAGE_TYPE, USER_SENDER } from '../src/shared/messages.js';
import { resolveTarget, sessionTree } from '../src/team/routing.js';
import type { Member } from '../src/team/model.js';
import { TeamStore } from '../src/team/store.js';
import { tmp } from './helpers.js';
import { agent, closeTeams, fakeClock, mailOf, member, openStore, repo, seen, teamMail } from './team-harness.js';

afterEach(closeTeams);

describe('message wake rule', () => {
  it('wakes the asker with the answer to its open question, never with a thank-you or a second answer', () => {
    const store = openStore(repo(), path.join(tmp(), 'team.sqlite'));
    store.save(member('a'));
    store.save(member('b'));
    const question = store.send('a', ['b'], 'which file?', { replyRequired: true }).id!;
    expect(store.pending('b')[0]!.wake).toBe(true);
    const answer = store.send('b', ['a'], 'src/x.ts', { replyRequired: false, replyTo: question }).id!;
    expect(store.pending('a').find((message) => message.id === answer)).toMatchObject({ wake: true });
    // A reply to an FYI (the answer asked for nothing) does not wake its recipient.
    const thanks = store.send('a', ['b'], 'thanks', { replyRequired: false, replyTo: answer }).id!;
    expect(store.pending('b').find((message) => message.id === thanks)).toMatchObject({ wake: false });
    // The question is answered: answering it again is an FYI.
    const again = store.send('b', ['a'], 'also y.ts', { replyRequired: false, replyTo: question }).id!;
    expect(store.pending('a').find((message) => message.id === again)).toMatchObject({ wake: false });
    expect(store.replyTarget(question)).toEqual({ from: 'a', replyRequired: true });
    expect(store.replyTarget(999_999)).toBeUndefined();
  });

  it('refuses a replyTo that names no message in the repository', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const a = agent(db, cwd);
    const b = agent(db, cwd);
    a.team.join();
    b.team.join();
    expect((await a.team.send(b.team.id!, 'hi', { replyTo: 424242 }))).toEqual({ error: expect.stringContaining('No message #424242') });
  });
});

describe('broadcast scope', () => {
  it('reaches the sender\'s session tree only, never another session in the same repository', () => {
    const members = [
      member('main-aaaaaa'), member('researcher-bbbbbb', { parentId: 'main-aaaaaa' }), member('reviewer-cccccc', { parentId: 'researcher-bbbbbb' }),
      member('main-zzzzzz'), member('general-yyyyyy', { parentId: 'main-zzzzzz' }),
    ];
    expect([...sessionTree(members, 'reviewer-cccccc')].sort()).toEqual(['main-aaaaaa', 'researcher-bbbbbb', 'reviewer-cccccc']);
    const ids = (result: Member[] | string) => (typeof result === 'string' ? result : result.map((m) => m.id).sort());
    expect(ids(resolveTarget(members, 'reviewer-cccccc', 'all'))).toEqual(['main-aaaaaa', 'researcher-bbbbbb']);
    // The user's broadcast is scoped to the session it was typed in.
    expect(ids(resolveTarget(members, USER_SENDER, 'all', undefined, 'main-zzzzzz'))).toEqual(['general-yyyyyy', 'main-zzzzzz']);
    expect(ids(resolveTarget([member('main-aaaaaa'), member('main-zzzzzz')], 'main-aaaaaa', 'all'))).toMatch(/No other agents are in your session tree/);
  });

  it('sends /agents tell all only to this session and its subagents', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const main = agent(db, cwd);
    const mine = main.team.join().id;
    const child = agent(db, cwd, { OCTOCODE_AGENT_ID: 'general-111111', OCTOCODE_PARENT_ID: mine });
    const stranger = agent(db, cwd);
    child.team.join();
    stranger.team.join();
    const result = (await main.team.send('all', 'stop and report', { from: USER_SENDER }));
    expect('sent' in result && result.sent.sort()).toEqual(['general-111111', mine].sort());
  });
});

describe('delivery acknowledgement', () => {
  it('records a message delivered only once the model sees it', async () => {
    fakeClock();
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const a = agent(db, cwd);
    const b = agent(db, cwd);
    a.team.join();
    b.team.join();
    const { id } = (await a.team.send(b.team.id!, 'check auth.ts', { replyRequired: false })) as { id: number };
    await vi.waitFor(() => expect(teamMail(b.sent)).toHaveLength(1), { timeout: 3_000 });
    const store = openStore(cwd, db);
    // Handed to Pi is not read: a queued steer can still be dropped by a kill.
    expect(store.recent().find((entry) => entry.id === id)?.state).toBe('queued');
    await seen(b, teamMail(b.sent)[0]!);
    expect(store.recent().find((entry) => entry.id === id)?.state).toBe('delivered');
    // Never handed over twice, and later model calls (the message stays in the conversation) record nothing again.
    const ack = vi.spyOn(TeamStore.prototype, 'ack');
    await seen(b, teamMail(b.sent)[0]!);
    await vi.advanceTimersByTimeAsync(2_000);
    expect(teamMail(b.sent)).toHaveLength(1);
    expect(ack).not.toHaveBeenCalled();
    ack.mockRestore();
  });

  it('records a quiet FYI once the model is called with it, however Pi added it', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const a = agent(db, cwd);
    const b = agent(db, cwd);
    a.team.join();
    b.team.join();
    const { id } = (await a.team.send(b.team.id!, 'fyi: tests are green', { replyRequired: false })) as { id: number };
    await vi.waitFor(() => expect(teamMail(b.sent)).toHaveLength(1), { timeout: 3_000 });
    expect(teamMail(b.sent)[0]!.options).toEqual({ triggerTurn: false, deliverAs: 'steer' });
    await seen(b, teamMail(b.sent)[0]!);
    expect(openStore(cwd, db).recent().find((entry) => entry.id === id)?.state).toBe('delivered');
  });

  it('records mail the model saw before leaving, even when the database was busy at the time', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const a = agent(db, cwd);
    const b = agent(db, cwd);
    a.team.join();
    b.team.join();
    const { id } = (await a.team.send(b.team.id!, 'read me', { replyRequired: false })) as { id: number };
    await vi.waitFor(() => expect(teamMail(b.sent)).toHaveLength(1), { timeout: 3_000 });
    const ack = vi.spyOn(TeamStore.prototype, 'ack').mockImplementationOnce(() => { throw new Error('database is locked'); });
    try {
      await seen(b, teamMail(b.sent)[0]!);
      b.team.leave();
      expect(openStore(cwd, db).recent().find((entry) => entry.id === id)?.state).toBe('delivered');
    } finally { ack.mockRestore(); }
  });

  it('dead-letters a message handed to Pi that its recipient left without seeing, and tells the sender', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const a = agent(db, cwd);
    const b = agent(db, cwd);
    a.team.join();
    b.team.join();
    const { id } = (await a.team.send(b.team.id!, 'stop, the plan changed', { replyRequired: false })) as { id: number };
    await vi.waitFor(() => expect(teamMail(b.sent)).toHaveLength(1), { timeout: 3_000 });
    b.team.leave(); // killed before Pi drained its steering queue
    await vi.waitFor(() => expect(a.sent.some((entry) => String(entry.message.content).startsWith(`Message #${id} to `))).toBe(true), { timeout: 8_000 });
  }, 10_000);

  it('retries a failed acknowledgement on the next tick', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const a = agent(db, cwd);
    const b = agent(db, cwd);
    a.team.join();
    b.team.join();
    const { id } = (await a.team.send(b.team.id!, 'once', { replyRequired: false })) as { id: number };
    await vi.waitFor(() => expect(teamMail(b.sent)).toHaveLength(1), { timeout: 3_000 });
    const ack = vi.spyOn(TeamStore.prototype, 'ack').mockImplementationOnce(() => { throw new Error('database is locked'); });
    try {
      await seen(b, teamMail(b.sent)[0]!);
      const store = openStore(cwd, db);
      expect(store.recent().find((entry) => entry.id === id)?.state).toBe('queued');
      await vi.waitFor(() => expect(store.recent().find((entry) => entry.id === id)?.state).toBe('delivered'), { timeout: 3_000 });
      expect(teamMail(b.sent)).toHaveLength(1);
    } finally { ack.mockRestore(); }
  });

  it('ignores messages started for another recipient or without team ids', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const b = agent(db, cwd);
    b.team.join();
    const store = openStore(cwd, db);
    store.save(member('peer-000001'));
    store.save(member('other-000002'));
    const id = store.send('peer-000001', ['other-000002'], 'not for b', { replyRequired: false }).id!;
    await b.emit('context', { messages: [{ role: 'custom', customType: INTERNAL_MESSAGE_TYPE, details: { messages: [{ id, to: 'other-000002' }] } }, { role: 'custom', customType: INTERNAL_MESSAGE_TYPE, details: { from: 'team', text: 'notice' } }, { role: 'custom', customType: INTERNAL_MESSAGE_TYPE, details: null }] }, b.ctx);
    expect(store.pending('other-000002').map((message) => message.id)).toEqual([id]);
  });

  it('keeps a question completed when its acknowledgement is retried after the reply', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const a = agent(db, cwd);
    const b = agent(db, cwd);
    a.team.join();
    b.team.join();
    const { id } = (await a.team.send(b.team.id!, 'question', { replyRequired: true })) as { id: number };
    await vi.waitFor(() => expect(teamMail(b.sent)).toHaveLength(1), { timeout: 3_000 });
    const ack = vi.spyOn(TeamStore.prototype, 'ack').mockImplementationOnce(() => { throw new Error('database is locked'); });
    try {
      await seen(b, teamMail(b.sent)[0]!);
      expect((await b.team.send(a.team.id!, 'answer', { replyTo: id, replyRequired: false }))).toMatchObject({ sent: [a.team.id] });
      await vi.waitFor(() => expect(ack).toHaveBeenCalledTimes(2), { timeout: 3_000 });
      expect(openStore(cwd, db).recent().find((entry) => entry.id === id)?.state).toBe('answered');
    } finally { ack.mockRestore(); }
  });
});

describe('batched delivery', () => {
  it('hands every waiting message to Pi as one message, in order, waking when any one asks for action', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const b = agent(db, cwd);
    const me = b.team.join().id;
    const store = openStore(cwd, db);
    store.save(member('peer-000001'));
    store.send('peer-000001', [me], 'first fyi', { replyRequired: false });
    store.send('peer-000001', [me], 'the build failed: which test?', { replyRequired: true });
    store.send('peer-000001', [me], 'third fyi', { replyRequired: false });
    await vi.waitFor(() => expect(teamMail(b.sent)).toHaveLength(1), { timeout: 3_000 });
    const [batch] = teamMail(b.sent);
    expect(batch!.options).toEqual({ triggerTurn: true, deliverAs: 'steer' });
    expect(batch!.message.details.messages.map((message: { text: string }) => message.text)).toEqual(['first fyi', 'the build failed: which test?', 'third fyi']);
    const content = String(batch!.message.content);
    expect(content.indexOf('first fyi')).toBeLessThan(content.indexOf('which test?'));
    expect(content.indexOf('which test?')).toBeLessThan(content.indexOf('third fyi'));
    await seen(b, batch!);
    expect(store.pending(me)).toEqual([]);
  });

  it('keeps a batch of FYIs quiet', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const b = agent(db, cwd);
    const me = b.team.join().id;
    const store = openStore(cwd, db);
    store.save(member('peer-000001'));
    store.send('peer-000001', [me], 'one', { replyRequired: false });
    store.send('peer-000001', [me], 'two', { replyRequired: false });
    await vi.waitFor(() => expect(teamMail(b.sent)).toHaveLength(1), { timeout: 3_000 });
    expect(teamMail(b.sent)[0]!.options).toEqual({ triggerTurn: false, deliverAs: 'steer' });
  });
});

describe('dead letters', () => {
  it('tells the sender once, without waking it, that a recipient left before reading', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const a = agent(db, cwd);
    const b = agent(db, cwd);
    a.team.join();
    b.team.join();
    const sent = (await a.team.send(b.team.id!, 'please check auth.ts\nmore detail', { replyRequired: false }));
    expect('id' in sent).toBe(true);
    const id = (sent as { id: number }).id;
    b.team.leave();
    // Dead letters are checked with the 5-second heartbeat.
    await vi.waitFor(() => expect(a.sent.some((entry) => String(entry.message.content).startsWith(`Message #${id} to ${b.team.id ?? ''}`))).toBe(true), { timeout: 8_000 });
    const notice = a.sent.find((entry) => String(entry.message.content).includes(`Message #${id}`))!;
    expect(notice.message).toMatchObject({ customType: INTERNAL_MESSAGE_TYPE, content: expect.stringContaining('was not read') });
    expect(notice.message.content).toContain('"please check auth.ts"');
    expect(notice.options).toEqual({ triggerTurn: false, deliverAs: 'steer' });
    // Once only.
    const store = openStore(cwd, db);
    expect(store.deadLetters(a.team.id!)).toEqual([]);
  }, 10_000);

  it('marks dead letters notified in the store', () => {
    const store = openStore(repo(), path.join(tmp(), 'team.sqlite'));
    store.save(member('a'));
    store.save(member('b'));
    store.save(member('c'));
    const id = store.send('a', ['b', 'c'], 'hello', { replyRequired: false }).id!;
    store.remove('b');
    store.remove('c');
    const letters = store.deadLetters('a');
    expect(letters).toEqual([{ id, recipients: ['b', 'c'], text: 'hello' }]);
    store.markNotified(letters);
    expect(store.deadLetters('a')).toEqual([]);
  });
});

describe('delivery edge cases', () => {
  it('tells the user once when an idle agent was handed a waking message and Pi started no turn', async () => {
    fakeClock();
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const sender = agent(db, cwd);
    sender.team.join();
    const idle = agent(db, cwd);
    idle.team.join();
    await sender.team.send(idle.team.id!, 'are you there?', { replyRequired: true });
    await vi.waitFor(() => expect(mailOf(idle.sent)).toHaveLength(1), { timeout: 3_000 });
    expect(idle.ctx.ui.notes).toEqual([]);
    // Ten seconds pass and the model was never called with it (no model, or no sign-in).
    vi.setSystemTime(Date.now() + 11_000);
    await vi.waitFor(() => expect(idle.ctx.ui.notes).toHaveLength(1), { timeout: 3_000 });
    expect(idle.ctx.ui.notes[0]).toMatchObject({ type: 'warning', message: expect.stringContaining('did not run one') });
    await vi.advanceTimersByTimeAsync(2_000);
    expect(idle.ctx.ui.notes).toHaveLength(1);
  }, 15_000);

  it('does not warn when the model was called with the message', async () => {
    fakeClock();
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const sender = agent(db, cwd);
    sender.team.join();
    const idle = agent(db, cwd);
    idle.team.join();
    await sender.team.send(idle.team.id!, 'are you there?', { replyRequired: true });
    await vi.waitFor(() => expect(mailOf(idle.sent)).toHaveLength(1), { timeout: 3_000 });
    await idle.emit('context', { messages: [{ role: 'custom', customType: INTERNAL_MESSAGE_TYPE, details: mailOf(idle.sent)[0]!.message.details }] }, idle.ctx);
    vi.setSystemTime(Date.now() + 11_000);
    await vi.advanceTimersByTimeAsync(2_000);
    expect(idle.ctx.ui.notes).toEqual([]);
  }, 15_000);

  it('hands over new messages after the team database was emptied under a live session', async () => {
    const cwd = repo();
    const db = path.join(tmp(), 'team.sqlite');
    const sender = agent(db, cwd);
    sender.team.join();
    const receiver = agent(db, cwd);
    receiver.team.join();
    for (const text of ['one', 'two']) {
      await sender.team.send(receiver.team.id!, text, { replyRequired: false });
    }
    await vi.waitFor(() => expect(mailOf(receiver.sent)).toHaveLength(1), { timeout: 3_000 });
    await receiver.emit('context', { messages: [{ role: 'custom', customType: INTERNAL_MESSAGE_TYPE, details: mailOf(receiver.sent)[0]!.message.details }] }, receiver.ctx);
    // The messages are wiped and their numbers start again, as in a replaced database.
    const { DatabaseSync } = await import('node:sqlite');
    const raw = new DatabaseSync(db);
    raw.exec("DELETE FROM messages; DELETE FROM sqlite_sequence WHERE name = 'messages'");
    raw.close();
    const id = ((await sender.team.send(receiver.team.id!, 'after the reset', { replyRequired: false })) as { id: number }).id;
    expect(id).toBe(1);
    await vi.waitFor(() => expect(mailOf(receiver.sent)).toHaveLength(2), { timeout: 3_000 });
    expect(mailOf(receiver.sent)[1]!.message.content).toContain('after the reset');
  }, 15_000);
});
