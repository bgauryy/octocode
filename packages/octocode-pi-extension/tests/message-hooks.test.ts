import fs from 'node:fs';
import path from 'node:path';
import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { registerHooks } from '../src/hooks/register.js';
import { interpret } from '../src/hooks/runner.js';
import type { MessageEnvelope } from '../src/shared/messages.js';
import { fingerprint, projectConfigFiles, saveDecision } from '../src/shared/trust.js';
import { Team } from '../src/team/session.js';
import { INTERNAL_MESSAGE_TYPE } from '../src/shared/messages.js';
import { registerCollab } from '../src/team/tools.js';
import { TeamStore } from '../src/team/store.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { repo, tmp } from './helpers.js';

type Command = { type: 'command'; command: string; timeout?: number; failClosed?: boolean };
const group = (command: string, matcher = '', extra: Partial<Command> = {}) => [{ matcher, hooks: [{ type: 'command', command, ...extra }] }];
const outbound = (extra: Partial<MessageEnvelope> = {}): MessageEnvelope => ({ channel: 'internal', direction: 'outbound', from: 'sender-a', to: 'receiver-b', text: 'original body', ...extra });
const inbound = (extra: Partial<MessageEnvelope> = {}): MessageEnvelope => outbound({ direction: 'inbound', id: 'm-1', ...extra });

function project(hooks: unknown): string {
  const cwd = repo('octocode-message-hooks-');
  fs.mkdirSync(path.join(cwd, '.codex'));
  fs.writeFileSync(path.join(cwd, '.codex', 'hooks.json'), JSON.stringify({ hooks }));
  saveDecision(cwd, fingerprint(projectConfigFiles(cwd)), true);
  return cwd;
}

const teams: Team[] = [];
beforeEach(() => vi.stubEnv('OCTOCODE_HOME', tmp('octocode-message-hook-home-')));
afterEach(() => { for (const team of teams.splice(0)) team.stop(); });

async function setup(config: unknown, enabled = true) {
  const cwd = project(config);
  const fake = fakePi();
  const ctx = fakeCtx({ cwd, hasUI: false });
  const failures: unknown[] = [];
  const hooks = registerHooks(fake.pi, enabled ? { OCTOCODE_HOOKS: '1' } : {}, tmp(), (...failure) => failures.push(failure));
  await fake.fire('session_start', { reason: 'startup' }, ctx);
  return { ...fake, cwd, ctx, hooks, failures };
}

const recorded = (cwd: string, file = 'received.jsonl') => fs.readFileSync(path.join(cwd, file), 'utf8').trim().split('\n').map((line) => JSON.parse(line));

