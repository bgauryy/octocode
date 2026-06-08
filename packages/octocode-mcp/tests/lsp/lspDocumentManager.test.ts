import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { MessageConnection } from 'vscode-jsonrpc/node.js';
import { TextDocumentSyncKind } from 'vscode-languageserver-protocol';

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

  it('uses the opened file extension language ID instead of the pooled client language ID', async () => {
    const tsxPath = path.join(tempDir, 'component.tsx');
    await writeFile(tsxPath, 'export const Component = () => null;\n', 'utf8');

    await manager.openDocument(tsxPath);

    expect(connection.sendNotification).toHaveBeenCalledWith(
      'textDocument/didOpen',
      expect.objectContaining({
        textDocument: expect.objectContaining({
          uri: toUri(tsxPath),
          languageId: 'typescriptreact',
        }),
      })
    );
  });

  it('falls back to the pooled client language ID for plaintext extensions', async () => {
    const plaintextPath = path.join(tempDir, 'notes.unknown');
    await writeFile(plaintextPath, 'plain text\n', 'utf8');

    await manager.openDocument(plaintextPath);

    expect(connection.sendNotification).toHaveBeenCalledWith(
      'textDocument/didOpen',
      expect.objectContaining({
        textDocument: expect.objectContaining({
          uri: toUri(plaintextPath),
          languageId: 'typescript',
        }),
      })
    );
  });

  it('syncs changed on-disk content for an already open document', async () => {
    await manager.openDocument(filePath);
    await writeFile(filePath, 'export const answer = 43;\n', 'utf8');
    vi.mocked(connection.sendNotification).mockClear();

    await manager.openDocument(filePath);

    expect(connection.sendNotification).toHaveBeenCalledWith(
      'textDocument/didChange',
      expect.objectContaining({
        textDocument: expect.objectContaining({
          uri: toUri(filePath),
          version: 2,
        }),
        contentChanges: [{ text: 'export const answer = 43;\n' }],
      })
    );
    expect(manager.getOpenDocumentRefCount(filePath)).toBe(2);
  });

  it('uses whole-document replacement ranges for incremental sync', async () => {
    await manager.openDocument(filePath);
    manager.setTextDocumentSync({
      openClose: true,
      change: TextDocumentSyncKind.Incremental,
    });
    await writeFile(filePath, 'export const answer = 44;\n', 'utf8');
    vi.mocked(connection.sendNotification).mockClear();

    await manager.openDocument(filePath);

    expect(connection.sendNotification).toHaveBeenCalledWith(
      'textDocument/didChange',
      expect.objectContaining({
        contentChanges: [
          expect.objectContaining({
            range: {
              start: { line: 0, character: 0 },
              end: { line: 1, character: 0 },
            },
            text: 'export const answer = 44;\n',
          }),
        ],
      })
    );
  });

  it('reopens changed documents when the server only supports open/close sync', async () => {
    await manager.openDocument(filePath);
    manager.setTextDocumentSync(TextDocumentSyncKind.None);
    await writeFile(filePath, 'export const answer = 45;\n', 'utf8');
    vi.mocked(connection.sendNotification).mockClear();

    await manager.openDocument(filePath);

    expect(connection.sendNotification).toHaveBeenCalledWith(
      'textDocument/didClose',
      { textDocument: { uri: toUri(filePath) } }
    );
    expect(connection.sendNotification).toHaveBeenCalledWith(
      'textDocument/didOpen',
      expect.objectContaining({
        textDocument: expect.objectContaining({
          uri: toUri(filePath),
          version: 2,
          text: 'export const answer = 45;\n',
        }),
      })
    );
  });

  it('rejects changed documents when the server supports neither changes nor open/close', async () => {
    await manager.openDocument(filePath);
    manager.setTextDocumentSync({
      openClose: false,
      change: TextDocumentSyncKind.None,
    });
    await writeFile(filePath, 'export const answer = 46;\n', 'utf8');

    await expect(manager.openDocument(filePath)).rejects.toThrow(
      'Language server does not support document changes'
    );
  });

  it('rejects new documents when the server does not support open/close', async () => {
    manager.setTextDocumentSync({
      openClose: false,
      change: TextDocumentSyncKind.Full,
    });

    await expect(manager.openDocument(filePath)).rejects.toThrow(
      'Language server does not support opening documents'
    );
  });

  it('syncs provided content without incrementing refCount', async () => {
    await manager.ensureDocumentSynced(filePath, {
      content: 'export const answer = 47;\n',
    });

    expect(connection.sendNotification).toHaveBeenCalledWith(
      'textDocument/didOpen',
      expect.objectContaining({
        textDocument: expect.objectContaining({
          text: 'export const answer = 47;\n',
        }),
      })
    );
    expect(manager.getOpenDocumentRefCount(filePath)).toBe(0);
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

  it('ignores closeDocument when disconnected or when the document is unknown', async () => {
    await manager.closeDocument(filePath);
    expect(connection.sendNotification).not.toHaveBeenCalled();

    const disconnected = new LSPDocumentManager({
      command: 'typescript-language-server',
      args: ['--stdio'],
      workspaceRoot: tempDir,
      languageId: 'typescript',
    });
    await expect(disconnected.closeDocument(filePath)).resolves.toBeUndefined();
  });

  it('removes documents from tracking even when force-close notifications fail', async () => {
    await manager.openDocument(filePath);
    vi.mocked(connection.sendNotification).mockClear();
    vi.mocked(connection.sendNotification).mockRejectedValueOnce(
      new Error('close failed')
    );

    await manager.closeAllDocuments();

    expect(manager.isDocumentOpen(filePath)).toBe(false);
  });
});
