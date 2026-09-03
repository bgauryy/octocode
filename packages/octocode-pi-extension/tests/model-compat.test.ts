import assert from 'node:assert/strict';
import { test } from 'vitest';
import { ensureAdaptiveThinkingCompatibility } from '../src/model-compat.js';

test('enables adaptive thinking for custom Anthropic-compatible Claude models that require it', () => {
  for (const id of ['claude-sonnet-4-6', 'claude-opus-4-6', 'claude-opus-4-8', 'claude-sonnet-5']) {
    const model = { id, api: 'anthropic-messages', compat: { sendSessionAffinityHeaders: true } };

    assert.equal(ensureAdaptiveThinkingCompatibility(model), true, id);
    assert.deepEqual(model.compat, {
      sendSessionAffinityHeaders: true,
      forceAdaptiveThinking: true,
    });
  }
});

test('preserves explicit compatibility choices and non-Anthropic transports', () => {
  const explicitlyDisabled = {
    id: 'claude-sonnet-4-6',
    api: 'anthropic-messages',
    compat: { forceAdaptiveThinking: false },
  };
  const bedrock = { id: 'claude-sonnet-4-6', api: 'bedrock-converse-stream', compat: {} };
  const olderClaude = { id: 'claude-sonnet-4-5', api: 'anthropic-messages', compat: {} };

  assert.equal(ensureAdaptiveThinkingCompatibility(explicitlyDisabled), false);
  assert.equal(explicitlyDisabled.compat.forceAdaptiveThinking, false);
  assert.equal(ensureAdaptiveThinkingCompatibility(bedrock), false);
  assert.equal(ensureAdaptiveThinkingCompatibility(olderClaude), false);
});
