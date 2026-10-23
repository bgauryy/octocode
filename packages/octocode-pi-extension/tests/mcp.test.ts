import fs from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { configuredOctocodeServer, OCTOCODE_MCP_SPEC, OctocodeMcp, octocodeLaunch } from '../src/mcp/octocode.js';
import { isOctocodeTool } from '../src/shared/octocode-tools.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';
import { alive, echo, exits, session, text } from './mcp-session.js';

afterEach(() => {
  vi.unstubAllEnvs();
});

describe('Octocode MCP launch and ownership', () => {
  it('launches the bundled octocode-mcp with the workspace allowed', () => {
    const launch = octocodeLaunch('/work/app', { OCTOCODE_HOME: '/h/.octocode' }, '/x/octocode-mcp/bin.js');
    expect(launch).toEqual({ command: process.execPath, args: ['/x/octocode-mcp/bin.js'], cwd: '/work/app', env: { WORKSPACE_ROOT: '/work/app', ALLOWED_PATHS: '/work/app,/h/.octocode' } });
    // The real bundled bin resolves (octocode-mcp is a dependency).
    expect(fs.existsSync(octocodeLaunch('/w').args[0]!)).toBe(true);
  });

  it('runs the octocode-mcp range of package.json: the dependency and the npx fallback share it', () => {
    const manifest = JSON.parse(fs.readFileSync(path.join(import.meta.dirname, '..', 'package.json'), 'utf8')) as { dependencies: Record<string, string> };
    const range = manifest.dependencies['octocode-mcp']!;
    expect(OCTOCODE_MCP_SPEC).toBe(`octocode-mcp@${range}`);
    expect(octocodeLaunch('/w', {}, '')).toMatchObject({ command: 'npx', args: ['-y', `octocode-mcp@${range}`] });
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
    fs.writeFileSync(path.join(cwd, '.pi', 'mcp.json'), JSON.stringify({ mcpServers: 'none' }));
    expect(configuredOctocodeServer(cwd, true, agentDir)).toEqual({ command: 'x', enabled: false });
    fs.writeFileSync(path.join(cwd, '.pi', 'mcp.json'), '{ not json');
    expect(configuredOctocodeServer(cwd, true, agentDir)).toEqual({ command: 'x', enabled: false });
  });

  it('recognizes Pi-named Octocode tools', () => {
    expect(isOctocodeTool('mcp__octocode__localSearch')).toBe(true);
    expect(isOctocodeTool('octocode_localSearch')).toBe(false);
    expect(isOctocodeTool('mcp__other__x')).toBe(false);
  });

  it('inherits global settings for a trusted project MCP override, but replaces them for a new transport', () => {
    const agentDir = tmp();
    const cwd = tmp();
    vi.stubEnv('PI_CODING_AGENT_DIR', agentDir);
    fs.mkdirSync(path.join(cwd, '.pi'));
    const projectConfig = path.join(cwd, '.pi', 'mcp.json');
    const global = { command: 'octocode-mcp', enabled: false, env: { TEST: 'kept' } };
    fs.writeFileSync(path.join(agentDir, 'mcp.json'), JSON.stringify({ mcpServers: { octocode: global } }));
    const writeProject = (octocode: unknown) => fs.writeFileSync(projectConfig, JSON.stringify({ mcpServers: { octocode } }));
    writeProject({ exposure: 'direct' });
    expect(configuredOctocodeServer(cwd, true, agentDir)).toEqual({ ...global, exposure: 'direct' });
    const mcp = new OctocodeMcp(fakePi().pi);
    const ctx = fakeCtx({ cwd });
    expect(mcp.owner(ctx)).toEqual({ by: 'user', enabled: false });
    writeProject({ enabled: true, toolExposure: { localFetch: 'direct' } });
    expect(configuredOctocodeServer(cwd, true, agentDir)).toEqual({ ...global, enabled: true, toolExposure: { localFetch: 'direct' } });
    expect(mcp.owner(ctx)).toEqual({ by: 'user', enabled: true });
    expect(configuredOctocodeServer(cwd, false, agentDir)).toEqual(global);
    writeProject({ command: 'replacement' });
    expect(configuredOctocodeServer(cwd, true, agentDir)).toEqual({ command: 'replacement' });
    writeProject({ enabled: true, env: { TEST: 'invalid override' } });
    expect(configuredOctocodeServer(cwd, true, agentDir)).toEqual(global);
    fs.unlinkSync(path.join(agentDir, 'mcp.json'));
    writeProject({ enabled: false });
    expect(configuredOctocodeServer(cwd, true, agentDir)).toBeUndefined();
  });


  it('is owned by the user\'s mcp.json entry, else by this extension, unless OCTOCODE_MCP=0', () => {
    const agentDir = tmp();
    vi.stubEnv('PI_CODING_AGENT_DIR', agentDir);
    const cwd = tmp();
    const ctx = { cwd, isProjectTrusted: () => false };
    const owner = (env: NodeJS.ProcessEnv = {}) => new OctocodeMcp(fakePi().pi, { env }).owner(ctx);
    expect(owner()).toEqual({ by: 'extension' });
    expect(owner({ OCTOCODE_MCP: '0' })).toEqual({ by: 'off' });
    fs.writeFileSync(path.join(agentDir, 'mcp.json'), JSON.stringify({ mcpServers: { octocode: { command: 'octocode-mcp' } } }));
    expect(owner()).toEqual({ by: 'user', enabled: true });
    fs.writeFileSync(path.join(agentDir, 'mcp.json'), JSON.stringify({ mcpServers: { octocode: { command: 'false', enabled: false } } }));
    // The user's entry wins even over OCTOCODE_MCP=0, and `enabled: false` is the user's way to turn Octocode off.
    expect(owner({ OCTOCODE_MCP: '0' })).toEqual({ by: 'user', enabled: false });
    expect(new OctocodeMcp(fakePi().pi).owner({ cwd, isProjectTrusted: undefined as never })).toEqual({ by: 'user', enabled: false });
  });

  it('starts nothing for a user-owned or switched-off server, and says so', async () => {
    const launch = vi.fn((cwd: string) => echo(cwd));
    const agentDir = tmp();
    vi.stubEnv('PI_CODING_AGENT_DIR', agentDir);
    const off = session(launch, { env: { OCTOCODE_MCP: '0' } });
    await off.mcp.start(off.ctx);
    expect(await off.mcp.ready(off.ctx)).toBe(false);
    expect(off.mcp.describe()).toBe('Octocode MCP: off (OCTOCODE_MCP=0)');
    fs.writeFileSync(path.join(agentDir, 'mcp.json'), JSON.stringify({ mcpServers: { octocode: { command: 'octocode-mcp' } } }));
    const own = session(launch);
    await own.mcp.start(own.ctx);
    expect(own.mcp.describe()).toContain('your own `octocode` server from mcp.json');
    expect(own.mcp.instructions).toBeUndefined();
    expect(launch).not.toHaveBeenCalled();
    expect(own.fake.tools.size).toBe(0);
  });
});

