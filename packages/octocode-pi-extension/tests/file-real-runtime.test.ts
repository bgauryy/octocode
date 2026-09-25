import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, expect, it, vi } from 'vitest';
import {
  createAgentSession,
  DefaultResourceLoader,
  SessionManager,
  SettingsManager,
  type ExtensionFactory,
} from '@earendil-works/pi-coding-agent';
import builtOctocodeExtension from '@octocodeai/pi-extension';
import { allowLocalFixtureProcesses } from '../../../test-utils/external-effects-guard.js';

const PROVIDER = 'octocode-file-runtime-fixture';
const API = 'octocode-file-runtime-fixture-api';
const MODEL = 'deterministic-local';
type Block = { type: 'text'; text: string }
  | { type: 'toolCall'; id: string; name: string; arguments: Record<string, unknown> };

// Implements the SDK stream protocol locally; no provider transport is used.
function localStream(block: Block) {
  const message = {
    role: 'assistant', content: [block], api: API, provider: PROVIDER, model: MODEL,
    usage: { input: 32, output: 8, cacheRead: 0, cacheWrite: 0, totalTokens: 40,
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
    stopReason: block.type === 'toolCall' ? 'toolUse' : 'stop', timestamp: Date.now(),
  };
  const partial = { ...message, content: [], stopReason: 'pending' };
  return {
    async *[Symbol.asyncIterator]() {
      yield { type: 'start', partial };
      if (block.type === 'toolCall') {
        yield { type: 'toolcall_start', contentIndex: 0, partial };
        yield { type: 'toolcall_delta', contentIndex: 0, delta: JSON.stringify(block.arguments), partial };
        yield { type: 'toolcall_end', contentIndex: 0, toolCall: block, partial };
      } else {
        yield { type: 'text_start', contentIndex: 0, partial };
        yield { type: 'text_delta', contentIndex: 0, delta: block.text, partial };
        yield { type: 'text_end', contentIndex: 0, content: block.text, partial };
      }
      yield { type: 'done', reason: message.stopReason, message };
    },
    result: async () => message,
  };
}

let root: string | undefined;
let restoreProcesses: (() => void) | undefined;
afterEach(() => {
  restoreProcesses?.();
  restoreProcesses = undefined;
  vi.unstubAllEnvs();
  if (root) fs.rmSync(root, { recursive: true, force: true });
  root = undefined;
});

it('built file tool preserves exact bytes and reports preflight failures to the real next model turn', async () => {
  restoreProcesses = allowLocalFixtureProcesses();
  root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-file-real-runtime-')));
  const workspace = path.join(root, 'workspace');
  const agentDir = path.join(root, 'agent');
  fs.mkdirSync(workspace);
  fs.mkdirSync(agentDir);
  vi.stubEnv('OCTOCODE_HOME', path.join(root, 'octocode-home'));
  vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
  vi.stubEnv('OCTOCODE_AGENT_ID', 'pi:file-real-runtime-test');

  const original = '\uFEFFalpha\r\nbeta\ngamma\r\n';
  const expected = '\uFEFFalpha\r\nBETA\ngamma\r\n';
  const toolCall = (id: string, queries: Array<Record<string, unknown>>): Block => ({
    type: 'toolCall', id, name: 'file', arguments: { queries },
  });
  const scripted: Block[] = [
    toolCall('file-write', [{ type: 'write', reasoning: 'Create the exact-byte fixture.', path: 'mixed.txt', content: original }]),
    toolCall('file-edit', [{ type: 'edit', reasoning: 'Use the read state recorded by write.', path: 'mixed.txt',
      requireRecentRead: true, edits: [{ oldText: 'beta', newText: 'BETA' }] }]),
    toolCall('file-invalid-batch', [
      { type: 'write', reasoning: 'Must remain unwritten if preflight fails.', path: 'must-not-exist.txt', content: 'unexpected' },
      { type: 'write', reasoning: 'Exercise operation-specific preflight.', path: 'malformed.txt', content: 'unexpected', force: true },
    ]),
    { type: 'text', text: 'Observed the file mutation receipts.' },
  ];
  const contexts: string[] = [];
  const fixture: ExtensionFactory = pi => {
    pi.registerProvider(PROVIDER, {
      name: 'Local file runtime fixture', api: API, baseUrl: 'http://127.0.0.1:0', apiKey: 'local-fixture',
      models: [{ id: MODEL, name: 'Local file runtime fixture', api: API, reasoning: false, input: ['text'],
        cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 65_536, maxTokens: 1024 }],
      streamSimple: (_model, context) => {
        contexts.push(JSON.stringify(context));
        return localStream(scripted.shift() ?? { type: 'text', text: 'done' }) as never;
      },
    });
  };
  const settings = SettingsManager.inMemory({ compaction: { enabled: false }, retry: { enabled: false } });
  const loader = new DefaultResourceLoader({
    cwd: workspace, agentDir, settingsManager: settings,
    extensionFactories: [
      { name: 'octocode-product', factory: builtOctocodeExtension as never },
      { name: 'local-file-fixture', factory: fixture },
    ],
    noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true,
  });
  await loader.reload();
  const { session } = await createAgentSession({
    cwd: workspace, agentDir, tools: ['file'], resourceLoader: loader,
    sessionManager: SessionManager.create(workspace, path.join(root, 'sessions')), settingsManager: settings,
  });
  try {
    await session.bindExtensions({ mode: 'json', shutdownHandler() {} });
    const model = session.modelRuntime.getModel(PROVIDER, MODEL);
    expect(model).toBeDefined();
    await session.setModel(model!);
    await session.prompt('Run the authorized local file mutation fixtures.', { expandPromptTemplates: false });
    await session.waitForIdle();

    expect(contexts).toHaveLength(4);
    const finalContext = JSON.parse(contexts[3]!) as {
      messages: Array<{ role: string; toolCallId?: string; isError?: boolean; content?: unknown }>;
    };
    const receipts = finalContext.messages.filter(message => message.role === 'toolResult');
    for (const id of ['file-write', 'file-edit']) {
      const receipt = receipts.find(message => message.toolCallId === id);
      expect(receipt).toBeDefined();
      expect(receipt?.isError, JSON.stringify(receipt?.content)).not.toBe(true);
    }
    const failure = receipts.find(message => message.toolCallId === 'file-invalid-batch');
    expect(failure?.isError).toBe(true);
    expect(JSON.stringify(failure?.content)).toMatch(/preflight|does not accept force/i);
    expect(fs.readFileSync(path.join(workspace, 'mixed.txt'))).toEqual(Buffer.from(expected, 'utf8'));
    expect(fs.existsSync(path.join(workspace, 'must-not-exist.txt'))).toBe(false);
    expect(fs.existsSync(path.join(workspace, 'malformed.txt'))).toBe(false);
  } finally {
    session.dispose();
    await settings.flush();
  }
}, 30_000);
