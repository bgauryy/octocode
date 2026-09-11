import assert from 'node:assert/strict';
import { test } from 'vitest';
import { classifyEvidenceAuthority } from '../src/tools/evidence-authority.js';

test('matching fresh evidence remains live and authoritative', () => {
  assert.deepEqual(classifyEvidenceAuthority({
    observedAt: 9_000,
    now: 10_000,
    staleAfterMs: 2_000,
    observedGeneration: 3,
    currentGeneration: 3,
    observedFingerprint: 'same',
    currentFingerprint: 'same',
  }), {
    authority: 'live',
    ambient: true,
    preserveCoordinates: true,
    mayBlock: true,
    recheckRequired: false,
  });
});

test('changed, unverifiable, or old evidence is demoted with an explicit recheck', () => {
  for (const input of [
    { observedGeneration: 2, currentGeneration: 3 },
    { observedFingerprint: 'old', currentFingerprint: 'new' },
    { observedAt: 1, now: 10, staleAfterMs: 5 },
  ]) {
    const result = classifyEvidenceAuthority({ observedAt: 1, now: 1, ...input });
    assert.equal(result.authority, 'demoted');
    assert.equal(result.ambient, true);
    assert.equal(result.preserveCoordinates, false);
    assert.equal(result.mayBlock, false);
    assert.equal(result.recheckRequired, true);
    assert.ok(result.reason);
  }
});

test('a missing evidence target retires from ambient UI without pretending it never existed', () => {
  assert.deepEqual(classifyEvidenceAuthority({
    observedAt: 1,
    now: 2,
    sourceExists: false,
  }), {
    authority: 'retired',
    ambient: false,
    preserveCoordinates: false,
    mayBlock: false,
    recheckRequired: false,
    reason: 'evidence target is no longer available',
  });
});
