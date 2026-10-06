import { describe, expect, it } from 'vitest';
import { InMemoryTransport, McpServer } from '@modelcontextprotocol/server';
import { z } from 'zod';
import { commandHelp, defaultOutput } from '../src/help.js';
import { cliView, isCliView, runCli } from '../src/run.js';
import { defineCli } from '../src/spec.js';
import {
  cliFromMcp,
  connectClient,
  connectStdio,
  connectStreamableHttp,
  createStdioTransport,
  createStreamableHttpTransport,
  type McpClientLike,
  type McpToolResult,
} from '../src/from-mcp.js';

function fixture(pages: Array<{ tools: McpClientLike['listTools'] extends (params?: infer _P) => Promise<infer R> ? R extends { tools: infer T } ? T : never : never; nextCursor?: string }>, call?: (name: string) => McpToolResult, info?: { name?: string; version?: string } | undefined, instructions?: string | undefined): McpClientLike & { calls: unknown[] } {
  const calls: unknown[] = [];
  return {
    calls,
    getInstructions: () => instructions,
    getServerVersion: () => info,
    async listTools(params) {
      calls.push(params);
      const page = pages[calls.length - 1];
      if (!page) throw new Error('unexpected page');
      return page;
    },
    async callTool(params) {
      return call?.(params.name) ?? { content: [] };
    },
  };
}

