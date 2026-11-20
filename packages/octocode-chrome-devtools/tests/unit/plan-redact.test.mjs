import test from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';

const { redactPlanForEvidence } = await import(
  join(import.meta.dirname, '../../dist/engine/cdp-checks/plan-redact.mjs')
);

test('typed text never survives in the evidence copy, and the live plan is untouched', () => {
  const plan = {
    steps: [
      {
        op: 'act',
        action: 'fill',
        role: 'textbox',
        name: 'Password',
        value: 'hunter2-secret',
      },
      { op: 'act', action: 'type', selector: '#pin', value: '4242' },
      { op: 'act', action: 'click', role: 'button', name: 'Sign in' },
      { op: 'act', action: 'press', key: 'Enter' },
    ],
  };
  const saved = redactPlanForEvidence(plan);
  assert.doesNotMatch(JSON.stringify(saved), /hunter2-secret|4242/);
  assert.equal(saved.steps[0].value, '[redacted 14 chars]');
  assert.equal(saved.steps[0].name, 'Password');
  assert.deepEqual(saved.steps.slice(2), plan.steps.slice(2));
  assert.equal(plan.steps[0].value, 'hunter2-secret');
});

test('scrubTyped removes a typed value from any string at any depth, leaving other text', async () => {
  const { scrubTyped, typedValues } = await import(
    join(import.meta.dirname, '../../dist/engine/cdp-checks/plan-redact.mjs')
  );
  const secrets = typedValues([
    { op: 'act', action: 'fill', value: 'pw-123456' },
    { op: 'act', action: 'select', value: 'Blue' },
  ]);
  assert.deepEqual(secrets, ['pw-123456']);
  const scrubbed = scrubTyped(
    { state: { value: 'pw-123456' }, list: ['x pw-123456 y', 'Blue'] },
    secrets
  );
  assert.equal(scrubbed.state.value, '[redacted 9 chars]');
  assert.deepEqual(scrubbed.list, ['x [redacted 9 chars] y', 'Blue']);
});
