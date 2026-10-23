import http from 'node:http';
import { afterEach, beforeEach, vi } from 'vitest';
import { pageFetcher, type PageFetcher } from '../src/web/fetch.js';
import { registerWebTool } from '../src/web/web.js';
import type { Permission } from '../src/permission/ask.js';
import { addressOf, toolContext } from './doubles.js';
import { fakeCtx, fakePermission, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

export type ToolResult = { content: Array<{ type: string; text?: string; data?: string; mimeType?: string }>; details?: Record<string, unknown> };
export type WebTool = {
  execute: (id: string, params: Record<string, unknown>, signal?: AbortSignal) => Promise<ToolResult>;
  renderCall: (args: Record<string, unknown>, theme: unknown, context: Record<string, unknown>) => { render(width: number): string[] };
  renderResult: (result: Record<string, unknown>, options: Record<string, unknown>, theme: unknown, context: Record<string, unknown>) => { render(width: number): string[] };
};

const servers: http.Server[] = [];
/** Each case reads through its own fetcher, so no case sees a page another one cached (the tool makes its own per registration). */
export let fetchUrl: PageFetcher;

/** Gives each test of the calling file a fresh fetcher, and closes its servers and undoes its env and global stubs after it. */
export function useWebServers(): void {
  beforeEach(() => {
    fetchUrl = pageFetcher();
  });
  afterEach(async () => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
    await Promise.all(servers.splice(0).map((server) => new Promise((resolve) => server.close(resolve))));
  });
}

export async function serve(handler: http.RequestListener): Promise<string> {
  const server = http.createServer(handler);
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  return `http://127.0.0.1:${addressOf(server).port}`;
}

export const privateOk = { private: true };

export function webTool(active: string[] = [], permission: Permission = fakePermission('full'), model?: unknown): WebTool {
  const fake = fakePi();
  fake.pi.setActiveTools(active);
  registerWebTool(fake.pi, permission);
  const definition = fake.tools.get('web');
  // The tool asks through the call's context: a local one, with no UI, so only the mode decides.
  const execute: WebTool['execute'] = (id, params, signal) => definition.execute(id, params, signal, undefined, toolContext(fakeCtx({ cwd: tmp(), model })));
  return { ...definition, execute };
}

export const text = (result: ToolResult) => result.content.map((part) => part.text ?? '').join('\n');