describe('cliFromMcp', () => {
  it('pages tools/list and keeps instructions', async () => {
    const client = fixture([
      {
        tools: [{ name: 'search', description: 'Search issues', inputSchema: { type: 'object', properties: { query: { type: 'string', description: 'Search text' } }, required: ['query'] } }],
        nextCursor: 'c2',
      },
      {
        tools: [{ name: 'issues.list', description: 'List issues', inputSchema: { type: 'object', properties: {} } }],
      },
    ], undefined, { name: 'issues', version: '9.0.0' }, 'Read the index.');
    const cli = await cliFromMcp(client);
    expect(client.calls).toEqual([undefined, { cursor: 'c2' }]);
    expect(cli.instructions).toBe('Read the index.');
    expect(cli.name).toBe('issues');
    expect(cli.version).toBe('9.0.0');
    expect(cli.commands.map(command => command.name)).toEqual(['search', 'issues.list']);
    expect(cli.commands[1]?.mcpName).toBeUndefined();
    expect(cli.commands[0]?.flags[0]).toMatchObject({ name: 'query', kind: 'string', description: 'Search text' });
  });

  it('throws when a cursor repeats or the page limit is hit', async () => {
    const repeated = fixture([
      { tools: [{ name: 'search', description: 'Search' }], nextCursor: 'same' },
      { tools: [], nextCursor: 'same' },
    ], undefined, { name: 'issues', version: '1' }, 'Read.');
    await expect(cliFromMcp(repeated)).rejects.toThrow('Repeated tools/list cursor');
    expect(repeated.calls).toHaveLength(2);

    const endless = fixture([], undefined, { name: 'issues', version: '1' }, '');
    endless.listTools = async params => {
      endless.calls.push(params);
      return { tools: [], nextCursor: `p${endless.calls.length}` };
    };
    await expect(cliFromMcp(endless)).rejects.toThrow('tools/list page limit');
    expect(endless.calls).toHaveLength(100);
  });

  it('calls the original tool name and prefers structured content', async () => {
    const client = fixture([
      {
        tools: [
          { name: 'help', description: 'Help tool' },
          { name: 'a-b', description: 'First' },
          { name: 'a.b', description: 'Second' },
          { name: 'a-b-2', description: 'Taken' },
          { name: 'a b', description: 'Collision' },
        ],
      },
    ], name => {
      if (name === 'help') return { isError: true, content: [{ type: 'text', text: 'nope' }] };
      if (name === 'a-b') return { isError: true, content: [] };
      return {
        structuredContent: { ok: true },
        content: [{ type: 'text', text: 'ignore' }, { type: 'text', text: 'also' }],
      };
    }, { name: '', version: '' }, undefined);
    const cli = await cliFromMcp(client);
    expect(cli.name).toBe('mcp');
    expect(cli.version).toBe('0.0.0');
    expect(cli.instructions).toBe('');
    expect(cli.commands.map(command => [command.name, command.mcpName])).toEqual([
      ['help-command', 'help'],
      ['a-b', undefined],
      ['a.b', undefined],
      ['a-b-2', undefined],
      ['a-b-3', 'a b'],
    ]);
    expect(cli.commands[0]?.inputSchema).toEqual({ type: 'object', properties: {} });
    await expect(cli.commands[0]?.run({})).rejects.toThrow('nope');
    await expect(cli.commands[1]?.run({})).rejects.toThrow('tool error');
    const kept = [{ type: 'text', text: 'ignore' }, { type: 'text', text: 'also' }];
    await expect(cli.commands[2]?.run({})).resolves.toEqual(cliView(
      JSON.stringify({ ok: true }, null, 2),
      { content: kept, structuredContent: { ok: true }, isError: false },
    ));
    const bothOut: string[] = [];
    expect(await runCli(cli, ['a.b'], {
      stdout: text => { bothOut.push(text); },
      stderr: () => undefined,
    })).toBe(0);
    expect(bothOut.join('')).toBe('ok: true\nignore\nalso\n');

    const image = { type: 'image', mimeType: 'image/png', data: 'aaaa' };
    const textClient = fixture([
      { tools: [{ name: 'search', description: 'Search', inputSchema: { type: 'object', properties: {} } }] },
    ], () => ({ content: [{ type: 'text', text: 'one' }, image, { type: 'text', text: 'two' }] }), { name: 'issues', version: '1' }, 'i');
    const textCli = await cliFromMcp(textClient);
    await expect(textCli.commands[0]?.run({})).resolves.toEqual(cliView(`one\n${JSON.stringify(image)}\ntwo`, {
      content: [{ type: 'text', text: 'one' }, image, { type: 'text', text: 'two' }],
      isError: false,
    }));

    const taskClient = fixture([
      { tools: [{ name: 'search', description: 'Search', inputSchema: { type: 'object', properties: {} } }] },
    ], () => ({ toolResult: { ok: true } }), { name: 'issues', version: '1' }, 'i');
    const taskCli = await cliFromMcp(taskClient);
    await expect(taskCli.commands[0]?.run({})).resolves.toEqual(cliView(
      JSON.stringify({ ok: true }, null, 2),
      { toolResult: { ok: true } },
    ));
  });

  it('keeps dotted names, titles, output schemas, hints, and non-text blocks', async () => {
    const outputSchema = {
      type: 'object',
      properties: { temperature: { type: 'number', description: 'Celsius' } },
      required: ['temperature'],
    };
    const blocks = [
      { type: 'audio', mimeType: 'audio/wav' },
      { type: 'resource_link', uri: 'file:///a', name: 'a' },
      { type: 'resource', resource: { uri: 'file:///b', text: 'body' } },
      { type: 'widget' },
    ];
    const client = fixture([
      {
        tools: [
          {
            name: 'admin.tools.list',
            title: 'Admin tools',
            description: 'List admin tools',
            inputSchema: { type: 'object', additionalProperties: false },
            outputSchema,
            annotations: {
              title: 'Fallback title',
              readOnlyHint: true,
              destructiveHint: false,
              idempotentHint: true,
              openWorldHint: false,
            },
          },
          { name: 'picture', description: 'Picture', inputSchema: {} },
          { name: 'fallback', description: 'Fallback', annotations: { title: 'Shown title' }, inputSchema: { type: 'object' } },
        ],
      },
    ], name => (
      name === 'picture'
        ? { content: blocks }
        : { isError: true, content: [{ type: 'image', mimeType: 'image/png' }] }
    ), { name: 'srv', version: '1' }, 'Use the index.');
    const cli = await cliFromMcp(client);
    const listed = cli.commands[0];
    expect(listed).toMatchObject({
      name: 'admin.tools.list',
      title: 'Admin tools',
      outputSchema,
      flags: [],
    });
    const help = commandHelp(defineCli({ name: cli.name, instructions: cli.instructions, commands: cli.commands }), listed!);
    expect(help.startsWith('List admin tools')).toBe(true);
    expect(help).toContain('Admin tools');
    expect(help).toContain('temperature number');
    expect(help).toContain('Celsius (required)');
    expect(help).toContain('HINTS\n  read-only\n  additive\n  idempotent\n  closed-world');
    expect(defaultOutput(cli)).toContain('admin.tools.list:  Admin tools — List admin tools');
    await expect(listed?.run({})).rejects.toThrow(JSON.stringify({ type: 'image', mimeType: 'image/png' }));
    const picture = cli.commands[1];
    expect(picture?.flags).toEqual([]);
    const view = cliView(
      blocks.map(block => JSON.stringify(block)).join('\n'),
      { content: blocks, isError: false },
    );
    await expect(picture?.run({})).resolves.toEqual(view);
    const stdout: string[] = [];
    expect(await runCli(cli, ['picture', '--json'], {
      stdout: text => { stdout.push(text); },
      stderr: () => undefined,
    })).toBe(0);
    expect(JSON.parse(stdout.join(''))).toEqual({ content: blocks, isError: false });
    expect(cli.commands[2]).toMatchObject({ name: 'fallback', title: 'Shown title' });
  });

  it('builds the CLI from a connected MCP client and calls the tool', async () => {
    const server = new McpServer({ name: 'issues', version: '2.0.0' }, { instructions: 'Search before you write.' });
    server.registerTool(
      'search',
      {
        description: 'Search issues by text.',
        inputSchema: z.object({ query: z.string().describe('Search text') }),
      },
      async ({ query }) => ({ content: [{ type: 'text' as const, text: `found ${query}` }] }),
    );
    server.registerTool(
      'shot',
      { description: 'Shot', inputSchema: z.object({}) },
      async () => ({
        content: [
          { type: 'text' as const, text: 'see' },
          { type: 'image' as const, data: 'aaaa', mimeType: 'image/png' },
          { type: 'resource' as const, resource: { uri: 'file:///b', mimeType: 'text/plain', text: 'body' } },
          { type: 'resource' as const, resource: { uri: 'file:///c', mimeType: 'application/octet-stream', blob: 'bbbb' } },
        ],
      }),
    );
    server.registerTool(
      'full',
      { description: 'Full', inputSchema: z.object({}) },
      async () => ({
        _meta: { trace: 'abc' },
        structuredContent: { line: 'function flagHead(flag: Flag, dashed = true): string {', note: 'tail ' },
        content: [
          { type: 'text' as const, text: 'function flagHead(flag: Flag, dashed = true): string {' },
          { type: 'image' as const, data: 'aaaa', mimeType: 'image/png' },
          { type: 'resource' as const, resource: { uri: 'file:///b', mimeType: 'application/octet-stream', blob: 'bbbb' } },
        ],
      }),
    );
    const [serverTransport, clientTransport] = InMemoryTransport.createLinkedPair();
    await server.connect(serverTransport);
    const client = await connectClient(clientTransport);
    const stdout: string[] = [];
    const stderr: string[] = [];
    const io = {
      stdout: (text: string) => { stdout.push(text); },
      stderr: (text: string) => { stderr.push(text); },
    };
    try {
      const cli = await cliFromMcp(client);
      expect(cli.instructions).toBe('Search before you write.');
      expect(cli.name).toBe('issues');
      expect(cli.version).toBe('2.0.0');
      expect(cli.commands[0]).toMatchObject({ name: 'search', description: 'Search issues by text.' });
      expect(cli.commands[0]?.flags[0]).toMatchObject({ name: 'query', kind: 'string', required: true, description: 'Search text' });
      stdout.length = 0;
      expect(await runCli(cli, [], io)).toBe(0);
      expect(stdout.join('')).toContain('Search before you write.');
      expect(stderr.join('')).toBe('');
      stdout.length = 0;
      expect(await runCli(cli, ['search', '--help'], io)).toBe(0);
      expect(stdout.join('').startsWith('Search issues by text.')).toBe(true);
      expect(stdout.join('')).toContain('--query <string>');
      stdout.length = 0;
      expect(await runCli(cli, ['search', '--query', 'hooks'], io)).toBe(0);
      expect(stdout.join('')).toBe('found hooks\n');
      expect(stderr.join('')).toBe('');
      const shot = cli.commands.find(command => command.name === 'shot');
      const shotResult = await shot?.run({});
      expect(isCliView(shotResult)).toBe(true);
      if (!isCliView(shotResult)) return;
      const shotJson = shotResult.json as { content: Array<Record<string, unknown>> };
      expect(shotJson.content[1]).toMatchObject({ type: 'image', data: 'aaaa', mimeType: 'image/png' });
      expect(shotJson.content[2]).toMatchObject({ type: 'resource', resource: { uri: 'file:///b', mimeType: 'text/plain', text: 'body' } });
      expect(shotJson.content[3]).toMatchObject({ type: 'resource', resource: { uri: 'file:///c', mimeType: 'application/octet-stream', blob: 'bbbb' } });
      expect(shotResult.text).toContain('aaaa');
      expect(shotResult.text).toContain('body');
      expect(shotResult.text).toContain('bbbb');
      expect(shotResult.text.startsWith('see\n')).toBe(true);
      stdout.length = 0;
      expect(await runCli(cli, ['shot'], io)).toBe(0);
      expect(stdout.join('')).toBe(`${shotResult.text}\n`);
      const full = cli.commands.find(command => command.name === 'full');
      const fullResult = await full?.run({});
      expect(isCliView(fullResult)).toBe(true);
      if (!isCliView(fullResult)) return;
      const fullJson = fullResult.json as {
        _meta: { trace: string };
        structuredContent: { line: string; note: string };
        content: Array<{ type: string; text?: string; data?: string; resource?: { blob?: string } }>;
        isError: boolean;
      };
      expect(fullJson._meta).toEqual({ trace: 'abc' });
      expect(fullJson.structuredContent.line).toBe('function flagHead(flag: Flag, dashed = true): string {');
      expect(fullJson.structuredContent.note).toBe('tail ');
      expect(fullJson.content[0]?.text).toBe('function flagHead(flag: Flag, dashed = true): string {');
      expect(fullJson.content[1]?.data).toBe('aaaa');
      expect(fullJson.content[2]?.resource?.blob).toBe('bbbb');
      expect(fullJson.isError).toBe(false);
      stdout.length = 0;
      expect(await runCli(cli, ['full'], io)).toBe(0);
      const human = stdout.join('');
      expect(human).toContain('line: function flagHead(flag: Flag, dashed = true): string {');
      expect(human).toContain('note: tail ');
      expect(human).toContain('aaaa');
      expect(human).toContain('bbbb');
      expect(human).toContain('function flagHead(flag: Flag, dashed = true): string {\n');
      stdout.length = 0;
      expect(await runCli(cli, ['full', '--json'], io)).toBe(0);
      expect(JSON.parse(stdout.join(''))).toEqual(fullJson);
    } finally {
      await client.close();
      await server.close();
    }
  });

  it('builds transports without starting them and connects an in-memory client', async () => {
    const stdio = createStdioTransport({ command: 'node', args: ['-e', ''] });
    expect(stdio).toBeDefined();
    let seen: unknown;
    const connected = await connectStdio({ command: 'node', args: ['-e', ''] }, async transport => {
      seen = transport;
      return { close: async () => undefined } as never;
    });
    expect(seen).toBeInstanceOf(createStdioTransport({ command: 'node' }).constructor);
    expect(connected).toBeDefined();

    const http = createStreamableHttpTransport('http://127.0.0.1/mcp');
    const httpUrl = createStreamableHttpTransport(new URL('http://127.0.0.1/mcp'), { sessionId: 's' });
    expect(http).toBeDefined();
    expect(httpUrl).toBeDefined();
    await connectStreamableHttp('http://127.0.0.1/mcp', undefined, async transport => {
      expect(transport).toBeInstanceOf(createStreamableHttpTransport('http://127.0.0.1/mcp').constructor);
      return { close: async () => undefined } as never;
    });

    const server = new McpServer({ name: 'demo', version: '3' }, { instructions: 'hi' });
    const [serverTransport, clientTransport] = InMemoryTransport.createLinkedPair();
    await server.connect(serverTransport);
    const client = await connectClient(clientTransport);
    expect(client.getInstructions()).toBe('hi');
    await client.close();
    await server.close();
  });
});
