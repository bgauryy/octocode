import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { discoverMcpSystem } from '../src/mcp-discovery.js';
import { workspaceAgentRoot } from '../src/paths.js';

const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

function temporaryRoot(prefix: string): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  roots.push(root);
  return root;
}

function write(file: string, content: string): void {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, content);
}

describe('host-neutral MCP discovery', () => {
  it('discovers only canonical global and namespaced workspace Octocode MCP files as active', () => {
    const cwd = temporaryRoot('shared-mcp-cwd-');
    const homeDir = temporaryRoot('shared-mcp-home-');
    const octocodeHome = path.join(homeDir, '.octocode-custom');
    const global = path.join(octocodeHome, 'agent', 'mcp', 'servers.json');
    const workspace = path.join(workspaceAgentRoot(cwd, octocodeHome), 'mcp', 'servers.json');
    write(global, JSON.stringify({ mcpServers: { global: { command: 'global-mcp' } } }));
    write(workspace, JSON.stringify({ mcpServers: { workspace: { command: 'workspace-mcp' } } }));

    const result = discoverMcpSystem(cwd, { homeDir, octocodeHome });

    expect(result.configs.filter((config) => config.active).map((config) => config.path))
      .toEqual([workspace, global]);
    expect(result.definitions).toEqual([]);
  });

  it('normalizes JSON and TOML vendor definitions with disabled provenance and collision-safe names', () => {
    const cwd = temporaryRoot('shared-mcp-vendors-cwd-');
    const homeDir = temporaryRoot('shared-mcp-vendors-home-');
    const cursor = path.join(cwd, '.cursor', 'mcp.json');
    const projectCodex = path.join(cwd, '.codex', 'config.toml');
    const userCodex = path.join(homeDir, '.codex', 'config.toml');
    write(cursor, JSON.stringify({ mcpServers: {
      docs: { url: 'https://docs.example.test/mcp', headers: { Authorization: '${env:DOCS_AUTH}' } },
    } }));
    write(projectCodex, '[mcp_servers.docs]\ncommand = "project-docs"\n');
    write(userCodex, '[mcp_servers.docs]\ncommand = "user-docs"\nargs = ["--stdio"]\n');

    const result = discoverMcpSystem(cwd, { homeDir });

    expect(result.definitions.map((definition) => definition.name))
      .toEqual(['cursor.docs', 'codex.docs', 'codex.user.docs']);
    expect(result.definitions[0]!.config).toMatchObject({
      disabled: true,
      transport: 'http',
      headerRefs: { Authorization: 'DOCS_AUTH' },
      discovered: { host: 'cursor', scope: 'project', path: cursor, originalName: 'docs' },
    });
    expect(result.definitions[2]!.config).toMatchObject({ disabled: true, args: ['--stdio'] });
  });

  it('covers .agents conventions and reports malformed sources without aborting', () => {
    const cwd = temporaryRoot('shared-mcp-agents-cwd-');
    const homeDir = temporaryRoot('shared-mcp-agents-home-');
    const agents = path.join(cwd, '.agents', 'mcp.json');
    const antigravity = path.join(cwd, '.agents', 'mcp_config.json');
    const malformed = path.join(homeDir, '.cursor', 'mcp.json');
    write(agents, JSON.stringify({ mcpServers: { shared: { command: 'agents-mcp' } } }));
    write(antigravity, JSON.stringify({ mcpServers: { drive: { serverUrl: 'https://drive.example.test/mcp', oauth: {} } } }));
    write(malformed, '{not json');

    const result = discoverMcpSystem(cwd, { homeDir });

    expect(result.definitions).toEqual(expect.arrayContaining([
      expect.objectContaining({ name: 'agents.shared', config: expect.objectContaining({ disabled: true }) }),
      expect.objectContaining({ name: 'antigravity.drive', config: expect.objectContaining({ disabled: true, auth: 'oauth' }) }),
    ]));
    expect(result.configs.find((config) => config.path === malformed)).toMatchObject({
      host: 'cursor', active: false, servers: [], error: expect.any(String),
    });
  });

  it('imports Pi, Claude, Cursor, Codex, and .agent MCP files from project and user scopes', () => {
    const cwd = temporaryRoot('shared-mcp-import-matrix-cwd-');
    const homeDir = temporaryRoot('shared-mcp-import-matrix-home-');
    const jsonSources = [
      [path.join(cwd, '.pi', 'mcp.json'), 'pi-project'],
      [path.join(cwd, '.pi', 'agent', 'mcp.json'), 'pi-agent-project'],
      [path.join(homeDir, '.pi', 'mcp.json'), 'pi-user'],
      [path.join(homeDir, '.pi', 'agent', 'mcp.json'), 'pi-agent-user'],
      [path.join(cwd, '.claude', 'mcp.json'), 'claude-project'],
      [path.join(homeDir, '.claude', 'mcp.json'), 'claude-user'],
      [path.join(cwd, '.cursor', 'mcp.json'), 'cursor-project'],
      [path.join(homeDir, '.cursor', 'mcp.json'), 'cursor-user'],
      [path.join(cwd, '.agent', 'mcp.json'), 'agent-project'],
      [path.join(homeDir, '.agent', 'mcp.json'), 'agent-user'],
    ] as const;
    for (const [file, server] of jsonSources) write(file, JSON.stringify({ mcpServers: { [server]: { command: server } } }));
    write(path.join(cwd, '.codex', 'config.toml'), '[mcp_servers.codex-project]\ncommand = "codex-project"\n');
    write(path.join(homeDir, '.codex', 'config.toml'), '[mcp_servers.codex-user]\ncommand = "codex-user"\n');

    const result = discoverMcpSystem(cwd, { homeDir });
    const discovered = result.definitions.map(({ config }) => config.discovered);

    expect(discovered).toEqual(expect.arrayContaining(jsonSources.map(([file]) => expect.objectContaining({ path: file }))));
    expect(discovered).toEqual(expect.arrayContaining([
      expect.objectContaining({ host: 'codex', scope: 'project', path: path.join(cwd, '.codex', 'config.toml') }),
      expect.objectContaining({ host: 'codex', scope: 'user', path: path.join(homeDir, '.codex', 'config.toml') }),
    ]));
    expect(result.definitions.every(({ config }) => config.disabled === true)).toBe(true);
  });

  it('rejects symlinked and oversized compatibility files before reading them', () => {
    const cwd = temporaryRoot('shared-mcp-bounds-cwd-');
    const homeDir = temporaryRoot('shared-mcp-bounds-home-');
    const outside = path.join(homeDir, 'outside.json');
    const linked = path.join(cwd, '.agents', 'mcp.json');
    const oversized = path.join(homeDir, '.cursor', 'mcp.json');
    write(outside, JSON.stringify({ mcpServers: { escaped: { command: 'nope' } } }));
    fs.mkdirSync(path.dirname(linked), { recursive: true });
    fs.symlinkSync(outside, linked);
    write(oversized, ' '.repeat((1024 * 1024) + 1));

    const result = discoverMcpSystem(cwd, { homeDir });

    expect(result.definitions).toEqual([]);
    expect(result.configs.find((config) => config.path === linked)?.error).toMatch(/non-symbolic-link/);
    expect(result.configs.find((config) => config.path === oversized)?.error).toMatch(/exceeds/);
  });
});
