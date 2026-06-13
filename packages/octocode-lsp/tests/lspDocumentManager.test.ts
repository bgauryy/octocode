import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { TextDocumentSyncKind } from 'vscode-languageserver-protocol';
import { LSPDocumentManager } from '../src/lspDocumentManager.js';
import { toUri } from '../src/uri.js';
import type { LanguageServerConfig } from '../src/types.js';

const tempDirs: string[] = [];

afterEach(async () => {
  while (tempDirs.length > 0) {
    await rm(tempDirs.pop()!, { recursive: true, force: true });
  }
});

async function tempFile(name: string, content: string): Promise<string> {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'octocode-lsp-doc-'));
  tempDirs.push(dir);
  const file = path.join(dir, name);
  await writeFile(file, content);
  return file;
}

function config(root: string, languageId = 'foo'): LanguageServerConfig {
  return { command: 'server', args: [], workspaceRoot: root, languageId };
}

function configWithoutLanguageId(root: string): LanguageServerConfig {
  return { command: 'server', args: [], workspaceRoot: root };
}

function connection() {
  return {
    sendNotification: vi.fn().mockResolvedValue(undefined),
  };
}

describe('LSPDocumentManager', () => {
  it('opens, reference-counts, closes, and clears documents', async () => {
    const file = await tempFile('demo.foo', 'alpha\n');
    const conn = connection();
    const manager = new LSPDocumentManager(config(path.dirname(file)));
    const onDidOpen = vi.fn();
    manager.setOnDidOpen(onDidOpen);
    manager.setConnection(conn as never, true);

    await manager.openDocument(file);
    await manager.openDocument(file);

    expect(onDidOpen).toHaveBeenCalledTimes(1);
    expect(manager.isDocumentOpen(file)).toBe(true);
    expect(manager.getOpenDocumentRefCount(file)).toBe(2);
    expect(conn.sendNotification).toHaveBeenCalledWith(
      'textDocument/didOpen',
      expect.objectContaining({
        textDocument: expect.objectContaining({
          uri: toUri(file),
          languageId: 'foo',
          version: 1,
          text: 'alpha\n',
        }),
      })
    );

    await manager.closeDocument(file);
    expect(manager.isDocumentOpen(file)).toBe(true);
    await manager.closeDocument(file);
    expect(manager.isDocumentOpen(file)).toBe(false);

    await manager.openDocument(file);
    manager.setConnection(null, false);
    expect(manager.getOpenDocumentUris()).toEqual([]);
  });

  it('sends full, incremental, and reopen changes', async () => {
    const file = await tempFile('demo.ts', 'one\ntwo\n');
    const conn = connection();
    const manager = new LSPDocumentManager(config(path.dirname(file)));
    manager.setConnection(conn as never, true);

    await manager.openDocument(file);
    await manager.ensureDocumentSynced(file, { content: 'one\nthree\n' });
    expect(conn.sendNotification).toHaveBeenLastCalledWith(
      'textDocument/didChange',
      expect.objectContaining({
        contentChanges: [{ text: 'one\nthree\n' }],
      })
    );

    manager.setTextDocumentSync(TextDocumentSyncKind.Incremental);
    await manager.ensureDocumentSynced(file, { content: 'one\nfour\n' });
    expect(conn.sendNotification).toHaveBeenLastCalledWith(
      'textDocument/didChange',
      expect.objectContaining({
        contentChanges: [
          expect.objectContaining({
            range: {
              start: { line: 0, character: 0 },
              end: { line: 2, character: 0 },
            },
            text: 'one\nfour\n',
          }),
        ],
      })
    );

    manager.setTextDocumentSync(TextDocumentSyncKind.None);
    await manager.ensureDocumentSynced(file, { content: 'one\nfive\n' });
    expect(conn.sendNotification).toHaveBeenCalledWith(
      'textDocument/didClose',
      { textDocument: { uri: toUri(file) } }
    );
    expect(conn.sendNotification).toHaveBeenLastCalledWith(
      'textDocument/didOpen',
      expect.objectContaining({
        textDocument: expect.objectContaining({ text: 'one\nfive\n' }),
      })
    );
  });

  it('handles unsupported sync modes and close failures', async () => {
    const file = await tempFile('demo.foo', 'alpha\n');
    const manager = new LSPDocumentManager(config(path.dirname(file)));

    await expect(manager.openDocument(file)).rejects.toThrow(
      'LSP client not initialized'
    );
    await expect(manager.closeDocument(file)).resolves.toBeUndefined();

    manager.setConnection(connection() as never, true);
    await expect(manager.closeDocument(file)).resolves.toBeUndefined();
    manager.setTextDocumentSync({ openClose: false });
    await expect(manager.openDocument(file)).rejects.toThrow(
      'Language server does not support opening documents'
    );

    const closeConn = {
      sendNotification: vi.fn().mockImplementation((method: string) => {
        if (method === 'textDocument/didClose') {
          throw new Error('closed');
        }
        return Promise.resolve();
      }),
    };
    const closeManager = new LSPDocumentManager(config(path.dirname(file)));
    closeManager.setConnection(closeConn as never, true);
    await closeManager.openDocument(file);
    await closeManager.closeAllDocuments();
    expect(closeManager.getOpenDocumentUris()).toEqual([]);
  });

  it('uses fallback defaults for language id, ref counts, and sync config', async () => {
    const file = await tempFile('demo.foo', 'alpha\n');
    const conn = connection();
    const manager = new LSPDocumentManager(
      configWithoutLanguageId(path.dirname(file))
    );
    manager.setConnection(conn as never, true);
    manager.setTextDocumentSync(null);

    await manager.ensureDocumentSynced(file);

    expect(manager.getOpenDocumentRefCount(file)).toBe(0);
    expect(
      manager.getOpenDocumentRefCount(
        path.join(path.dirname(file), 'missing.foo')
      )
    ).toBe(0);
    expect(conn.sendNotification).toHaveBeenCalledWith(
      'textDocument/didOpen',
      expect.objectContaining({
        textDocument: expect.objectContaining({
          languageId: 'plaintext',
        }),
      })
    );
  });

  it('rejects document changes when a server disables change sync', async () => {
    const file = await tempFile('demo.foo', 'alpha\n');
    const manager = new LSPDocumentManager(config(path.dirname(file)));
    manager.setConnection(connection() as never, true);
    await manager.openDocument(file);

    manager.setTextDocumentSync({
      openClose: false,
      change: TextDocumentSyncKind.None,
    });

    await expect(
      manager.ensureDocumentSynced(file, { content: 'beta\n' })
    ).rejects.toThrow('Language server does not support document changes');
  });
});
