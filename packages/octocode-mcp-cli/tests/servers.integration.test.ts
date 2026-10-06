import { mkdtemp, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import type { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { cliFromMcp, connectClient, createStdioTransport } from '../src/from-mcp.js';
import { runCli, type CliIo } from '../src/run.js';
import type { CliSpec } from '../src/spec.js';

const servers: Client[] = [];

afterEach(async () => {
  await Promise.all(servers.splice(0).map(client => client.close()));
});

async function mcpCli(args: string[]): Promise<{ client: Client; cli: CliSpec; stderr: () => string }> {
  const stderrChunks: Buffer[] = [];
  const transport = createStdioTransport({
    command: 'npx',
    args: ['-y', ...args],
    stderr: 'pipe',
  });
  transport.stderr?.on('data', (chunk: Buffer | string) => {
    stderrChunks.push(typeof chunk === 'string' ? Buffer.from(chunk) : chunk);
  });
  const client = await connectClient(transport);
  servers.push(client);
  const cli = await cliFromMcp(client);
  return {
    client,
    cli,
    stderr: () => Buffer.concat(stderrChunks).toString('utf8'),
  };
}

async function run(cli: CliSpec, argv: string[]): Promise<{ code: number; out: string; err: string }> {
  const stdout: string[] = [];
  const stderr: string[] = [];
  const io: CliIo = {
    stdout: text => { stdout.push(text); },
    stderr: text => { stderr.push(text); },
  };
  const code = await runCli(cli, argv, io);
  return { code, out: stdout.join(''), err: stderr.join('') };
}

describe('everything MCP server', () => {
  it('turns protocol tools into commands and calls echo and get-sum', async () => {
    const { cli, stderr } = await mcpCli(['@modelcontextprotocol/server-everything']);
    const names = cli.commands.map(command => command.name);
    expect(names, `${names.join(', ')}\n${stderr()}`).toEqual(expect.arrayContaining(['echo', 'get-sum']));

    const echo = cli.commands.find(command => command.name === 'echo');
    expect(echo?.description).toBe('Echoes back the input string');
    expect(echo?.flags.map(flag => [flag.name, flag.kind, flag.required])).toEqual([
      ['message', 'string', true],
    ]);
    const echoHelp = await run(cli, ['echo', '--help']);
    expect(echoHelp.code).toBe(0);
    expect(echoHelp.out.startsWith('Echoes back the input string')).toBe(true);
    expect(echoHelp.out).toContain('--message <string>');
    expect(echoHelp.err).toBe('');

    const echoed = await run(cli, ['echo', '--message', 'hello from mcp']);
    expect(echoed.err, echoed.out).toBe('');
    expect(echoed.code).toBe(0);
    expect(echoed.out).toContain('Echo: hello from mcp');

    const sum = cli.commands.find(command => command.name === 'get-sum');
    expect(sum?.description).toBe('Returns the sum of two numbers');
    expect(sum?.flags.map(flag => flag.name)).toEqual(['a', 'b']);
    const added = await run(cli, ['get-sum', '--a', '2', '--b', '3']);
    expect(added.err, added.out).toBe('');
    expect(added.code).toBe(0);
    expect(added.out).toContain('5');
  });
});

describe('filesystem MCP server', () => {
  it('lists the allowed directory and reads a file through the CLI', async () => {
    const root = await realpath(await mkdtemp(join(tmpdir(), 'octocode-mcp-cli-fs-')));
    await writeFile(join(root, 'note.txt'), 'hello from filesystem\n');
    try {
      const { cli, stderr } = await mcpCli(['@modelcontextprotocol/server-filesystem', root]);
      const names = cli.commands.map(command => command.name);
      expect(names, `${names.join(', ')}\n${stderr()}`).toEqual(expect.arrayContaining([
        'list_allowed_directories',
        'list_directory',
        'read_text_file',
      ]));

      const allowed = cli.commands.find(command => command.name === 'list_allowed_directories');
      expect(allowed?.description).toContain('allowed to access');
      expect(allowed?.flags.filter(flag => flag.required)).toEqual([]);
      const allowedRun = await run(cli, ['list_allowed_directories']);
      expect(allowedRun.err, allowedRun.out).toBe('');
      expect(allowedRun.code).toBe(0);
      expect(allowedRun.out).toContain(root);

      const listed = await run(cli, ['list_directory', '--path', root]);
      expect(listed.err, listed.out).toBe('');
      expect(listed.code).toBe(0);
      expect(listed.out).toContain('note.txt');

      const read = cli.commands.find(command => command.name === 'read_text_file');
      expect(read?.flags.find(flag => flag.name === 'path')).toMatchObject({ kind: 'string', required: true });
      const readHelp = await run(cli, ['read_text_file', '--help']);
      expect(readHelp.code).toBe(0);
      expect(readHelp.out.startsWith(read?.description ?? '')).toBe(true);
      expect(readHelp.out).toContain('--path <string>');
      const body = await run(cli, ['read_text_file', '--path', join(root, 'note.txt')]);
      expect(body.err, body.out).toBe('');
      expect(body.code).toBe(0);
      expect(body.out).toContain('hello from filesystem');
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
