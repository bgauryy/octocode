import { expect, it } from 'vitest';
import { z } from 'zod';
import { getAwarenessOperationDescriptor } from '../src/schema/operation-catalog.js';

it('publishes executable schemas with the same sensor and feedback requirements as runtime', () => {
  const at = '2026-09-12T12:00:00.000Z';
  const base = { observation_id: 'sample', observed_at: at };
  const feedback = { feedback_id: 'receipt', observed_at: at, advisory_id: 'advice', action_taken: 'changed approach' };
  const cases = [
    ['context.observe', { ...base, context: { used: 9, limit: 10 } }, true],
    ['context.observe', { ...base, progress: { scope: 'tests', value: 0 } }, true],
    ['context.observe', { ...base, action_fingerprint: 'test', outcome: 'success' }, true],
    ['context.observe', base, false],
    ['context.observe', { ...base, action_fingerprint: 'test', context: { used: 9, limit: 10 } }, false],
    ['context.observe', { ...base, outcome: 'success' }, false],
    ['context.feedback', { ...feedback, outcome: 'helpful' }, false],
    ['context.feedback', { ...feedback, outcome: 'helpful', observation_id: 'later' }, true],
    ['context.feedback', { ...feedback, outcome: 'unnecessary' }, true],
  ] as const;
  for (const [operation, params, valid] of cases) {
    const descriptor = getAwarenessOperationDescriptor(operation)!;
    expect(z.fromJSONSchema(descriptor.inputSchema).safeParse(params).success, JSON.stringify(params)).toBe(valid);
    if (valid) expect(() => descriptor.validate(params)).not.toThrow();
    else expect(() => descriptor.validate(params)).toThrow();
  }
});
