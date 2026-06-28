import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { afterEach, describe, expect, it } from 'vitest';
import { LSPClient } from '../../src/lsp/client.js';
import type { LanguageServerConfig } from '../../src/lsp/types.js';

// Minimal LSP server that speaks JSON-RPC and controls $/progress timing.
// PROGRESS_DELAY_MS env: ms before sending $/progress end; -1 = never send end.
const MOCK_LSP_SERVER = `
const delay = parseInt(process.env.PROGRESS_DELAY_MS ?? '0', 10);
let buf = '';
process.stdin.setEncoding('binary');
function send(m) {
  const body = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(body, 'utf8') + '\\r\\n\\r\\n' + body);
}
function progress(kind) {
  send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'indexing', value: kind === 'begin' ? { kind, title: 'Indexing' } : { kind } } });
}
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: {} } });
  } else if (msg.method === 'initialized') {
    progress('begin');
    if (delay >= 0) setTimeout(() => progress('end'), delay);
  } else if (msg.method === 'shutdown') {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
  }
}
process.stdin.on('data', d => {
  buf += d;
  for (;;) {
    const m = buf.match(/Content-Length: (\\d+)\\r\\n\\r\\n/);
    if (!m) break;
    const len = +m[1], start = m.index + m[0].length;
    if (buf.length < start + len) break;
    try { handle(JSON.parse(buf.slice(start, start + len))); } catch {}
    buf = buf.slice(start + len);
  }
});
process.stdin.resume();
`;

const tempDirs: string[] = [];

afterEach(async () => {
  while (tempDirs.length > 0)
    await rm(tempDirs.pop()!, { recursive: true, force: true });
});

async function fixture(): Promise<{
  root: string;
  file: string;
  config: LanguageServerConfig;
}> {
  const root = await mkdtemp(
    path.join(os.tmpdir(), 'octocode-engine-native-client-')
  );
  tempDirs.push(root);
  const file = path.join(root, 'demo.unknown');
  await writeFile(file, 'plain\n');
  return {
    root,
    file,
    config: {
      command: process.execPath,
      args: ['-e', 'process.stdin.resume()'],
      workspaceRoot: root,
      languageId: 'plaintext',
    },
  };
}

/**
 * Creates a temp dir + a mock LSP server script that sends $/progress begin
 * after initialized, then $/progress end after `progressDelayMs` ms.
 * Pass -1 to never send the end notification (simulates a hung indexer).
 */
async function progressFixture(progressDelayMs: number): Promise<{
  root: string;
  serverScript: string;
  config: LanguageServerConfig;
}> {
  const root = await mkdtemp(path.join(os.tmpdir(), 'octocode-engine-progress-'));
  tempDirs.push(root);
  const serverScript = path.join(root, 'mock-lsp-server.cjs');
  await writeFile(serverScript, MOCK_LSP_SERVER);
  return {
    root,
    serverScript,
    config: {
      command: process.execPath,
      args: [serverScript],
      workspaceRoot: root,
      languageId: 'typescript',
      env: { PROGRESS_DELAY_MS: String(progressDelayMs) },
    },
  };
}

describe('LSPClient native wrapper', () => {
  it('constructs and exposes lifecycle methods without TypeScript JSON-RPC internals', async () => {
    const { config } = await fixture();
    const client = new LSPClient(config);

    expect(client.hasCapability('definitionProvider')).toBe(false);
    await expect(client.stop()).resolves.toBeUndefined();
    expect(client.getRecentStderr()).toEqual([]);
  });

  it('routes document operations through the native client', async () => {
    const { config, file } = await fixture();
    const client = new LSPClient(config);

    await expect(client.openDocument(file, 'plain\n')).rejects.toThrow(
      'LSP client not initialized'
    );
    await expect(client.closeDocument(file)).resolves.toBeUndefined();
  });

  it('refuses to start a rejected shell-wrapper command before spawning', async () => {
    const root = await mkdtemp(
      path.join(os.tmpdir(), 'octocode-engine-shell-reject-')
    );
    tempDirs.push(root);
    const client = new LSPClient({
      command: 'bash',
      args: [],
      workspaceRoot: root,
      languageId: 'plaintext',
    });
    await expect(client.start()).rejects.toThrow(
      /Refusing to start language server/
    );
  });

  it('refuses to start a nonexistent absolute server path', async () => {
    const root = await mkdtemp(
      path.join(os.tmpdir(), 'octocode-engine-missing-reject-')
    );
    tempDirs.push(root);
    const client = new LSPClient({
      command: path.join(root, 'no-such-server'),
      args: [],
      workspaceRoot: root,
      languageId: 'plaintext',
    });
    await expect(client.start()).rejects.toThrow(
      /Refusing to start language server/
    );
  });

  it('fails startup promptly and preserves recent stderr when the server exits', async () => {
    const root = await mkdtemp(
      path.join(os.tmpdir(), 'octocode-engine-start-fail-')
    );
    tempDirs.push(root);
    const client = new LSPClient({
      command: process.execPath,
      args: [
        '-e',
        'console.error("octocode startup failure"); process.exit(1)',
      ],
      workspaceRoot: root,
      languageId: 'plaintext',
    });
    const startedAt = Date.now();

    await expect(client.start()).rejects.toThrow(/LSP connection closed/);

    expect(Date.now() - startedAt).toBeLessThan(5_000);
    await waitFor(() =>
      client.getRecentStderr().some(line => line.includes('startup failure'))
    );
    await expect(client.stop()).resolves.toBeUndefined();
  });
});

describe('waitForReady $/progress tracking', () => {
  it('resolves early when all $/progress tokens complete before timeout', async () => {
    // Server sends $/progress end after 150ms; timeout is 10s — should resolve ~150ms
    const { config } = await progressFixture(150);
    const client = new LSPClient(config);
    await client.start();

    const t0 = Date.now();
    await client.waitForReady(10_000);
    const elapsed = Date.now() - t0;

    // Must finish well before the 10s timeout — allow up to 3s for CI slowness
    expect(elapsed).toBeLessThan(3_000);
    await client.stop();
  });

  it('falls back to the full timeout when $/progress end never arrives', async () => {
    // Server sends $/progress begin but never end (-1 = never)
    const { config } = await progressFixture(-1);
    const client = new LSPClient(config);
    await client.start();

    const t0 = Date.now();
    await client.waitForReady(400); // short timeout so test stays fast
    const elapsed = Date.now() - t0;

    // Should have waited at least as long as the timeout
    expect(elapsed).toBeGreaterThanOrEqual(350);
    await client.stop();
  });

  it('resolves immediately when $/progress completes before waitForReady is called', async () => {
    // Server sends $/progress end immediately (0ms delay)
    const { config } = await progressFixture(0);
    const client = new LSPClient(config);
    await client.start();

    // Give the progress end time to arrive before we call waitForReady
    await delay(300);

    const t0 = Date.now();
    await client.waitForReady(5_000);
    const elapsed = Date.now() - t0;

    // Already settled — should resolve essentially immediately
    expect(elapsed).toBeLessThan(500);
    await client.stop();
  });
});

async function waitFor(predicate: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 50; attempt++) {
    if (predicate()) return;
    await delay(10);
  }
  throw new Error('condition was not met');
}