describe('message hooks', () => {
  it('does nothing while hooks are off, including receive event registration', async () => {
    const { hooks, ctx, cwd, handlers } = await setup({ PreMessageSend: group('touch sent'), PostMessageReceive: group('touch received') }, false);
    expect(await hooks.messageSend(outbound(), ctx)).toBeUndefined();
    await hooks.messageReceive(inbound(), ctx);
    expect(handlers.has('context')).toBe(false);
    expect(fs.existsSync(path.join(cwd, 'sent'))).toBe(false);
    expect(fs.existsSync(path.join(cwd, 'received'))).toBe(false);
  });

  it('matches internal/external direction and carries original sender, body and correlation once per command', async () => {
    const command = `cat >> sent.jsonl; printf '\\n' >> sent.jsonl`;
    const { hooks, ctx, cwd } = await setup({ PreMessageSend: [...group(command, 'internal.outbound'), ...group(command, 'internal')] });
    const message = outbound({ replyTo: '17' });
    expect(await hooks.messageSend(message, ctx)).toBeUndefined();
    expect(recorded(cwd, 'sent.jsonl')).toEqual([expect.objectContaining({ hook_event_name: 'PreMessageSend', session_id: ctx.sessionManager.getSessionId(), message })]);
    await hooks.messageSend(outbound({ channel: 'external' }), ctx);
    expect(recorded(cwd, 'sent.jsonl')).toHaveLength(1);
  });

  it('supports deny/ask while discarding context and ignores every receive verdict', async () => {
    const run = { code: 0, stdout: JSON.stringify({ hookSpecificOutput: { permissionDecision: 'ask', permissionDecisionReason: 'confirm destination', additionalContext: 'rewrite this' } }), stderr: '', timedOut: false };
    expect(interpret('PreMessageSend', run)).toEqual({ ask: 'confirm destination' });
    expect(interpret('PostMessageReceive', { ...run, code: 2, stderr: 'deny' })).toEqual({});
    expect(interpret('PostMessageReceive', { ...run, stdout: '{"decision":"block","reason":"send a reply"}' })).toEqual({});
    const { hooks, ctx } = await setup({ PreMessageSend: group(`echo '${run.stdout}'`) });
    expect(await hooks.messageSend(outbound(), ctx)).toEqual({ ask: 'confirm destination' });
  });

  it('audits fail-open timeout and denies a failClosed timeout', async () => {
    for (const failClosed of [false, true]) {
      const { hooks, ctx, failures } = await setup({ PreMessageSend: group('sleep 5', '', { timeout: 0.1, failClosed }) });
      const result = await hooks.messageSend(outbound(), ctx);
      expect(result).toEqual(failClosed ? { block: expect.stringContaining('PreMessageSend hook timed out after 0.1s and is set to failClosed') } : undefined);
      expect(failures).toEqual([[['PreMessageSend hook timed out after 0.1s: sleep 5', 'message: sender-a -> receiver-b'], expect.any(String), !failClosed]]);
    }
  });

  it('observes actual internal and API delivery once without acknowledging, replying or changing context', async () => {
    const command = `cat >> received.jsonl; printf '\\n' >> received.jsonl; echo '{"decision":"block","reason":"reply now"}'`;
    const { fire, ctx, cwd, sent } = await setup({ PostMessageReceive: group(command) });
    const mail = { role: 'custom', customType: INTERNAL_MESSAGE_TYPE, details: { messages: [{ id: 4, from: 'agent-a', to: 'agent-b', text: 'internal body', replyTo: 3 }] } };
    const apiMessage = inbound({ channel: 'external', from: 'api-client', id: 'api-1', text: 'raw external body' });
    const api = { role: 'custom', customType: 'octocode-external-message', content: 'fenced body', details: { message: apiMessage } };
    const messages = [mail, api];
    expect(await fire('context', { messages }, ctx)).toBeUndefined();
    await fire('context', { messages: structuredClone(messages) }, ctx);
    expect(recorded(cwd).map((record) => record.message)).toEqual([
      inbound({ id: '4', from: 'agent-a', to: 'agent-b', text: 'internal body', replyTo: '3' }), apiMessage,
    ]);
    expect(mail.details.messages[0]).not.toHaveProperty('acked');
    expect(sent).toEqual([]);
    expect(messages).toEqual([mail, api]);
  });

  it('skips resumed history and resets receive identity when the session changes', async () => {
    const { fire, hooks, ctx, cwd } = await setup({ PostMessageReceive: group(`cat >> received.jsonl; printf '\\n' >> received.jsonl`) });
    const message = inbound({ channel: 'external' });
    const entry = { type: 'custom_message', customType: 'octocode-external-message', details: { message } };
    const restored = { ...ctx, sessionManager: { ...ctx.sessionManager, getBranch: () => [entry] } };
    await fire('session_start', { reason: 'resume' }, restored);
    await hooks.messageReceive(message, restored as never);
    expect(fs.existsSync(path.join(cwd, 'received.jsonl'))).toBe(false);
    await fire('session_start', { reason: 'new' }, ctx);
    await hooks.messageReceive(message, ctx);
    expect(recorded(cwd)).toHaveLength(1);
  });

  it('cancels old session sends and observers without returning observer failures', async () => {
    const command = 'echo ready > started; sleep 10; echo finished > finished';
    const { hooks, fire, ctx, cwd } = await setup({ PreMessageSend: group(command), PostMessageReceive: group(command) });
    const send = hooks.messageSend(outbound(), ctx);
    const rejected = expect(send).rejects.toThrow();
    await vi.waitFor(() => expect(fs.existsSync(path.join(cwd, 'started'))).toBe(true));
    await fire('session_start', { reason: 'new' }, ctx);
    await rejected;
    fs.unlinkSync(path.join(cwd, 'started'));
    const receive = hooks.messageReceive(inbound(), ctx);
    await vi.waitFor(() => expect(fs.existsSync(path.join(cwd, 'started'))).toBe(true));
    await fire('session_shutdown', {}, ctx);
    await expect(receive).resolves.toBeUndefined();
    expect(fs.existsSync(path.join(cwd, 'finished'))).toBe(false);
  });

  it('denies direct, tool and API-origin Team sends before storage or finished-agent resume', async () => {
    const { hooks, pi, ctx, tools, cwd } = await setup({ PreMessageSend: group('echo forbidden >&2; exit 2', 'internal') });
    const dbFile = path.join(tmp(), 'team.sqlite');
    const team = new Team(pi, { OCTOCODE_AGENT_DB: dbFile });
    const other = new Team(fakePi().pi, { OCTOCODE_AGENT_DB: dbFile });
    teams.push(team, other);
    registerCollab(pi, team);
    team.start(ctx);
    other.start(ctx);
    team.join();
    const receiver = other.join().id;
    const gate = vi.fn(async (message: MessageEnvelope, context: ExtensionContext) => {
      const verdict = await hooks.messageSend(message, context);
      if (verdict?.block) throw new Error(verdict.block);
    });
    team.setMessageHook(gate);
    for (const from of [undefined, 'api:client']) {
      expect(await team.send(receiver, 'do work', { from })).toEqual({ error: 'Blocked by a PreMessageSend hook: forbidden' });
    }
    const tool = tools.get('sendMessage');
    await expect(tool.execute('t1', { to: receiver, message: 'tool body' }, undefined, undefined, ctx)).rejects.toThrow('forbidden');
    const resumed = vi.fn(async () => 'resumed');
    team.rememberFinished('done-aaaaaa', 'done');
    team.setResumer(resumed);
    expect(await team.send('done-aaaaaa', 'more work')).toEqual({ error: 'Blocked by a PreMessageSend hook: forbidden' });
    expect(resumed).not.toHaveBeenCalled();
    expect(gate.mock.calls.map(([message]) => [message.from, message.to])).toEqual([[team.id, receiver], ['api:client', receiver], [team.id, receiver], [team.id, 'done-aaaaaa']]);
    const store = TeamStore.open(cwd, dbFile, {});
    try { expect(store.recent()).toEqual([]); } finally { store.close(); }
  });

  it('cancels a Team send hook on the tool signal before its own timeout', async () => {
    const { hooks, pi, ctx, fire, cwd } = await setup({ PreMessageSend: group('echo running > hook-started; sleep 10; echo done > hook-finished', '', { timeout: 2 }) });
    const dbFile = path.join(tmp(), 'team.sqlite');
    const team = new Team(pi, { OCTOCODE_AGENT_DB: dbFile });
    const peer = new Team(fakePi().pi, { OCTOCODE_AGENT_DB: dbFile });
    teams.push(team, peer);
    team.start(ctx);
    peer.start(ctx);
    team.join();
    const receiver = peer.join().id;
    team.setMessageHook(async (message, context, signal) => {
      const verdict = await hooks.messageSend(message, signal ? { ...context, signal } : context);
      if (verdict?.block) throw new Error(verdict.block);
    });
    const controller = new AbortController();
    const sending = team.send(receiver, 'cancel me', { signal: controller.signal });
    let timer: NodeJS.Timeout | undefined;
    try {
      await vi.waitFor(() => expect(fs.existsSync(path.join(cwd, 'hook-started'))).toBe(true));
      controller.abort();
      const deadline = new Promise<string>((resolve) => { timer = setTimeout(() => resolve('hook did not cancel promptly'), 1_000); });
      expect(await Promise.race([sending, deadline])).toEqual({ error: expect.any(String) });
      expect(fs.existsSync(path.join(cwd, 'hook-finished'))).toBe(false);
      const store = TeamStore.open(cwd, dbFile, {});
      try { expect(store.recent()).toEqual([]); } finally { store.close(); }
    } finally {
      clearTimeout(timer);
      await fire('session_shutdown', {}, ctx);
      await sending;
    }
  });
});