describe('Octocode MCP client', () => {
  it('connects at session start, registers every listed tool directly, and keeps the instructions', async () => {
    const { fake, ctx, mcp, call } = session(echo);
    await mcp.start(ctx);
    expect(await mcp.ready(ctx)).toBe(true);
    const names = [...fake.tools.keys()].sort();
    expect(names).toEqual(['crash', 'exit', 'fail', 'grow', 'localFetch', 'pick', 'roots', 'shout', 'shrink'].map((name) => `mcp__octocode__${name}`));
    const shout = fake.tools.get('mcp__octocode__shout');
    expect(shout).toMatchObject({ description: 'Uppercase the given text', exposure: 'direct', namespace: { name: 'mcp__octocode', instructions: 'Echo test server.' } });
    // The server's JSON Schema is the tool's parameter schema as is.
    expect(shout.parameters).toMatchObject({ type: 'object', properties: { text: { type: 'string' } }, required: ['text'] });
    expect(mcp.instructions).toBe('Echo test server.');
    expect(text(await call('shout', { text: 'hello' }))).toBe('HELLO');
    const status = mcp.status();
    expect(status).toMatchObject({ state: 'connected', serverName: 'echo', serverVersion: '1.0.0', tools: 9, cwd: ctx.cwd });
    expect(mcp.describe()).toBe(`Octocode MCP: connected (echo 1.0.0, 9 tools, pid ${status.pid})`);
    await mcp.close();
    expect(alive(status.pid!)).toBe(false);
    expect(mcp.status().state).toBe('closed');
  });

  it('returns a server error as an error result and honours an aborted call', async () => {
    const { ctx, mcp, call } = session(echo);
    await mcp.start(ctx);
    await mcp.ready(ctx);
    expect(await call('fail')).toMatchObject({ isError: true, content: [{ type: 'text', text: 'boom' }] });
    const aborted = new AbortController();
    aborted.abort();
    await expect(call('shout', { text: 'x' }, aborted.signal)).rejects.toThrow();
    await mcp.close();
  });

  it('follows tools/list_changed: a new tool registers, a withdrawn one is hidden', async () => {
    const { fake, ctx, mcp, call } = session(echo);
    const registered = vi.spyOn(fake.pi, 'registerTool');
    await mcp.start(ctx);
    await mcp.ready(ctx);
    const shout = fake.tools.get('mcp__octocode__shout');
    registered.mockClear();
    await call('grow');
    await vi.waitFor(() => expect(fake.tools.get('mcp__octocode__late')).toMatchObject({ exposure: 'direct' }));
    expect(mcp.status().tools).toBe(10);
    expect(registered).toHaveBeenCalledTimes(1);
    expect(fake.tools.get('mcp__octocode__shout')).toBe(shout);
    registered.mockClear();
    // A tool the server withdraws is hidden from the model, not left callable.
    await call('shrink');
    await vi.waitFor(() => expect(fake.tools.get('mcp__octocode__late')).toMatchObject({ exposure: 'hidden' }));
    expect(mcp.status().tools).toBe(9);
    expect(registered).toHaveBeenCalledTimes(1);
    expect(fake.tools.get('mcp__octocode__shout')).toBe(shout);
    // Reintroducing a withdrawn tool makes it callable again without replacing the unchanged tools.
    registered.mockClear();
    await call('grow');
    await vi.waitFor(() => expect(fake.tools.get('mcp__octocode__late')).toMatchObject({ exposure: 'direct' }));
    expect(registered).toHaveBeenCalledTimes(1);
    expect(text(await call('late'))).toBe('late tool works');
    await mcp.close();
  });

  it('keeps unchanged definitions on reconnect and refreshes changed tools or namespace instructions', async () => {
    let instructions = 'First instructions';
    let description = 'Uppercase the given text';
    const { fake, ctx, mcp, call } = session((cwd) => ({ ...echo(cwd), env: { ECHO_INSTRUCTIONS: instructions, ECHO_SHOUT_DESCRIPTION: description } }));
    const registered = vi.spyOn(fake.pi, 'registerTool');
    try {
      await mcp.start(ctx);
      await mcp.ready(ctx);
      const shout = fake.tools.get('mcp__octocode__shout');
      registered.mockClear();
      await mcp.restart(ctx);
      expect(registered).not.toHaveBeenCalled();
      expect(fake.tools.get('mcp__octocode__shout')).toBe(shout);
      expect(text(await call('shout', { text: 'reconnected' }))).toBe('RECONNECTED');
      description = 'Updated uppercase description';
      await mcp.restart(ctx);
      expect(registered).toHaveBeenCalledTimes(1);
      expect(fake.tools.get('mcp__octocode__shout').description).toBe(description);
      registered.mockClear();
      instructions = 'New instructions';
      await mcp.restart(ctx);
      expect(registered).toHaveBeenCalledTimes(mcp.status().tools);
      expect(fake.tools.get('mcp__octocode__shout').namespace.instructions).toBe(instructions);
    } finally {
      await mcp.close();
    }
  });

  it('reconnects once after the process dies, then reports a clear error until restart', async () => {
    let launches = 0;
    const { ctx, mcp, call } = session((cwd) => (++launches === 2 ? echo(cwd) : launches === 1 ? echo(cwd) : exits(cwd)));
    await mcp.start(ctx);
    await mcp.ready(ctx);
    const first = mcp.status().pid!;
    expect(text(await call('exit'))).toBe('bye');
    await vi.waitFor(() => expect(mcp.status().state).toBe('disconnected'));
    expect(mcp.describe()).toContain('Octocode MCP: stopped');
    // The next call reconnects once and carries on.
    expect(text(await call('shout', { text: 'again' }))).toBe('AGAIN');
    expect(launches).toBe(2);
    expect(mcp.status().pid).not.toBe(first);
    // Dies again; the one reconnect fails: a clear error, and no further attempts until restart.
    await call('exit');
    await vi.waitFor(() => expect(mcp.status().state).toBe('disconnected'));
    await expect(call('shout', { text: 'x' })).rejects.toThrow(/Octocode MCP server is not available: .*kaboom/s);
    expect(launches).toBe(3);
    await expect(call('shout', { text: 'x' })).rejects.toThrow('not available');
    expect(launches).toBe(3);
    expect(mcp.status().state).toBe('failed');
    launches = 1;
    await mcp.restart(ctx);
    expect(text(await call('shout', { text: 'back' }))).toBe('BACK');
    await mcp.close();
  });

  it('shows controls in the server\'s instructions as visible escapes, never dropped', async () => {
    const { fake, ctx, mcp } = session((cwd) => ({ ...echo(cwd), env: { ECHO_INSTRUCTIONS: 'Use it.\u001b]0;pwned\u0007\u202e' } }));
    await mcp.start(ctx);
    expect(await mcp.ready(ctx)).toBe(true);
    const expected = 'Use it.\\u{1b}]0;pwned\\u{7}\\u{202e}';
    expect(mcp.instructions).toBe(expected);
    expect(fake.tools.get('mcp__octocode__shout').namespace.instructions).toBe(expected);
    await mcp.close();
  });

  it('registers tools without a namespace instructions block when the server has none', async () => {
    const { fake, ctx, mcp } = session(echo, {});
    const quiet = (cwd: string) => ({ ...echo(cwd), env: { ECHO_INSTRUCTIONS: '' } });
    const silent = new OctocodeMcp(fake.pi, { launch: quiet, env: {} });
    await silent.start(ctx);
    expect(await silent.ready(ctx)).toBe(true);
    expect(silent.instructions).toBeUndefined();
    expect(fake.tools.get('mcp__octocode__shout').namespace).toEqual({ name: 'mcp__octocode' });
    await Promise.all([silent.close(), mcp.close()]);
  });

  it('describes every state it can be in', () => {
    const mcp = new OctocodeMcp(fakePi().pi, { env: {} });
    const say = (status: object) => {
      (mcp as unknown as { server: { status: () => object } }).server.status = () => status;
      (mcp as unknown as { owned: unknown }).owned = { by: 'extension' };
      return mcp.describe();
    };
    expect(say({ state: 'idle', tools: 0 })).toContain('not started');
    expect(say({ state: 'connected', tools: 2 })).toBe('Octocode MCP: connected (octocode-mcp, 2 tools, pid ?)');
    expect(say({ state: 'failed', tools: 0 })).toContain('failed to start: unknown error');
    expect(say({ state: 'disconnected', tools: 0 })).toContain('stopped: the server exited');
    expect(say({ state: 'closed', tools: 0 })).toContain('not started');
    expect(say({ state: 'connected', tools: 1, serverName: 'x', pid: 7 })).toBe('Octocode MCP: connected (x, 1 tools, pid 7)');
  });
});
