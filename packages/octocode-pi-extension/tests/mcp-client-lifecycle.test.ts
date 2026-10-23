import { describe, expect, it, vi } from 'vitest';
import { registerMcpCommand } from '../src/mcp/command.js';
import { OctocodeMcp, registerOctocodeMcp } from '../src/mcp/octocode.js';
import { Subcommands } from '../src/shared/commands.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';
import { alive, echo, exits, session, text } from './mcp-session.js';

const hangs = (cwd: string) => ({ command: process.execPath, args: ['-e', 'setTimeout(() => {}, 60000)'], cwd, env: {} });

describe('Octocode MCP client', () => {
  it('says so when the server dies during a call, and starts it again for the next one', async () => {
    const { ctx, mcp, call } = session(echo);
    await mcp.start(ctx);
    await mcp.ready(ctx);
    await expect(call('crash')).rejects.toThrow('Octocode MCP server stopped during the call');
    expect(text(await call('shout', { text: 'next' }))).toBe('NEXT');
    await mcp.close();
  });

  it('shows terminal escape sequences in the server stderr as visible escapes in the failure text and the notice', async () => {
    const { ctx, mcp, notes } = session((cwd) => exits(cwd, 'boom \u001b]0;pwned\u0007\u001b[31mred\u001b[0m\u202e!'));
    await mcp.start(ctx);
    expect(await mcp.ready(ctx)).toBe(false);
    const error = mcp.status().error!;
    expect(error).toContain('boom \\u{1b}]0;pwned\\u{7}\\u{1b}[31mred\\u{1b}[0m\\u{202e}!');
    for (const control of ['\u001b', '\u0007', '\u202e']) {
      expect(error).not.toContain(control);
      expect(notes[0].message).not.toContain(control);
    }
  });

  it('reports a failed connect at once: notifies once, retries once on a later ready, then stays failed', async () => {
    const launch = vi.fn((cwd: string) => exits(cwd));
    const { fake, ctx, mcp, notes } = session(launch);
    await mcp.start(ctx);
    const started = Date.now();
    expect(await mcp.ready(ctx)).toBe(false);
    expect(Date.now() - started).toBeLessThan(10_000);
    expect(mcp.status()).toMatchObject({ state: 'failed', tools: 0 });
    expect(mcp.status().error).toContain('kaboom');
    expect(notes).toHaveLength(1);
    expect(notes[0].message).toContain('Octocode MCP failed to start');
    expect(notes[0].message).toContain('kaboom');
    expect(mcp.describe()).toContain('failed to start');
    expect(mcp.instructions).toBeUndefined();
    // A later prompt retries once; it fails again without another notice, and the one after does not even launch.
    expect(await mcp.ready(ctx)).toBe(false);
    expect(await mcp.ready(ctx)).toBe(false);
    expect(launch).toHaveBeenCalledTimes(2);
    expect(notes).toHaveLength(1);
    expect(fake.tools.size).toBe(0);
  });

  it('stays quiet without a UI', async () => {
    const { ctx, mcp, notes } = session(exits, { hasUI: false });
    await mcp.start(ctx);
    expect(await mcp.ready(ctx)).toBe(false);
    expect(notes).toHaveLength(0);
  });
});

describe('Octocode MCP client session lifecycle', () => {
  it('waits for a slow connect no longer than its bound, and ends it with the session', async () => {
    const { ctx, mcp } = session(hangs, { connectWaitMs: 0 });
    await mcp.start(ctx);
    expect(await mcp.ready(ctx)).toBe(false);
    expect(mcp.status().state).toBe('connecting');
    expect(mcp.describe()).toBe('Octocode MCP: connecting…');
    const done = new AbortController();
    done.abort();
    expect(await mcp.ready({ ...ctx, signal: done.signal })).toBe(false);
    const aborted = new AbortController();
    const waiting = session(hangs, { connectWaitMs: 30_000 });
    await waiting.mcp.start(waiting.ctx);
    const pending = waiting.mcp.ready({ ...waiting.ctx, signal: aborted.signal });
    aborted.abort();
    expect(await pending).toBe(false);
    await Promise.all([mcp.close(), waiting.mcp.close()]);
    expect(mcp.status().state).toBe('closed');
  });

  it('moves to the new folder when the session folder changes, and a restart gets a fresh process', async () => {
    const folders: string[] = [];
    const { ctx, mcp, call } = session((cwd) => (folders.push(cwd), echo(cwd)));
    await mcp.start(ctx);
    await mcp.ready(ctx);
    const first = mcp.status().pid!;
    await mcp.start(ctx);
    expect(mcp.status().pid).toBe(first);
    const other = { ...ctx, cwd: tmp() } as typeof ctx;
    await mcp.start(other);
    expect(await mcp.ready(other)).toBe(true);
    expect(folders).toEqual([ctx.cwd, other.cwd]);
    await vi.waitFor(() => expect(alive(first)).toBe(false));
    const second = mcp.status().pid!;
    expect(text(await call('shout', { text: 'ok' }))).toBe('OK');
    expect((await mcp.restart(other)).pid).not.toBe(second);
    await vi.waitFor(() => expect(alive(second)).toBe(false));
    await mcp.close();
    expect(mcp.status().state).toBe('closed');
  });

  it('wires start and shutdown to the session', async () => {
    const fake = fakePi();
    const mcp = registerOctocodeMcp(fake.pi);
    expect(mcp).toBeInstanceOf(OctocodeMcp);
    const ctx = fakeCtx({ cwd: tmp() });
    vi.stubEnv('OCTOCODE_MCP', '0');
    await fake.emit('session_start', {}, ctx);
    expect(mcp.describe()).toContain('off');
    await fake.emit('session_shutdown', {}, ctx);
    expect(mcp.status().state).toBe('closed');
  });
});

describe('/octocode mcp', () => {
  it('shows the status and restarts the server', async () => {
    const { ctx, mcp } = session(echo);
    const commands = new Subcommands();
    registerMcpCommand(commands, mcp);
    const run = (args: string) => commands.get('mcp')!.handler(args, ctx);
    await mcp.start(ctx);
    await mcp.ready(ctx);
    const pid = mcp.status().pid;
    await run('');
    expect(ctx.ui.notes.at(-1)!.message).toBe(`Octocode MCP: connected (echo 1.0.0, 9 tools, pid ${pid})`);
    await run('restart');
    const restarted = ctx.ui.notes.at(-1)!.message;
    expect(restarted).toMatch(/^Octocode MCP: connected/);
    expect(restarted).not.toContain(`pid ${pid})`);
    await run('bogus');
    expect(ctx.ui.notes.at(-1)).toMatchObject({ type: 'warning', message: 'Usage: /octocode mcp [status|restart]' });
    expect(commands.get('mcp')!.complete!('r')).toEqual([{ value: 'restart', label: 'restart', description: 'stop and start the server again' }]);
    await mcp.close();
    // Nothing to restart when the user's own server or nobody runs it.
    vi.stubEnv('OCTOCODE_MCP', '0');
    const off = new OctocodeMcp(fakePi().pi, { env: { OCTOCODE_MCP: '0' } });
    registerMcpCommand(commands, off);
    await commands.get('mcp')!.handler('restart', ctx);
    expect(ctx.ui.notes.at(-1)).toMatchObject({ type: 'warning' });
    expect(ctx.ui.notes.at(-1)!.message).toContain('Nothing to restart');
  });
});

