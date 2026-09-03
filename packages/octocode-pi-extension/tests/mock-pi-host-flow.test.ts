import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import { createPiFlowHarness } from '@octocodeai/agent-testing';
import extension from '../src/index.js';
import type { PiInstance } from '../src/types.js';

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

test('boots the complete extension on the reusable Pi host and drives its public lifecycle', async () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-mock-pi-'));
  roots.push(cwd);
  const flow = createPiFlowHarness({ cwd, sessionId: 'full-flow' });

  await extension(flow.pi as unknown as PiInstance);

  assert.ok(flow.tools.has('plan'));
  assert.ok(flow.tools.has('askUser'));
  assert.ok(flow.tools.has('bash'));
  assert.ok(flow.commands.size > 0);
  assert.ok(flow.handlers.has('session_start'));
  assert.ok(flow.handlers.has('before_agent_start'));

  await flow.emit('session_start', { reason: 'startup' });
  await flow.emit('before_agent_start', { prompt: 'make a plan' });
  await flow.restart();

  flow.assertSequence([
    'tool.registered',
    'command.registered',
    'event.emitted',
    'event.handled',
    'session.restarted',
  ]);
});
