import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import { createMcpConnectionManager, retryMcpStartup } from '../src/tools/mcp/connection-manager.js';
import type { PiContext } from '../src/types.js';

const MCP_SERVER_ENTRY = import.meta.resolve('@modelcontextprotocol/server');
const MCP_STDIO_ENTRY = import.meta.resolve('@modelcontextprotocol/server/stdio');
const roots: string[] = [];

afterEach(() => {
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

function createRetryingServer(): { root: string; serverPath: string; attemptsPath: string; ctx: PiContext } {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-retry-manager-'));
  roots.push(root);
  const serverPath = path.join(root, 'server.mjs');
  const attemptsPath = path.join(root, 'attempts.txt');
  fs.writeFileSync(serverPath, `
    import fs from 'node:fs';
    import { Server } from ${JSON.stringify(MCP_SERVER_ENTRY)};
    import { StdioServerTransport } from ${JSON.stringify(MCP_STDIO_ENTRY)};
    const attemptsPath = ${JSON.stringify(attemptsPath)};
    const attempts = fs.existsSync(attemptsPath) ? Number(fs.readFileSync(attemptsPath, 'utf8')) + 1 : 1;
    fs.writeFileSync(attemptsPath, String(attempts));
    if (attempts === 1) {
      process.stderr.write('transient startup failure\\n');
      process.exit(1);
    }
    const server = new Server({ name: 'retry-fixture', version: '1.0.0' }, { capabilities: {} });
    await server.connect(new StdioServerTransport());
  `);
  return {
    root,
    serverPath,
    attemptsPath,
    ctx: { cwd: root, isProjectTrusted: () => true } as unknown as PiContext,
  };
}

test('startup retry helper is bounded and succeeds on a later attempt', async () => {
  let attempts = 0;
  const value = await retryMcpStartup(async () => {
    attempts += 1;
    if (attempts < 3) throw new Error(`transient-${attempts}`);
    return 'connected';
  }, 2, 1);
  assert.equal(value, 'connected');
  assert.equal(attempts, 3);

  attempts = 0;
  await assert.rejects(
    retryMcpStartup(async () => {
      attempts += 1;
      throw new Error('still-down');
    }, 1, 1),
    /still-down/,
  );
  assert.equal(attempts, 2);

  const controller = new AbortController();
  attempts = 0;
  const cancelled = retryMcpStartup(async () => {
    attempts += 1;
    throw new Error('retryable');
  }, 5, 10_000, controller.signal);
  controller.abort(new Error('cancelled-by-test'));
  await assert.rejects(cancelled, /cancelled-by-test/);
  assert.equal(attempts, 1);
});

test('connection manager retries bounded startup failures and reports live ping health', async () => {
  const fixture = createRetryingServer();
  const background = new Set<Promise<unknown>>();
  const manager = createMcpConnectionManager({
    onCatalogChanged() {},
    onClientInvalidated() {},
    onServerInvalidated() {},
    trackAsyncWork(work) {
      background.add(work);
      void work.finally(() => background.delete(work));
      return work;
    },
  });

  const connection = await manager.ensure('retry-fixture', {
    command: process.execPath,
    args: [fixture.serverPath],
    cwd: fixture.root,
    startupRetries: 1,
    retryDelayMs: 1,
    startupTimeoutMs: 5_000,
  }, fixture.ctx);

  assert.equal(fs.readFileSync(fixture.attemptsPath, 'utf8'), '2');
  assert.equal(connection.health.status, 'healthy');
  const health = await manager.probeConnected(undefined, 1_000);
  assert.equal(health.length, 1);
  assert.equal(health[0]?.name, 'retry-fixture');
  assert.equal(health[0]?.status, 'healthy');
  assert.equal(typeof health[0]?.latencyMs, 'number');

  (connection.client as unknown as { ping(): Promise<unknown> }).ping = async () => {
    throw new Error('fixture ping failed');
  };
  const failedHealth = await manager.probeConnected(undefined, 1_000);
  assert.equal(failedHealth[0]?.status, 'unhealthy');
  assert.match(failedHealth[0]?.error ?? '', /fixture ping failed/);
  assert.equal(manager.isConnected('retry-fixture'), false, 'failed health probes evict the stale connection');
  assert.equal(await manager.stop('retry-fixture'), false);
  await Promise.allSettled(background);
});
