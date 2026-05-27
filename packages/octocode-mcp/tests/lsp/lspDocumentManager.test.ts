import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { MessageConnection } from 'vscode-jsonrpc/node.js';

import { LSPDocumentManager } from '../../src/lsp/lspDocumentManager.js';
import { toUri } from '../../src/lsp/uri.js';

function createConnection(): MessageConnection {
  return {
    sendNotification: vi.fn(async () => undefined),
  } as unknown as MessageConnection;
}

describe('LSPDocumentManager document ref-counting', () => {
  let tempDir: string;
  let filePath: string;
  let connection: MessageConnection;
  let manager: LSPDocumentManager;

  beforeEach(async () => {
    tempDir = await mkdtemp(path.join(tmpdir(), 'octocode-lsp-doc-'));
    filePath = path.join(tempDir, 'example.ts');
    await writeFile(filePath, 'export const answer = 42;\n', 'utf8');
    connection = createConnection();
    manager = new LSPDocumentManager({
      command: 'typescript-language-server',
      args: ['--stdio'],
      workspaceRoot: tempDir,
      languageId: 'typescript',
    });
    manager.setConnection(connection, true);
  });

  afterEach(async () => {
    await rm(tempDir, { recursive: true, force: true });
  });

  it('increments refCount instead of sending duplicate didOpen', async () => {
    await manager.openDocument(filePath);
    await manager.openDocument(filePath);

    expect(connection.sendNotification).toHaveBeenCalledTimes(1);
    expect(connection.sendNotification).toHaveBeenCalledWith(
      'textDocument/didOpen',
      expect.objectContaining({
        textDocument: expect.objectContaining({ uri: toUri(filePath) }),
      })
    );
    expect(manager.getOpenDocumentRefCount(filePath)).toBe(2);
  });

  it('does not send didClose while another reference is still active', async () => {
    await manager.openDocument(filePath);
    await manager.openDocument(filePath);

    await manager.closeDocument(filePath);

    expect(connection.sendNotification).not.toHaveBeenCalledWith(
      'textDocument/didClose',
      expect.any(Object)
    );
    expect(manager.isDocumentOpen(filePath)).toBe(true);
    expect(manager.getOpenDocumentRefCount(filePath)).toBe(1);
  });

  it('sends didClose only for the final close', async () => {
    await manager.openDocument(filePath);
    await manager.openDocument(filePath);

    await manager.closeDocument(filePath);
    await manager.closeDocument(filePath);

    expect(connection.sendNotification).toHaveBeenCalledWith(
      'textDocument/didClose',
      { textDocument: { uri: toUri(filePath) } }
    );
    expect(manager.isDocumentOpen(filePath)).toBe(false);
    expect(manager.getOpenDocumentRefCount(filePath)).toBe(0);
  });

  it('closeAllDocuments force-closes tracked documents once regardless of refCount', async () => {
    await manager.openDocument(filePath);
    await manager.openDocument(filePath);

    vi.mocked(connection.sendNotification).mockClear();
    await manager.closeAllDocuments();

    expect(connection.sendNotification).toHaveBeenCalledTimes(1);
    expect(connection.sendNotification).toHaveBeenCalledWith(
      'textDocument/didClose',
      { textDocument: { uri: toUri(filePath) } }
    );
    expect(manager.isDocumentOpen(filePath)).toBe(false);
  });
});
