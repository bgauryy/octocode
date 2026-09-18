import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, expect, it, vi } from 'vitest';
import { createAgentSession, DefaultResourceLoader, SessionManager, SettingsManager, type ExtensionFactory } from '@earendil-works/pi-coding-agent';
import octocodeExtension from '../src/index.js';

afterEach(() => vi.unstubAllEnvs());

it.each(['before-abort', 'before-throw', 'armed-agent-start', 'product-assembly', 'model-window'] as const)('honors the Pi prompt preparation cancellation boundary: %s', async mode => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-preflight-abort-'));
  const agentDir = path.join(root, 'agent');
  fs.mkdirSync(agentDir);
  vi.stubEnv('OCTOCODE_HOME', path.join(root, 'octocode-home'));
  vi.stubEnv('OCTOCODE_AGENT_DIR', agentDir);
  vi.stubEnv('OCTOCODE_PI_SUBAGENT', '0');
  let providerCalls = 0;
  let preflightSignal: boolean | undefined;
  let startSignal: boolean | undefined;
  let armed = false;
  let rejectAssembly = true;
  let deactivateTools: (() => void) | undefined;
  const extension: ExtensionFactory = async pi => {
    deactivateTools = () => pi.setActiveTools([]);
    pi.on('before_agent_start', (_event, ctx) => {
      preflightSignal = ctx.signal?.aborted;
      if (!rejectAssembly) { armed = false; return undefined; }
      if (mode === 'before-abort') ctx.abort();
      if (mode === 'before-throw') throw new Error('Rejected assembly probe');
      if (mode === 'armed-agent-start') armed = true;
      if (mode === 'product-assembly') return { systemPrompt: 'oversized prompt '.repeat(40_000) };
      if (mode === 'model-window') return { systemPrompt: 'bounded prompt '.repeat(2_000) };
      return undefined;
    });
    pi.on('agent_start', (_event, ctx) => {
      startSignal = ctx.signal?.aborted;
      if (armed) ctx.abort();
    });
    pi.registerProvider('preflight-probe', {
      name: 'Preflight probe', api: 'preflight-probe-api', baseUrl: 'http://127.0.0.1:0', apiKey: 'local-fixture',
      models: ['probe', 'small'].map(id => ({ id, name: id, api: 'preflight-probe-api', reasoning: false, input: ['text'], cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: id === 'small' ? 4096 : 131072, maxTokens: 1024 })),
      streamSimple: () => {
        providerCalls++;
        const message = { role: 'assistant', content: [{ type: 'text', text: 'local result' }], api: 'preflight-probe-api', provider: 'preflight-probe', model: 'probe', usage: { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } }, stopReason: 'stop', timestamp: Date.now() };
        return { async *[Symbol.asyncIterator]() { yield { type: 'start', partial: message }; yield { type: 'done', reason: 'stop', message }; }, result: async () => message } as never;
      },
    });
    if (mode === 'product-assembly' || mode === 'model-window') await octocodeExtension(pi as never);
  };
  const settings = SettingsManager.inMemory({ compaction: { enabled: false }, retry: { enabled: false } });
  const loader = new DefaultResourceLoader({ cwd: root, agentDir, settingsManager: settings, extensionFactories: [{ name: 'preflight-probe', factory: extension }], noSkills: true, noPromptTemplates: true, noThemes: true, noContextFiles: true });
  await loader.reload();
  const { session } = await createAgentSession({ cwd: root, agentDir, tools: [], resourceLoader: loader, sessionManager: SessionManager.create(root, path.join(root, 'sessions')), settingsManager: settings });
  try {
    await session.bindExtensions({ mode: 'json', shutdownHandler() {} });
    deactivateTools?.();
    await session.setModel(session.modelRuntime.getModel('preflight-probe', mode === 'model-window' ? 'small' : 'probe')!);
    await session.prompt('Probe rejected prompt assembly.', { expandPromptTemplates: false });
    await session.waitForIdle();
    expect(preflightSignal).toBeUndefined();
    expect(startSignal).toBe(false);
    const abortsBeforeProvider = mode === 'armed-agent-start' || mode === 'product-assembly' || mode === 'model-window';
    expect(providerCalls).toBe(abortsBeforeProvider ? 0 : 1);
    rejectAssembly = false;
    if (mode === 'model-window') await session.setModel(session.modelRuntime.getModel('preflight-probe', 'probe')!);
    await session.prompt('Retry after repairing the rejected assembly.', { expandPromptTemplates: false });
    await session.waitForIdle();
    expect(providerCalls).toBe(abortsBeforeProvider ? 1 : 2);
  } finally {
    session.dispose();
    await settings.flush();
    fs.rmSync(root, { recursive: true, force: true });
  }
}, 30_000);
