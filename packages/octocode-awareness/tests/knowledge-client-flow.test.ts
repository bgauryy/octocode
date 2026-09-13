import { afterEach, expect, it } from 'vitest';
import { mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { createAwarenessClient } from '../src/client.js';
import { executeAwarenessCli } from '../src/command-cli.js';
import type { KnowledgeResult } from '../src/knowledge-recall.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
function fixture() {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'knowledge-client-')));
  roots.push(workspace);
  return { workspace, database: join(workspace, 'awareness.sqlite3'), agentId: 'writer', sessionId: 'investigation' };
}

it('composes CLI set, API get, revision update, and flow-aware orientation', async () => {
  const context = fixture();
  const created = await executeAwarenessCli(['memory', 'set', '--key', 'reply-identity',
    '--title', 'Use a message ID', '--lesson', 'Notification IDs do not identify reply messages.',
    '--why', 'The strict reply contract requires the message identity.',
    '--anchors', JSON.stringify([{ kind: 'flow', value: 'message.reply' }]),
    '--request-id', 'first-revision', '--expected-revision', 'null',
    '--workspace', context.workspace, '--db', context.database, '--agent-id', context.agentId]);
  expect(created.exitCode, JSON.stringify(created.payload)).toBe(0);
  const first = (created.payload as KnowledgeResult).memory!;
  const client = createAwarenessClient(context);
  const orientation = await client.orient({ flow: 'message.reply' });
  expect(orientation).toMatchObject({ knowledge: { advisory: true, memories: [expect.objectContaining({ key: first.key })] } });
  const next = await client.execute({ operation: 'memory.set', params: {
    key: first.key, title: first.title, lesson: 'Read the message first, then reply using its ID.',
    why: 'The earlier lesson omitted how to obtain the ID.', anchors: first.anchors,
    expected_revision: first.revision, request_id: 'second-revision',
  } });
  expect(next.exitCode, JSON.stringify(next.payload)).toBe(0);
  const historical = await client.execute({ operation: 'memory.get', params: { key: first.key, revision: first.revision } });
  expect(historical.exitCode).toBe(0);
  expect((historical.payload as KnowledgeResult).memories).toEqual([expect.objectContaining({
    revision: first.revision, why: first.why, state: 'superseded',
  })]);
});

it('refreshes orientation when declared source evidence changes without a ledger write', async () => {
  const context = fixture();
  writeFileSync(join(context.workspace, 'contract.ts'), 'export const revision = 1;');
  const client = createAwarenessClient(context);
  const stored = await client.execute({ operation: 'memory.set', params: {
    key: 'contract-rule', title: 'Contract rule', lesson: 'Check this contract before editing.',
    anchors: [{ kind: 'file', value: 'contract.ts' }], request_id: 'capture', expected_revision: null,
    applicability: { files: ['contract.ts'], capture_fingerprint: true },
  } });
  expect(stored.exitCode, JSON.stringify(stored.payload)).toBe(0);
  const first = await client.orient({ file: 'contract.ts' });
  expect(first).toMatchObject({ knowledge: { memories: [expect.objectContaining({ evidence: { state: 'fresh', claim_verification: 'unverified' } })] } });
  writeFileSync(join(context.workspace, 'contract.ts'), 'export const revision = 2;');
  const changed = await client.orient({ file: 'contract.ts', if_revision: first.revision });
  expect(changed).toMatchObject({ unchanged: false, knowledge: { memories: [expect.objectContaining({ evidence: { state: 'stale', claim_verification: 'unverified' } })] } });
});

it('surfaces knowledge pagination at the orientation boundary with an executable read', async () => {
  const client = createAwarenessClient(fixture());
  for (let index = 0; index < 4; index++) {
    const saved = await client.execute({ operation: 'memory.set', params: {
      key: `lesson-${index}`, title: `Lesson ${index}`, lesson: 'A scoped lesson.',
      anchors: [{ kind: 'flow', value: 'pagination' }], expected_revision: null, request_id: `write-${index}`,
    } });
    expect(saved.exitCode).toBe(0);
  }
  const orientation = await client.orient({ flow: 'pagination', limit: 1 });
  expect(orientation).toMatchObject({ partial: true, partialReasons: expect.arrayContaining(['knowledge_partial']) });
  if (orientation.unchanged) throw new Error('First orientation cannot be unchanged');
  const read = orientation.next.find(call => call.operation === 'memory.get');
  expect(read).toBeDefined();
  const page = await client.execute(read!);
  expect(page.exitCode).toBe(0);
  expect(page.payload).toMatchObject({ memories: [expect.objectContaining({ key: expect.stringMatching(/^lesson-/) })] });
});
