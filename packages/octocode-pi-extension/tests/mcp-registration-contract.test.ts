import { afterEach, expect, test, vi } from 'vitest';
import { validateToolArguments } from '@earendil-works/pi-ai';
import { buildServerEnv } from '../src/tools/mcp/config.js';
import { mcpGatewayItemSchema } from '../src/tools/mcp/gateway-contract.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { buildQueryEnvelopeSchema } from '../src/tools/query-envelope.js';
import type { ToolDefinition } from '../src/types.js';

afterEach(() => vi.unstubAllEnvs());

test('tool guidelines remain attributed when Pi combines them into a flat prompt section', () => {
  const tools: ToolDefinition[] = [];
  const guidelines = ['Set mode only for creation.'];
  for (const name of ['first', 'second']) {
    registerUniqueTool({ registerTool: tool => { tools.push(tool); } }, new Set(), {
      name, label: name, description: name, parameters: {}, promptGuidelines: guidelines,
      execute: async () => ({ content: [] }),
    });
  }
  expect(tools.flatMap(tool => tool.promptGuidelines)).toEqual([
    'first: Set mode only for creation.', 'second: Set mode only for creation.',
  ]);
  expect(guidelines).toEqual(['Set mode only for creation.']);
});

test('built-in MCP receives supported GitHub credentials without exposing them to arbitrary servers', () => {
  vi.stubEnv('GITHUB_PERSONAL_ACCESS_TOKEN', 'fixture-token');
  const config = { command: 'fixture-server' };
  expect(buildServerEnv('octocode', config).GITHUB_PERSONAL_ACCESS_TOKEN).toBe('fixture-token');
  expect(buildServerEnv('third-party', config).GITHUB_PERSONAL_ACCESS_TOKEN).toBeUndefined();
});

test('registered MCP envelope rejects misplaced target fields and preserves nested arguments', () => {
  const schema = buildQueryEnvelopeSchema(mcpGatewayItemSchema());
  const query = { reasoning: 'fetch source', action: 'call', server: 'octocode', tool: 'localFetch', arguments: { queries: [{ path: '/fixture.ts' }] } };
  const validate = (value: unknown) => validateToolArguments(
    { name: 'MCPTool', description: 'MCP gateway', parameters: schema },
    { type: 'toolCall', id: 'registration-fixture', name: 'MCPTool', arguments: value as Record<string, unknown> },
  );
  expect(validate({ queries: [query] })).toEqual({ queries: [query] });
  expect(() => validate({ queries: [{ ...query, path: '/misplaced.ts' }] })).toThrow();
});

test('registration composes tool-owned input preparation with the shared envelope', () => {
  let registered: ToolDefinition | undefined;
  const prepareArguments = vi.fn(() => ({ queries: [{ operation: 'read' }] }));
  const parameters = { type: 'object', additionalProperties: false };
  registerUniqueTool({ registerTool: tool => { registered = tool; } }, new Set(), {
    name: 'fixture', label: 'Fixture', description: 'Fixture', parameters, prepareArguments,
    execute: async () => ({ content: [] }),
  });
  const input = { customInput: true };
  const prepared = registered!.prepareArguments!(input) as { queries: Array<Record<string, unknown>> };
  expect(prepareArguments).toHaveBeenCalledExactlyOnceWith(input);
  expect(prepared.queries[0]).toMatchObject({ operation: 'read', reasoning: expect.any(String) });
  expect(registered!.parameters).toBe(parameters);
});
