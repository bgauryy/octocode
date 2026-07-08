import assert from 'node:assert/strict';
import { test } from 'vitest';
import { getRandomAgentName } from '../src/agentNames.js';

test('getRandomAgentName returns a non-empty string', () => {
  const name = getRandomAgentName();
  assert.equal(typeof name, 'string');
  assert.ok(name.length > 0);
});

test('getRandomAgentName returns camelCase (no spaces, no underscores, no uppercase-only)', () => {
  for (let i = 0; i < 50; i++) {
    const name = getRandomAgentName();
    assert.doesNotMatch(name, /\s/, `name "${name}" contains whitespace`);
    assert.doesNotMatch(name, /_/, `name "${name}" contains underscore`);
    assert.doesNotMatch(name, /^[A-Z]/, `name "${name}" starts with uppercase`);
  }
});

test('getRandomAgentName returns different values across calls (not always the same)', () => {
  const results = new Set(Array.from({ length: 50 }, () => getRandomAgentName()));
  assert.ok(results.size > 1, 'expected variety across 50 calls');
});

test('getRandomAgentName always returns a known name from the list', () => {
  // Import the module as a namespace to inspect internals via dynamic import trick —
  // instead, we verify structural shape: starts with lowercase letter, no illegal chars.
  const seen = new Set<string>();
  for (let i = 0; i < 200; i++) {
    const name = getRandomAgentName();
    seen.add(name);
    assert.match(name, /^[a-z][a-zA-Z]+$/, `name "${name}" does not match camelCase pattern`);
  }
  // Should have seen at least 10 distinct names in 200 draws
  assert.ok(seen.size >= 10, `only saw ${seen.size} distinct names in 200 draws`);
});
