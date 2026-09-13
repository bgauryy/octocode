import { afterEach, expect, it } from 'vitest';
import { makeMockAgentProcess } from './helpers/mock-process.js';
import { setAgentProcessFactoryForTests } from '../src/tools/agents/registry.js';
import { spawnRpcAgent } from '../src/tools/agents/process.js';
import { summarizeAgent } from '../src/tools/agents/rendering.js';
import { normalizeWorkerOutput } from '../src/tools/agents/normalization.js';

afterEach(() => setAgentProcessFactoryForTests(null));

it.each(['DONE', 'BLOCKED'])('inspection does not resurrect the previous %s handback during a follow-up', terminal => {
  const child = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => child as never);
  const worker = spawnRpcAgent({ task: 'Review the assigned evidence.', resourceMode: 'lean' });
  const emit = (event: unknown) => child._emit('stdout:data', Buffer.from(`${JSON.stringify(event)}\n`));
  const previous = { role: 'assistant', content: [{ type: 'text', text: `[${terminal}] Previous assignment` }] };
  emit({ type: 'agent_start' });
  emit({ type: 'message_end', message: previous });
  emit({ type: 'agent_end', messages: [previous] });
  expect(summarizeAgent(worker).normalizedResult?.status).toBe(terminal.toLowerCase());

  emit({ type: 'agent_start' });
  const current = summarizeAgent(worker);
  expect(current.status).toBe('running');
  expect(current.normalizedResult?.status).not.toBe(terminal.toLowerCase());
  expect(worker.messages).toContainEqual(previous);
  expect(worker.lastOutput).toBe(previous.content[0]!.text);

  emit({ type: 'message_end', message: { role: 'assistant', content: [{ type: 'text', text: '[RESULT] New evidence\n[DONE] Follow-up verified' }] } });
  expect(summarizeAgent(worker).normalizedResult).toMatchObject({ status: 'done', result: 'New evidence' });
});

it('projects the reviewer verdict as a result while preserving all criteria and gaps', () => {
  const result = normalizeWorkerOutput('[VERDICT] WARN\n[CRITERIA] retry: PASS\n[GAPS] live provider unverified\n[CONFIDENCE] likely\n[DONE] Assigned review complete');
  expect(result).toMatchObject({ status: 'done', result: 'WARN', confidence: 'likely' });
  expect(result.rawPrefixes).toMatchObject({ CRITERIA: ['retry: PASS'], GAPS: ['live provider unverified'] });
});

it.each(['DONE', 'BLOCKED', 'FAILED'])('preserves the concise %s summary without requiring a duplicate RESULT line', terminal => {
  expect(normalizeWorkerOutput(`[${terminal}] Bounded outcome`).result).toBe('Bounded outcome');
});

it.each(['unconfirmed', 'not confirmed', 'unlikely'])('does not promote invalid confidence text "%s" into evidence', confidence => {
  expect(normalizeWorkerOutput(`[CONFIDENCE] ${confidence}`).confidence).toBe('uncertain');
});
