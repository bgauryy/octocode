import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import fs from 'node:fs';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { configuredOctocodeServer, isOctocodeTool, literalConfigValue, OCTOCODE_MCP_VERSION, octocodeServerConfig, registerOctocodeMcp, sanitizeOctocodeResult } from '../src/mcp/octocode.js';
import { fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

const piDist = path.dirname(fileURLToPath(import.meta.resolve('@earendil-works/pi-coding-agent')));

describe('Octocode MCP on Pi built-in MCP', () => {
  it('registers for every session and can retry a failed registration in the same folder', async () => {
    const fake = fakePi();
    const registration = vi.spyOn(fake.pi, 'registerMcpServer').mockImplementationOnce(() => { throw new Error('not ready'); });
    registerOctocodeMcp(fake.pi);
    await expect(fake.emit('session_start', {}, { cwd: '/w' })).rejects.toThrow('not ready');
    await fake.emit('session_start', {}, { cwd: '/w' });
    expect(fake.mcpServers.get('octocode')).toMatchObject({ cwd: '/w' });
    await fake.emit('session_start', {}, { cwd: '/w' });
    expect(registration).toHaveBeenCalledTimes(3);
  });

  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it('launches the bundled octocode-mcp with direct exposure and the workspace allowed', () => {
    const config = octocodeServerConfig('/work/app', { OCTOCODE_HOME: '/h/.octocode' }, '/x/octocode-mcp/bin.js');
    expect(config).toMatchObject({ command: process.execPath, args: ['/x/octocode-mcp/bin.js'], exposure: 'direct', env: { WORKSPACE_ROOT: '/work/app', ALLOWED_PATHS: '/work/app,/h/.octocode' } });
    // The real bundled bin resolves (octocode-mcp is a dependency).
    const bundled = octocodeServerConfig('/w');
    expect('args' in bundled && fs.existsSync(bundled.args![0]!)).toBe(true);
  });

  it('falls back to the pinned npm release, never @latest', () => {
    const manifest = JSON.parse(fs.readFileSync(path.join(import.meta.dirname, '..', 'package.json'), 'utf8')) as { dependencies: Record<string, string> };
    expect(manifest.dependencies['octocode-mcp']).toBe(OCTOCODE_MCP_VERSION);
    expect(octocodeServerConfig('/w', {}, '')).toMatchObject({ command: 'npx', args: ['-y', `octocode-mcp@${OCTOCODE_MCP_VERSION}`] });
  });

  it('keeps env values literal under Pi config-value expansion', async () => {
    const { resolveConfigValueOrThrow } = (await import(pathToFileURL(path.join(piDist, 'core', 'resolve-config-value.js')).href)) as { resolveConfigValueOrThrow: (value: string, label: string) => string };
    for (const value of ['/plain/path', '/a/$HOME/b', '/x/${USER}', '!echo hi', '/c$$d']) expect(resolveConfigValueOrThrow(literalConfigValue(value), 'test')).toBe(value);
  });

  it('registers per session folder on session_start, unless OCTOCODE_MCP=0', async () => {
    const fake = fakePi();
    expect(registerOctocodeMcp(fake.pi)).toBe(true);
    expect(fake.mcpServers.size).toBe(0);
    await fake.emit('session_start', {}, { cwd: '/w' });
    await fake.emit('session_start', {}, { cwd: '/w' });
    expect(fake.mcpServers.get('octocode')).toMatchObject({ cwd: '/w', exposure: 'direct' });
    await fake.emit('session_start', {}, { cwd: '/other' });
    expect(fake.mcpServers.get('octocode')).toMatchObject({ cwd: '/other' });
    vi.stubEnv('OCTOCODE_MCP', '0');
    const off = fakePi();
    expect(registerOctocodeMcp(off.pi)).toBe(false);
    await off.emit('session_start', {}, { cwd: '/w' });
    expect(off.mcpServers.size).toBe(0);
  });

  it('reads an octocode entry from Pi\'s mcp.json, the trusted project file replacing the global one', () => {
    const agentDir = tmp();
    const cwd = tmp();
    expect(configuredOctocodeServer(cwd, true, agentDir)).toBeUndefined();
    fs.writeFileSync(path.join(agentDir, 'mcp.json'), JSON.stringify({ mcpServers: { octocode: { command: 'x', enabled: false }, other: {} } }));
    fs.mkdirSync(path.join(cwd, '.pi'));
    fs.writeFileSync(path.join(cwd, '.pi', 'mcp.json'), JSON.stringify({ mcpServers: { octocode: { command: 'y' } } }));
    expect(configuredOctocodeServer(cwd, false, agentDir)).toEqual({ command: 'x', enabled: false });
    expect(configuredOctocodeServer(cwd, true, agentDir)).toEqual({ command: 'y' });
    fs.writeFileSync(path.join(cwd, '.pi', 'mcp.json'), '{ not json');
    expect(configuredOctocodeServer(cwd, true, agentDir)).toEqual({ command: 'x', enabled: false });
  });

  it('recognizes Pi-named Octocode tools', () => {
    expect(isOctocodeTool('mcp__octocode__localSearch')).toBe(true);
    expect(isOctocodeTool('octocode_localSearch')).toBe(false);
    expect(isOctocodeTool('mcp__other__x')).toBe(false);
  });

  it('declares local and LSP tools directly and defers GitHub and npm tools, unless OCTOCODE_MCP_DIRECT=1', async () => {
    type Exposure = (config: unknown, tool: string) => string;
    const servers = (await import(pathToFileURL(path.join(piDist, 'core', 'mcp-servers.js')).href)) as { getMcpToolExposure: Exposure; validateMcpServerConfig: (name: string, raw: unknown) => unknown };
    const config = octocodeServerConfig('/w', {}, '/x/bin.js');
    // Pi accepts the config as is.
    expect(servers.validateMcpServerConfig('octocode', config)).toMatchObject({ toolExposure: { 'gh*': 'deferred', npmSearch: 'deferred' } });
    const exposures = Object.fromEntries(['localSearch', 'localGetFileContent', 'localAnalyzeGraph', 'lspGetSemantics', 'ghSearch', 'ghGetFileContent', 'ghSearchHistory', 'ghGetHistoryItem', 'npmSearch'].map((tool) => [tool, servers.getMcpToolExposure(config, tool)]));
    expect(exposures).toEqual({ localSearch: 'direct', localGetFileContent: 'direct', localAnalyzeGraph: 'direct', lspGetSemantics: 'direct', ghSearch: 'deferred', ghGetFileContent: 'deferred', ghSearchHistory: 'deferred', ghGetHistoryItem: 'deferred', npmSearch: 'deferred' });
    const direct = octocodeServerConfig('/w', { OCTOCODE_MCP_DIRECT: '1' }, '/x/bin.js');
    expect(direct).not.toHaveProperty('toolExposure');
    expect(servers.getMcpToolExposure(direct, 'ghSearch')).toBe('direct');
  });

  it('sanitizes Octocode tool results: text parts and structured content, other tools untouched', async () => {
    const dirty = 'a\u202eb\u{e0041}\u{e0042}c\u001b]0;title\u0007d\u001b[31me\u0000f';
    const event = { toolName: 'mcp__octocode__localSearch', content: [{ type: 'text' as const, text: dirty }, { type: 'image' as const, data: 'x', mimeType: 'image/png' }], structuredContent: { files: [{ path: 'p', text: dirty, n: 1 }] } };
    const result = sanitizeOctocodeResult(event)!;
    expect(result.content).toEqual([{ type: 'text', text: 'abcdef' }, { type: 'image', data: 'x', mimeType: 'image/png' }]);
    expect(result.structuredContent).toEqual({ files: [{ path: 'p', text: 'abcdef', n: 1 }] });
    // Content without structuredContent stays that way; clean results and other tools pass through.
    expect(sanitizeOctocodeResult({ toolName: 'mcp__octocode__ghSearch', content: [{ type: 'text', text: dirty }] })).toEqual({ content: [{ type: 'text', text: 'abcdef' }] });
    expect(sanitizeOctocodeResult({ toolName: 'mcp__octocode__ghSearch', content: [{ type: 'text', text: 'clean\ttext\n' }], structuredContent: { a: ['x'] } })).toBeUndefined();
    expect(sanitizeOctocodeResult({ toolName: 'mcp__other__x', content: [{ type: 'text', text: dirty }] })).toBeUndefined();
    // Registered as a tool_result handler, also when the built-in server is off (a user's own octocode server).
    vi.stubEnv('OCTOCODE_MCP', '0');
    const fake = fakePi();
    registerOctocodeMcp(fake.pi);
    expect(await fake.fire('tool_result', event, {})).toEqual(result);
  });
});
