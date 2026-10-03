import * as fs from 'fs/promises';
import * as os from 'os';
import * as path from 'path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { configureMcpServer, updateMcpConfigToken } from '../src/mcpConfig';

vi.mock('fs/promises', async original => ({
  ...(await original<typeof import('fs/promises')>()),
}));

let directory: string;
let file: string;
beforeEach(async () => {
  directory = await fs.mkdtemp(path.join(os.tmpdir(), 'octocode-config-'));
  file = path.join(directory, 'mcp.json');
});
afterEach(async () => {
  await fs.rm(directory, { recursive: true, force: true });
});

describe('MCP configuration persistence', () => {
  it.each([
    null,
    [],
    { servers: [] },
    { servers: { octocode: null } },
    { servers: { octocode: { env: [] } } },
  ])('rejects invalid shapes without writing: %j', async value => {
    const original = JSON.stringify(value);
    await fs.writeFile(file, original);
    await expect(configureMcpServer(file, 'token', 'servers')).rejects.toThrow(
      'Invalid'
    );
    expect(await fs.readFile(file, 'utf8')).toBe(original);
  });

  it('installs and syncs credentials using the VS Code servers key', async () => {
    await fs.writeFile(
      file,
      JSON.stringify({
        inputs: [],
        servers: { other: { url: 'https://example.test' } },
      })
    );
    expect(await configureMcpServer(file, 'first', 'servers')).toBe(
      'installed'
    );
    expect(await configureMcpServer(file, 'first', 'servers')).toBe(
      'unchanged'
    );
    await updateMcpConfigToken(file, 'second', 'servers');
    const result = JSON.parse(await fs.readFile(file, 'utf8'));
    expect(result.servers.octocode.env.OCTOCODE_TOKEN).toBe('second');
    expect(result.servers.other.url).toBe('https://example.test');
    expect(result.inputs).toEqual([]);
    expect(result).not.toHaveProperty('mcpServers');
  });

  it('serializes overlapping installation and token updates', async () => {
    await Promise.all([
      configureMcpServer(file, 'first'),
      updateMcpConfigToken(file, 'second'),
    ]);
    expect(
      JSON.parse(await fs.readFile(file, 'utf8')).mcpServers.octocode.env
        .OCTOCODE_TOKEN
    ).toBe('second');
  });

  it('preserves original bytes and cleans temporary files if replacement fails', async () => {
    await fs.writeFile(file, '{}');
    vi.spyOn(fs, 'rename').mockRejectedValueOnce(
      new Error('replacement failed')
    );
    await expect(configureMcpServer(file, 'token')).rejects.toThrow(
      'replacement failed'
    );
    expect(await fs.readFile(file, 'utf8')).toBe('{}');
    expect(await fs.readdir(directory)).toEqual(['mcp.json']);
    expect(await configureMcpServer(file, 'token')).toBe('installed');
  });
});
