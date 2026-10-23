import path from 'node:path';
import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { USER_SENDER, type MessageEnvelope } from '../src/shared/messages.js';
import type { Mail } from '../src/team/mail.js';
import { Outbox } from '../src/team/outbox.js';
import { TeamStore } from '../src/team/store.js';
import { stub } from './doubles.js';
import { fakeCtx } from './fake-pi.js';
import { tmp } from './helpers.js';
import { member, repo } from './team-harness.js';

const MAIN = 'main-aaaaaa';
const WORKER = 'worker-bbbbbb';
const stores: TeamStore[] = [];
afterEach(() => {
  for (const store of stores.splice(0)) store.close();
});

/** An outbox for `main` over a fresh team database holding `main` and `worker`; `ctx` is the session it reads. */
function outbox() {
  const cwd = repo();
  const store = TeamStore.open(cwd, path.join(tmp(), 'team.sqlite'), {});
  stores.push(store);
  store.save(member(MAIN));
  store.save(member(WORKER, { parentId: MAIN }));
  const session: { ctx: ExtensionContext | undefined } = { ctx: fakeCtx({ cwd }) };
  const created: number[] = [];
  const box = new Outbox({ id: () => MAIN, parentId: () => undefined, ctx: () => session.ctx, store: () => store }, stub<Mail>({ createdForUser: (id: number) => void created.push(id) }));
  const hooked: MessageEnvelope[] = [];
  return { box, store, session, created, hooked };
}

describe('Outbox.send', () => {
  it('runs the message hook for each recipient, with the message it answers, then stores the message', async () => {
    const { box, store, hooked } = outbox();
    box.messageHook = async (message) => void hooked.push(message);
    const question = store.send(WORKER, [MAIN], 'which file?', { replyRequired: true }).id!;
    const result = await box.send(undefined, 'a.ts', { replyTo: question });
    expect(hooked).toEqual([{ channel: 'internal', direction: 'outbound', from: MAIN, to: WORKER, text: 'a.ts', replyTo: String(question) }]);
    expect(result).toEqual({ sent: [WORKER], id: expect.any(Number) });
    expect(store.pending(WORKER).map((message) => message.text)).toEqual(['a.ts']);
  });

  it('sends nothing when the hook refuses or the session changes while it runs', async () => {
    const { box, store, session } = outbox();
    box.messageHook = async () => { throw new Error('blocked by policy'); };
    expect(await box.send(WORKER, 'hi')).toEqual({ error: 'blocked by policy' });
    box.messageHook = async () => void (session.ctx = fakeCtx({ cwd: '/' }));
    expect(await box.send(WORKER, 'hi')).toEqual({ error: 'The session changed before the message could be sent.' });
    expect(store.pending(WORKER)).toEqual([]);
  });

  it('records a message the user sends as created for the user', async () => {
    const { box, created } = outbox();
    const result = await box.send(WORKER, 'from the user', { from: USER_SENDER });
    expect(created).toEqual(['id' in result ? result.id : -1]);
  });

  it('names what a request lacks before sending anything', async () => {
    const { box } = outbox();
    expect(await box.send(undefined, 'hi')).toEqual({ error: expect.stringMatching(/^Say who to send to/) });
    expect(await box.send(undefined, 'hi', { replyTo: 999 })).toEqual({ error: expect.stringMatching(/^No message #999 in this repository/) });
    expect(await box.send('nobody', 'hi')).toEqual({ error: expect.stringMatching(/^No live agent "nobody"/) });
  });

  it('resumes a finished subagent with the message once the hook allows it, else says it has finished', async () => {
    const { box, hooked } = outbox();
    box.rememberFinished('done-cccccc', 'Its report went to the parent.');
    expect(await box.send('done-cccccc', 'more')).toEqual({ error: 'done-cccccc has finished and takes no more messages; Its report went to the parent. To continue its work, start a new agent and give it the context it needs.' });
    const resumer = vi.fn(async () => 'resumed with your message');
    box.resumer = resumer;
    box.messageHook = async (message) => void hooked.push(message);
    expect(await box.send('done-cccccc', 'more')).toEqual({ sent: ['done-cccccc'], id: 0, resumed: 'resumed with your message' });
    expect(resumer).toHaveBeenCalledWith('done-cccccc', 'more', MAIN, undefined);
    expect(hooked.map((message) => message.to)).toEqual(['done-cccccc']);
    box.messageHook = async () => { throw new Error('not to that one'); };
    expect(await box.send('done-cccccc', 'more')).toEqual({ error: 'not to that one' });
    expect(resumer).toHaveBeenCalledTimes(1);
  });
});
