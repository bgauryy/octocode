import path from 'node:path';
import { OctocodeMcp } from '../src/mcp/octocode.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

const ECHO = path.join(import.meta.dirname, 'fixtures', 'echo-mcp.mjs');
export const echo = (cwd: string) => ({ command: process.execPath, args: [ECHO], cwd, env: {} });
export const exits = (cwd: string, stderr = 'kaboom') => ({ command: process.execPath, args: ['-e', `require('node:fs').writeSync(2, ${JSON.stringify(stderr)}); process.exit(3)`], cwd, env: {} });
export const alive = (pid: number) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

/** An Octocode MCP wired to a fake Pi and a stdio launch the test chooses. */
export function session(launch: (cwd: string) => ReturnType<typeof echo>, options: { env?: NodeJS.ProcessEnv; hasUI?: boolean; connectWaitMs?: number } = {}) {
  const fake = fakePi();
  const cwd = tmp();
  const ctx = fakeCtx({ cwd, hasUI: options.hasUI ?? true });
  const mcp = new OctocodeMcp(fake.pi, { launch, env: { ...options.env }, version: '1.2.3', connectWaitMs: options.connectWaitMs ?? 20_000 });
  const call = async (tool: string, params: Record<string, unknown> = {}, signal?: AbortSignal): Promise<{ content: Array<{ type: string; text?: string }>; isError?: boolean }> =>
    fake.tools.get(`mcp__octocode__${tool}`).execute('call-1', params, signal, undefined, ctx);
  return { fake, cwd, ctx, mcp, call, notes: ctx.ui.notes };
}
export const text = (result: { content: Array<{ text?: string }> }) => result.content.map((part) => part.text ?? '').join('');
