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

  const { registerBrowserAgentTool } = await import('../src/tools/browser-agent-tool.js');
  const tools = new Map<string, ToolDefinition>();
  const pi = { registerTool: (def: ToolDefinition) => tools.set(def.name, def) };
  registerBrowserAgentTool(
    pi,
    Type,
    new Set<string>(),
    (targetPi, names, def) => {
      names.add(def.name);
      targetPi.registerTool?.(def);
    },
  );

  return {
    tool: tools.get('browserAgent')!,
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
  vi.resetModules();
});

test('browserAgent runNow navigates, runs routed schemes, builds spawn config, and cleans up launched Chrome', async () => {
  const {
    tool,
    session,
    connectToChrome,
    cleanupConnection,
    securityRecipe,
    networkRecipe,
  } = await registerBrowserAgentWithMocks();

  const result = await tool.execute('call-1', {
    task: 'audit security cookies and network auth failures',
    url: 'https://example.com/app',
    port: 19333,
    model: 'sonnet:high',
    launch: true,
    headless: false,
    durationMs: 25,
    workspaceCwd: '/repo',
  });

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

  const text = result.content?.[0]?.text ?? '';
  assert.match(text, /schemes run: security, network/);
  assert.match(text, /\[AGENT\] navigated to https:\/\/example\.com\/app/);
  assert.match(text, /findings: 2  actions: 1/);
  assert.match(text, /tools: chromeDebug/);
  assert.match(text, /model: sonnet:high/);
  assert.match(text, /Your ONLY browser tool is `chromeDebug`/);
  assert.match(text, /Network, Runtime, DOM, DOMDebugger, Fetch/);
  assert.match(tool.description, /pi -ne --list-models/);
  assert.match(tool.description, /hardcoded config paths/);
  assert.match(
    String((tool.parameters.properties as Record<string, { description?: string }>).model?.description ?? ''),
    /pi -ne --list-models/,
  );

  assert.match(tool.renderCall!({ task: 'x'.repeat(90), url: 'https://example.com' }).render(80)[0]!, /browserAgent/);
  assert.match(tool.renderResult!(result, { expanded: false }).render(160)[0]!, /5 findings/);
});

test('browserAgent records connection errors and still returns a usable spawn config', async () => {
  const { tool, cleanupConnection } = await registerBrowserAgentWithMocks({
    connectImpl: async () => {
      throw new Error('Chrome down');
    },
  });

  const result = await tool.execute('call-1', {
    task: 'inspect console errors',
    runNow: true,
  });

  const text = result.content?.[0]?.text ?? '';
  assert.match(text, /\[AGENT\] connect error: Chrome down/);
  assert.match(text, /schemes run: \(none\)/);
  assert.match(text, /Runtime, Log/);
  assert.equal(cleanupConnection.mock.calls.length, 0);
});
