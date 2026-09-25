import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { expect, it, vi } from 'vitest';
import { createPiFlowHarness } from '@octocodeai/agent-testing';
import { allowLocalFixtureProcesses } from '../../../test-utils/external-effects-guard.js';
import { getAssetPaths } from '../src/assets.js';
import { registerCommunicationRuntime } from '../src/tools/communication-runtime.js';
import type { PiInstance } from '../src/types.js';

it('real native leases block copy destinations and move/delete sources while permitting reads', async () => {
  const restore = allowLocalFixtureProcesses();
  const workspace = realpathSync(mkdtempSync(path.join(tmpdir(), 'pi-native-lease-')));
  vi.stubEnv('OCTOCODE_HOME', path.join(workspace, 'home'));
  vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
  const flow = createPiFlowHarness({ cwd: workspace, sessionId: 'lease-gate' });
  const ledger: ReturnType<typeof flow.context.sessionManager.getEntries> = [];
  const recordMessage = flow.pi.sendMessage.bind(flow.pi);
  flow.pi.sendMessage = (message, options) => {
    recordMessage(message, options);
    ledger.push({ type: 'custom_message', ...(message as Record<string, unknown>) } as typeof ledger[number]);
  };
  flow.context.sessionManager.getEntries = () => ledger;
  const binary = path.join(getAssetPaths().skillsDir, 'octocode-agents-communication/scripts/agents-communication');
  mkdirSync(path.join(workspace, 'destination')); mkdirSync(path.join(workspace, 'free'));
  writeFileSync(path.join(workspace, 'source.txt'), 'original');
  registerCommunicationRuntime(flow.pi as unknown as PiInstance);
  try {
    await flow.emit('session_start');
    const identity = flow.context.sessionManager.getEntries().find((entry: unknown) =>
      (entry as { customType?: string }).customType === 'octocode-identity') as unknown as { details: { database: string } };
    const call = (command: string, input: unknown, session?: string) => JSON.parse(execFileSync(binary,
      [command, JSON.stringify(input), '--workspace', workspace, '--database', identity.details.database,
        ...(session ? ['--session', session] : [])], { encoding: 'utf8' })) as { id: string };
    const peer = call('join', { vendor: 'raw', name: 'lease-owner' }).id;
    call('lock', { path: 'destination', kind: 'tree', reasoning: 'Changing output structure' }, peer);
    call('lock', { path: 'source.txt', reasoning: 'Editing source' }, peer);
    const gate = async (toolName: string, input: unknown) => (await flow.emit('tool_call', { toolName, input }))
      .find(result => (result as { block?: boolean } | undefined)?.block);
    for (const command of ['cp -t destination source.txt', 'cp --target-directory=destination source.txt', 'mv source.txt free/moved.txt', 'rm source.txt']) {
      expect(await gate('bash', { queries: [{ command }] }), command).toMatchObject({ block: true, reason: expect.stringContaining('lease conflict') });
    }
    expect(await gate('bash', { queries: [{ command: 'cp -t free source.txt' }] })).toBeUndefined();
    expect(await gate('read', { path: 'source.txt' })).toBeUndefined();
    expect(await gate('file', { queries: [{ type: 'delete', path: 'source.txt' }] })).toMatchObject({ block: true });
  } finally {
    await flow.emit('session_shutdown');
    vi.unstubAllEnvs(); restore(); rmSync(workspace, { recursive: true, force: true });
  }
}, 15000);
