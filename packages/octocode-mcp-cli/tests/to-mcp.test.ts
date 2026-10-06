import { describe, expect, it } from 'vitest';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport, McpServer } from '@modelcontextprotocol/server';
import { z } from 'zod';
import { cliView } from '../src/run.js';
import { defineCli, defineCommand, type CliCommand } from '../src/spec.js';
import { cliToMcp, jsonSchemaStandard, mcpServerOptions, registerOn, type ToolRegistrar } from '../src/to-mcp.js';

const schema = z.object({ query: z.string().describe('Search text') });
const inputSchema = {
  type: 'object',
  properties: { note: { type: 'string', description: 'A note' } },
  required: ['note'],
};

const spec = defineCli({
  name: 'issues',
  instructions: 'Read the index.',
  commands: [
    defineCommand({
      name: 'search',
      description: 'Search issues',
      schema,
      run: input => `found ${String(input.query)}`,
    }),
    defineCommand({
      name: 'note',
      title: 'Add note',
      description: 'Add a note',
      inputSchema,
      outputSchema: { type: 'object', properties: { note: { type: 'string' } }, required: ['note'] },
      annotations: { readOnlyHint: false },
      mcpName: 'notes.add',
      run: input => ({ note: input.note }),
    }),
    defineCommand({
      name: 'quiet',
      description: 'Quiet',
      schema: z.object({}),
      run: () => undefined,
    }),
  ],
});

describe('cliToMcp', () => {
  it('keeps instructions, Zod JSON Schema, and the original imported schema', () => {
    const exported = cliToMcp(spec);
    expect(exported.instructions).toBe(spec.instructions);
    expect(exported.tools.map(tool => tool.description)).toEqual(['Search issues', 'Add a note', 'Quiet']);
    expect(exported.tools[0]?.inputSchema).toEqual(z.toJSONSchema(schema));
    expect(exported.tools[1]?.inputSchema).toBe(inputSchema);
    expect(exported.tools[1]?.name).toBe('notes.add');
    expect(exported.tools[1]).toMatchObject({
      title: 'Add note',
      outputSchema: { type: 'object', properties: { note: { type: 'string' } }, required: ['note'] },
      annotations: { readOnlyHint: false },
    });
    expect(exported.tools[0]?.description).toBe('Search issues');
    const zodCommand = spec.commands[0] as CliCommand;
    expect(() => cliToMcp({ ...spec, commands: [{ ...zodCommand, schema: undefined }] })).toThrow(/schema/);
    const mcpCommand = spec.commands[1] as CliCommand;
    expect(() => cliToMcp({ ...spec, commands: [{ ...mcpCommand, inputSchema: undefined }] })).toThrow(/inputSchema/);
  });

  it('registers Zod schemas and a Standard Schema wrapper', async () => {
    const registered: Array<{ name: string; config: { description?: string; title?: string; outputSchema?: unknown; annotations?: { readOnlyHint?: boolean }; inputSchema?: { '~standard'?: { validate: (value: unknown) => { value?: unknown; issues?: unknown[] }; jsonSchema: { input: () => unknown } } } }; callback: (args: Record<string, unknown>) => Promise<{ content: Array<{ type: 'text'; text: string }> }> }> = [];
    const server: ToolRegistrar = {
      registerTool(name, config, callback) {
        registered.push({ name, config: config as typeof registered[number]['config'], callback });
      },
    };
    registerOn(server, spec);
    expect(registered.map(item => item.name)).toEqual(['search', 'notes.add', 'quiet']);
    expect(registered[1]?.config.description).toBe('Add a note');
    expect(registered[1]?.config.title).toBe('Add note');
    expect(registered[1]?.config.annotations).toEqual({ readOnlyHint: false });
    const standard = jsonSchemaStandard(inputSchema);
    expect(standard['~standard']).toBeDefined();
    const wrapped = registered[1]?.config.inputSchema?.['~standard'];
    expect(wrapped?.jsonSchema.input()).toBe(inputSchema);
    expect(wrapped?.validate({ note: 'a' })).toEqual({ value: { note: 'a' } });
    expect(wrapped?.validate({})).toMatchObject({ issues: expect.any(Array) });
    expect(wrapped?.validate({ note: 1 }).issues?.[0]).toMatchObject({ path: ['note'] });
    await expect(registered[0]?.callback({ query: 'q' })).resolves.toEqual({ content: [{ type: 'text', text: 'found q' }] });
    await expect(registered[1]?.callback({ note: 'a' })).resolves.toEqual({ content: [{ type: 'text', text: JSON.stringify({ note: 'a' }) }] });
    await expect(registered[2]?.callback({})).resolves.toEqual({ content: [{ type: 'text', text: '' }] });
    const viewed = defineCommand({
      name: 'view',
      description: 'View',
      inputSchema: { type: 'object', properties: {} },
      run: () => cliView('{"ok":true}', { structuredContent: { ok: true }, isError: false }),
    });
    const exposed: Array<{ callback: (args: Record<string, unknown>) => Promise<{ content: Array<{ type: 'text'; text: string }>; structuredContent?: Record<string, unknown> }> }> = [];
    registerOn({
      registerTool(_name, _config, callback) {
        exposed.push({ callback });
      },
    }, { ...spec, commands: [viewed] });
    await expect(exposed[0]?.callback({})).resolves.toEqual({
      content: [{ type: 'text', text: '{"ok":true}' }],
      structuredContent: { ok: true },
    });
    const fallback: ToolRegistrar = { registerTool() { return undefined; } };
    const mcpCommand = spec.commands[1];
    if (!mcpCommand) throw new Error('missing command');
    registerOn(fallback, { ...spec, commands: [{ ...mcpCommand, inputSchema: undefined }] });
  });

  it('serves instructions and tools through an in-memory MCP server', async () => {
    const server = new McpServer({ name: 'issues', version: '1.0.0' }, mcpServerOptions(spec));
    registerOn(server as unknown as ToolRegistrar, spec);
    const client = new Client({ name: 'octocode-mcp-cli-test', version: '0.1.0' });
    const [serverTransport, clientTransport] = InMemoryTransport.createLinkedPair();
    await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);
    try {
      expect(client.getInstructions()).toBe('Read the index.');
      const listed = await client.listTools();
      const search = listed.tools.find(tool => tool.name === 'search');
      const note = listed.tools.find(tool => tool.name === 'notes.add');
      expect(search?.description).toBe('Search issues');
      expect(search?.inputSchema).toMatchObject({
        type: 'object',
        properties: { query: { type: 'string', description: 'Search text' } },
      });
      expect(note?.description).toBe('Add a note');
      expect(note?.title).toBe('Add note');
      expect(note?.annotations).toMatchObject({ readOnlyHint: false });
      expect(note?.outputSchema).toMatchObject({ type: 'object', properties: { note: { type: 'string' } } });
      expect(note?.inputSchema).toMatchObject({ properties: { note: { description: 'A note' } } });
      const result = await client.callTool({ name: 'search', arguments: { query: 'hi' } });
      expect(result.isError).not.toBe(true);
      const content = Array.isArray(result.content) ? result.content : [];
      const text = content.find(block => block.type === 'text' && 'text' in block);
      expect(text && 'text' in text ? text.text : '').toBe('found hi');
    } finally {
      await client.close();
      await server.close();
    }
  });
});
