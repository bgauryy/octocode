import assert from 'node:assert/strict';
import { afterEach, test, vi } from 'vitest';
import { Type } from 'typebox';
import type { ToolDefinition } from '../src/types.js';

type CdpHandler = () => void;

class BrowserAgentSession {
  targetInfo = { id: 'page-1', type: 'page', url: 'about:blank', title: 'Blank' };
  closed = false;
  calls: Array<{ method: string; params: Record<string, unknown> }> = [];

  async send(method: string, params: Record<string, unknown> = {}) {
    this.calls.push({ method, params });
    return {};
  }

  on(event: string, handler: CdpHandler): void {
    if (event === 'Page.loadEventFired') queueMicrotask(handler);
  }

  off(): void { /* no-op */ }

  close(): void {
    this.closed = true;
  }
}

async function registerBrowserAgentWithMocks(options: {
  connectImpl?: () => Promise<unknown>;
  securityLines?: string[];
  networkLines?: string[];
} = {}) {
  vi.resetModules();
  const session = new BrowserAgentSession();
  const cleanupConnection = vi.fn(async () => undefined);
  const connectToChrome = vi.fn(options.connectImpl ?? (async () => ({
    session,
    version: { Browser: 'Chrome/150.0.0.0' },
    screenshotDir: '/tmp/screens',
  })));
  const securityRecipe = vi.fn(async () => ({
    evidenceLines: options.securityLines ?? ['[FINDING] insecure cookie', '[ACTION] add HttpOnly'],
    details: { scheme: 'security' },
  }));
  const networkRecipe = vi.fn(async () => ({
    evidenceLines: options.networkLines ?? ['[FINDING] 500 from /api/orders'],
    details: { scheme: 'network' },
  }));

  vi.doMock('../src/chrome-debug.js', () => ({ connectToChrome, cleanupConnection }));
  vi.doMock('../src/chrome-debug-schemes.js', () => ({
    SCHEME_REGISTRY: {
      security: { recipe: securityRecipe },
      network: { recipe: networkRecipe },
      debug: { recipe: vi.fn(async () => ({ evidenceLines: [], details: {} })) },
      console: { recipe: vi.fn(async () => ({ evidenceLines: [], details: {} })) },
    },
  }));
  const spawnRpcAgent = vi.fn(() => ({ id: 'browser-worker', name: 'Browser Worker', policyWarnings: [] }));
  vi.doMock('../src/tools/agent-tools.js', async (importOriginal) => ({
    ...await importOriginal<typeof import('../src/tools/agent-tools.js')>(),
    spawnRpcAgent,
  }));

  const { registerUnifiedAgentTool } = await import('../src/tools/unified-agent-tool.js');
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (def: ToolDefinition) => tools.set(def.name, def) };
  registerUnifiedAgentTool(
    pi,
    Type,
    new Set<string>(),
    (targetPi, names, def) => {
      names.add(def.name);
      targetPi.registerTool?.(def);
    },
  );

  return {
    tool: tools.get('agent')!,
    spawnRpcAgent,
    session,
    connectToChrome,
    cleanupConnection,
    securityRecipe,
    networkRecipe,
  };
}

afterEach(() => {
  vi.doUnmock('../src/chrome-debug.js');
  vi.doUnmock('../src/chrome-debug-schemes.js');
  vi.doUnmock('../src/tools/agent-tools.js');
  vi.resetModules();
});

test('agent browser profile navigates, runs routed schemes, passes findings to worker, and cleans up Chrome', async () => {
  const {
    tool,
    session,
    connectToChrome,
    cleanupConnection,
    securityRecipe,
    networkRecipe,
    spawnRpcAgent,
  } = await registerBrowserAgentWithMocks();

  const result = await tool.execute('call-1', { queries: [{ reasoning: 'Inspect browser security', type: 'spawn', profile: 'browser',
    task: 'audit security cookies and network auth failures',
    url: 'https://example.com/app',
    port: 19333,
    model: 'sonnet:high',
    launch: true,
    headless: false,
    durationMs: 25,
    workspaceCwd: '/repo',
  }] });

  const connectCalls = connectToChrome.mock.calls as unknown as Array<[Record<string, unknown>]>;
  const cleanupCalls = cleanupConnection.mock.calls as unknown as Array<[unknown, boolean, boolean]>;
  assert.equal(connectCalls[0]![0].port, 19333);
  assert.equal(connectCalls[0]![0].launch, true);
  assert.equal(connectCalls[0]![0].headless, false);
  assert.ok(session.calls.some((call) => call.method === 'Page.enable'));
  assert.ok(session.calls.some((call) => call.method === 'Page.navigate' && call.params.url === 'https://example.com/app'));
  assert.equal(securityRecipe.mock.calls.length, 1);
  assert.equal(networkRecipe.mock.calls.length, 1);
  assert.equal(cleanupCalls[0]![1], false);
  assert.equal(cleanupCalls[0]![2], true);

  const spawned = spawnRpcAgent.mock.calls[0] as unknown as [Record<string, unknown>];
  const text = String(spawned[0].systemPrompt);
  assert.match(text, /\[AGENT\] navigated to https:\/\/example\.com\/app/);
  assert.match(text, /insecure cookie/);
  assert.match(text, /500 from \/api\/orders/);
  assert.deepEqual(spawned[0].tools, ['chromeDebug']);
  assert.equal(spawned[0].model, 'sonnet:high');
  assert.match(text, /Your ONLY browser tool is `chromeDebug`/);
  assert.match(text, /Network, Runtime, DOM, DOMDebugger, Fetch/);
  assert.match(tool.renderCall!({ queries: [{ type: 'spawn', profile: 'browser', task: 'Inspect app' }] }).render(80)[0]!, /agent\(spawn profile:browser\)/);
  assert.match(tool.renderResult!(result, { expanded: false }).render(160)[0]!, /SPAWNED/);
});

test('agent browser profile passes connection errors to its worker', async () => {
  const { tool, cleanupConnection, spawnRpcAgent } = await registerBrowserAgentWithMocks({
    connectImpl: async () => {
      throw new Error('Chrome down');
    },
  });

  await tool.execute('call-1', { queries: [{ reasoning: 'Inspect console errors', type: 'spawn', profile: 'browser',
    task: 'inspect console errors',
    runNow: true,
  }] });

  const spawned = spawnRpcAgent.mock.calls[0] as unknown as [Record<string, unknown>];
  const text = String(spawned[0].systemPrompt);
  assert.match(text, /\[AGENT\] connect error: Chrome down/);
  assert.match(text, /Runtime, Log/);
  assert.equal(cleanupConnection.mock.calls.length, 0);
});
