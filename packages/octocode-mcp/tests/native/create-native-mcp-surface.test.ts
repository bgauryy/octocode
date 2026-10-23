// createNativeMcp catalog surface: per-request budget ownership, the MCP
// availability filter, instruction size, and cancellation forwarding.
import { afterEach, describe, expect, it, vi } from 'vitest';
import { buildMcpInstructions } from '@octocodeai/config/mcp';
import {
  getNativeContractFingerprint,
  isCliOnlyTool,
  TOOL_POLICIES,
} from '@octocodeai/config/schema';
import { createNativeMcp } from '../../src/native/index.js';
import {
  bindingFor,
  connectClient,
  FakeRuntime,
  tool,
} from './native-mcp-fixture.js';

afterEach(() => {
  FakeRuntime.last = undefined;
});

describe('createNativeMcp registration + execution', () => {
  it('leaves the per-request execution budget to native', async () => {
    const instance = await createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true)],
      })),
    });

    expect(FakeRuntime.last?.options).toEqual({ surface: 'mcp', env: {} });

    await instance.close();
  });

  it('registers only the tools native reports available on MCP', async () => {
    const cliOnly = Object.keys(TOOL_POLICIES).filter(isCliOnlyTool);
    expect(cliOnly.length).toBeGreaterThan(0);
    // Native resolves CLI-only tools as unavailable on the MCP surface.
    const instance = await createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [
          tool('localFetch', true),
          ...cliOnly.map(name => tool(name, false)),
        ],
      })),
    });
    const client = await connectClient(instance, 'cli-only');
    expect((await client.listTools()).tools.map(t => t.name)).toEqual([
      'localFetch',
    ]);
    await client.close();
    await instance.close();
  });

  // Hosts truncate server instructions near 2 KB; the grammar inventory is
  // served by `octocode schema`, not the MCP instructions.
  it('keeps the grammar inventory out of the size-bounded MCP instructions', async () => {
    const instance = await createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('astSearch', true)],
        grammarCapabilities: [
          {
            language: 'Rust',
            languageId: 'rust',
            selectorAliases: [],
            extensions: ['rs'],
            structuralSearch: true,
            signatureOutline: true,
            graphFacts: true,
          },
        ],
      })),
    });
    const client = await connectClient(instance, 'grammar-inventory');
    expect(client.getInstructions()).not.toContain('Runtime grammar inventory');
    expect(client.getInstructions()).toBe(
      buildMcpInstructions({ availableTools: ['astSearch'] })
    );
    expect(client.getInstructions()!.length).toBeLessThanOrEqual(2_000);
    await client.close();
    await instance.close();
  });
});

describe('createNativeMcp cancellation', () => {
  it('forwards client cancellation (notifications/cancelled) with the admitted native request id', async () => {
    const instance = await createNativeMcp({
      env: {},
      binding: bindingFor(() => ({
        fingerprint: getNativeContractFingerprint(),
        tools: [tool('localFetch', true)],
      })),
    });
    const client = await connectClient(instance, 'cancel');
    const runtime = FakeRuntime.last!;
    runtime.hang = true;
    const controller = new AbortController();
    const call = client
      .callTool(
        {
          name: 'localFetch',
          arguments: {
            queries: [
              { mainGoal: 'test goal', reasoning: 'cancel test', path: '.' },
            ],
          },
        },
        undefined,
        { signal: controller.signal }
      )
      .catch((error: unknown) => error);
    await vi.waitFor(() => expect(runtime.executions).toHaveLength(1));
    controller.abort();
    await call;
    await vi.waitFor(() => expect(runtime.cancelled).toHaveLength(1));
    expect(runtime.cancelled[0]).toBe(runtime.executions[0]!.requestId);
    await client.close();
    await instance.close();
  });
});
