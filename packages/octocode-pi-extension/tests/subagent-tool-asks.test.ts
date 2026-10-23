import fs from 'node:fs';
import path from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TeamStore } from '../src/team/store.js';
import { tmp } from './helpers.js';
import { setup, useScriptedPi } from './subagent-tool-harness.js';

useScriptedPi();

/** The run id a progress update of the agent tool carries in its details. */
function runId(update: unknown): string {
  const details = typeof update === 'object' && update !== null && 'details' in update ? update.details : undefined;
  const id = typeof details === 'object' && details !== null && 'id' in details ? details.id : undefined;
  if (typeof id !== 'string') throw new Error('expected a progress update with a run id');
  return id;
}

describe('agent tool with a scripted pi child', () => {
  let s: ReturnType<typeof setup>;
  beforeEach(() => {
    s = setup();
  });

  /** The background reports sent to the parent (its team mail, the child's questions, is sent too). */
  const reports = () => s.sent.filter((entry) => entry.message.customType === 'octocode-agent-result');

  /** Acts as the gated child's team: joins it, asks its parent twice (one FYI too), and returns the store to answer with. */
  async function askAsChild(id: string) {
    const store = TeamStore.open(s.cwd, process.env['OCTOCODE_AGENT_DB']);
    const parentId = s.team.id!;
    store.save({ id, role: 'general', parentId, pid: process.pid, status: 'working', joinedAt: Date.now(), updatedAt: Date.now(), toolCalls: 0, input: 0, cacheRead: 0, output: 0, cost: 0, unpriced: false });
    const first = store.send(id, [parentId], 'Which database should the migration target?', { replyRequired: true }).id!;
    store.send(id, [parentId], 'FYI: tests are slow', { replyRequired: false });
    const second = store.send(id, [parentId], 'Rename the column too?', { replyRequired: true }).id!;
    return { store, parentId, first, second };
  }

  it('records a child\'s questions to its parent in the run details, the report footer and /agents', async () => {
    const gate = path.join(tmp(), 'gate');
    process.env['OCTOCODE_FAKE_GATE'] = gate;
    const started = await s.run({ task: 'gated background', background: true });
    const id: string = started.details.id;
    const { store, parentId, first, second } = await askAsChild(id);
    try {
      await vi.waitFor(() => expect(store.asks(id, parentId, 0)).toHaveLength(2));
      // The parent answers the first question; the second stays open. /agents shows the live count.
      store.send(parentId, [id], 'Postgres', { replyRequired: false, replyTo: first });
      await s.commands.get('agents').handler('', s.ctx);
      expect(s.ctx.ui.notes.at(-1)!.message).toMatch(/\n {2}2 asks \(1 answered in \d+s, 1 unanswered\) to its parent/);
      fs.writeFileSync(gate, '');
      await vi.waitFor(() => expect(reports()).toHaveLength(1), { timeout: 10_000 });
    } finally {
      store.close();
      delete process.env['OCTOCODE_FAKE_GATE'];
    }
    const { message } = reports()[0];
    expect(String(message.content)).toMatch(/\nAsks to its parent: 2 asks \(1 answered in \d+s, 1 unanswered\)\.$/);
    expect(message.details.asks).toEqual([
      { id: first, text: 'Which database should the migration target?', askedAt: expect.any(Number), answer: { id: expect.any(Number), at: expect.any(Number), by: parentId } },
      { id: second, text: 'Rename the column too?', askedAt: expect.any(Number) },
    ]);
    expect(message.details.asks[0].answer.at).toBeGreaterThanOrEqual(message.details.asks[0].askedAt);
  });

  it('returns a foreground run\'s asks in its details and marks an answer that followed an askUser call', async () => {
    const gate = path.join(tmp(), 'gate');
    process.env['OCTOCODE_FAKE_GATE'] = gate;
    let id: string | undefined;
    const running = s.run({ task: 'gated foreground' }, undefined, (update) => (id ||= runId(update)));
    try {
      await vi.waitFor(() => expect(id).toBeTruthy());
      const { store, parentId, first } = await askAsChild(id!);
      try {
        // The parent asks the user, then answers: the answer is marked.
        await s.emit('tool_execution_start', { toolCallId: 'ask-1', toolName: 'askUser' }, s.ctx);
        await s.emit('tool_execution_end', { toolCallId: 'ask-1', toolName: 'askUser' }, s.ctx);
        store.send(parentId, [id!], 'Postgres, the user said', { replyRequired: false, replyTo: first });
      } finally {
        store.close();
      }
      fs.writeFileSync(gate, '');
      const result = await running;
      expect(result.details.asks.map((ask: { id: number; answer?: { afterAskUser?: boolean } }) => [ask.id, ask.answer?.afterAskUser])).toEqual([[first, true], [first + 2, undefined]]);
      expect(result.content[0].text).toMatch(/\nAsks to its parent: 2 asks \(1 answered in \d+s \(1 after askUser\), 1 unanswered\)\./);
    } finally {
      fs.writeFileSync(gate, '');
      delete process.env['OCTOCODE_FAKE_GATE'];
    }
  });

  it('omits asks from a run that asked nothing', async () => {
    const result = await s.run({ task: 'report quickly' });
    expect(result.details).not.toHaveProperty('asks');
    expect(result.content[0].text).not.toContain('Asks to its parent');
  });
});
