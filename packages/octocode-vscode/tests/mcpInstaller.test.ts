import * as fs from 'fs/promises';
import * as os from 'os';
import * as path from 'path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { EditorInfo, McpClientDef } from '../src/configPaths';
import { McpInstaller } from '../src/mcpInstaller';

let directory: string;
let lines: string[];
const info = vi.fn();
const error = vi.fn();

beforeEach(async () => {
  directory = await fs.mkdtemp(path.join(os.tmpdir(), 'octocode-installer-'));
  lines = [];
  info.mockReset();
  error.mockReset();
});

afterEach(async () => {
  await fs.rm(directory, { recursive: true, force: true });
});

function file(name: string): string {
  return path.join(directory, name, 'mcp.json');
}

function client(name: string, configPath = file(name)): McpClientDef {
  return { name, getConfigPath: () => configPath, configKey: 'mcpServers' };
}

function unresolvable(name: string): McpClientDef {
  return {
    name,
    getConfigPath: () => {
      throw new Error('no home directory');
    },
    configKey: 'mcpServers',
  };
}

function installer(
  clients: Record<string, McpClientDef>,
  editor: EditorInfo = { name: 'Editor', configKey: 'servers', mcpConfigPath: null },
  getToken = () => Promise.resolve<string | undefined>('token')
): McpInstaller {
  return new McpInstaller({
    clients,
    editorInfo: () => editor,
    getToken,
    output: line => lines.push(line),
    info,
    error,
  });
}

async function readServer(configPath: string, key = 'mcpServers') {
  const config = JSON.parse(await fs.readFile(configPath, 'utf8'));
  return config[key].octocode;
}

async function writeBroken(configPath: string): Promise<void> {
  await fs.mkdir(path.dirname(configPath), { recursive: true });
  await fs.writeFile(configPath, '{ broken');
}

describe('McpInstaller.install', () => {
  it('reports installed, then unchanged, when notifying', async () => {
    const target = { name: 'Cline', path: file('cline'), configKey: 'mcpServers' as const };
    const subject = installer({});
    expect(await subject.install(target, true)).toBe('installed');
    expect(info).toHaveBeenLastCalledWith(
      'Octocode MCP server configured for Cline! Restart to enable it.'
    );
    expect(await subject.install(target, true)).toBe('unchanged');
    expect(info).toHaveBeenLastCalledWith(
      'Octocode MCP server is already configured for Cline.'
    );
    expect((await readServer(target.path)).env).toEqual({ GH_TOKEN: 'token' });
    expect(lines).toContain(`MCP config installed: ${target.path}`);
  });

  it('stays silent without notification', async () => {
    const target = { name: 'Cline', path: file('cline'), configKey: 'mcpServers' as const };
    expect(await installer({}).install(target, false)).toBe('installed');
    await writeBroken(target.path);
    expect(await installer({}).install(target, false)).toBe('failed');
    expect(info).not.toHaveBeenCalled();
    expect(error).not.toHaveBeenCalled();
  });

  it('reports a failure, including a non-Error rejection', async () => {
    const target = { name: 'Cline', path: file('cline'), configKey: 'mcpServers' as const };
    const subject = installer({}, undefined, () => Promise.reject('keychain locked'));
    expect(await subject.install(target, true)).toBe('failed');
    expect(error).toHaveBeenCalledWith('Failed to configure MCP server: keychain locked');
    expect(lines).toContain('Failed to configure MCP server: keychain locked');
  });
});

describe('McpInstaller.syncToken', () => {
  it('updates each distinct config once and lists the clients it could not update', async () => {
    const shared = file('shared');
    const editor: EditorInfo = { name: 'Cursor', configKey: 'mcpServers', mcpConfigPath: shared };
    const subject = installer(
      { same: client('Same', shared), broken: client('Broken'), lost: unresolvable('Lost') },
      editor
    );
    await subject.install({ name: 'Cursor', path: shared, configKey: 'mcpServers' }, false);
    await writeBroken(file('Broken'));
    await expect(subject.syncToken('fresh')).rejects.toThrow(
      'Could not update MCP configs for: Lost, Broken'
    );
    expect((await readServer(shared)).env).toEqual({ GH_TOKEN: 'fresh' });
    expect(lines.filter(line => line.startsWith('Updated token'))).toEqual([
      `Updated token in Cursor config: ${shared}`,
    ]);
    expect(lines.join('\n')).toContain('Cannot resolve the Lost config path: Error: no home directory');
    expect(lines.join('\n')).toContain('Failed to update Broken config');
  });

  it('skips the editor when it has no MCP config', async () => {
    const subject = installer({ cline: client('Cline') });
    await subject.install({ name: 'Cline', path: file('Cline'), configKey: 'mcpServers' }, false);
    await subject.syncToken(undefined);
    expect(await readServer(file('Cline'))).not.toHaveProperty('env');
  });
});

describe('McpInstaller commands', () => {
  it('installs into the editor config, or says the editor has none', async () => {
    const editor: EditorInfo = { name: 'Code', configKey: 'servers', mcpConfigPath: file('code') };
    await installer({}).installForEditor(editor);
    expect((await readServer(file('code'), 'servers')).command).toBe('npx');
    await installer({}).installForEditor({ ...editor, mcpConfigPath: null });
    expect(error).toHaveBeenCalledWith('MCP configuration not supported for this editor.');
  });

  it('reports an editor install that fails while reporting', async () => {
    const editor: EditorInfo = { name: 'Code', configKey: 'servers', mcpConfigPath: file('code') };
    const subject = new McpInstaller({
      clients: {},
      editorInfo: () => editor,
      getToken: () => Promise.resolve(undefined),
      output: () => {
        throw new Error('channel closed');
      },
      info,
      error,
    });
    await subject.installForEditor(editor);
    expect(error).toHaveBeenLastCalledWith('Failed to install MCP: channel closed');
  });

  it('installs one client by key and reports unknown or unresolvable ones', async () => {
    const subject = installer({ cline: client('Cline'), lost: unresolvable('Lost') });
    await subject.installForClient('cline');
    expect(info).toHaveBeenCalledWith(
      'Octocode MCP server configured for Cline! Restart to enable it.'
    );
    await subject.installForClient('missing');
    expect(error).toHaveBeenCalledWith('Unknown MCP client: missing');
    await subject.installForClient('lost');
    expect(error).toHaveBeenLastCalledWith('Error installing for lost: no home directory');
    expect(lines).toContain('Error installing for lost: no home directory');
  });

  it('summarizes a bulk install per client', async () => {
    const subject = installer({
      done: client('Done'),
      fresh: client('Fresh'),
      lost: unresolvable('Lost'),
    });
    await subject.install({ name: 'Done', path: file('Done'), configKey: 'mcpServers' }, false);
    await subject.installForAll();
    expect(info).toHaveBeenLastCalledWith(
      'Octocode MCP installation complete:\n⏭️ Done (already configured)\n✅ Fresh\n❌ Lost (failed)'
    );
  });
});
