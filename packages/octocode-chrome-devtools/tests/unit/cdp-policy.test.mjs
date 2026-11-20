// The one CDP safety policy: the table is the contract; everything else is free.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  CDP_POLICY_TABLE,
  classify,
  policyRefusal,
  blockedRefusal,
  ackRefusal,
} from '../../dist/engine/cdp-policy.mjs';

test('every table entry classifies as declared and has a reason', () => {
  assert.ok(CDP_POLICY_TABLE.length > 0);
  const seen = new Set();
  for (const entry of CDP_POLICY_TABLE) {
    assert.ok(!seen.has(entry.method), `duplicate ${entry.method}`);
    seen.add(entry.method);
    assert.match(entry.method, /^[A-Z][A-Za-z]*\.[A-Za-z]+$/);
    assert.equal(classify(entry.method), entry.policy);
    assert.ok(['blocked', 'needsAck'].includes(entry.policy));
    assert.ok(entry.reason.length > 0, `${entry.method} has no reason`);
    if (entry.policy === 'blocked')
      assert.ok(entry.alternative, `${entry.method} has no alternative`);
  }
});

test('unlisted methods are free and never refused', () => {
  for (const method of ['DOM.querySelector', 'Runtime.evaluate', 'Nope.nope']) {
    assert.equal(classify(method), 'free');
    assert.equal(policyRefusal(method, false), undefined);
  }
});

test('blocked methods are refused even when acknowledged; needsAck only without acknowledgement', () => {
  for (const { method, policy } of CDP_POLICY_TABLE) {
    if (policy === 'blocked') {
      assert.match(policyRefusal(method, true), /blocked/);
      assert.ok(blockedRefusal(method));
      assert.equal(ackRefusal(method, true), undefined);
    } else {
      assert.match(policyRefusal(method, false), /user's permission/);
      assert.match(policyRefusal(method, undefined), /acknowledge: true/);
      assert.equal(policyRefusal(method, true), undefined);
      assert.equal(blockedRefusal(method), undefined);
    }
  }
});

test('the docs mention every policy entry', () => {
  const docs = readFileSync(
    join(import.meta.dirname, '../../docs/cdp-protocol.md'),
    'utf8'
  );
  for (const { method } of CDP_POLICY_TABLE)
    assert.ok(docs.includes(method), `${method} missing from docs`);
});
