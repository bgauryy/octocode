import { promises as fs } from 'fs';
import { MessageConnection } from 'vscode-jsonrpc/node.js';
import {
  DidOpenTextDocumentParams,
  DidCloseTextDocumentParams,
  DidChangeTextDocumentParams,
  TextDocumentItem,
  TextDocumentIdentifier,
  VersionedTextDocumentIdentifier,
  TextDocumentSyncKind,
} from 'vscode-languageserver-protocol';
import { fromUri, toUri } from './uri.js';
import { detectLanguageId } from './config.js';
import type { LanguageServerConfig } from './types.js';

interface OpenDocumentState {
  version: number;
  refCount: number;
  content: string;
}

type NormalizedTextDocumentSync = {
  openClose: boolean;
  change: TextDocumentSyncKind;
};

export class LSPDocumentManager {
  private openFiles = new Map<string, OpenDocumentState>();
  private connection: MessageConnection | null = null;
  private initialized = false;
  private config: LanguageServerConfig;
  private textDocumentSync: NormalizedTextDocumentSync = {
    openClose: true,
    change: TextDocumentSyncKind.Full,
  };

  constructor(config: LanguageServerConfig) {
    this.config = config;
  }

  setConnection(
    connection: MessageConnection | null,
    initialized: boolean
  ): void {
    this.connection = connection;
    this.initialized = initialized;
    if (!connection) {
      this.openFiles.clear();
    }
  }

  setTextDocumentSync(value: unknown): void {
    this.textDocumentSync = normalizeTextDocumentSync(value);
  }

  async openDocument(filePath: string, content?: string): Promise<void> {
    await this.ensureDocumentSynced(filePath, {
      content,
      incrementRefCount: true,
    });
  }

  async ensureDocumentSynced(
    filePath: string,
    options: { content?: string; incrementRefCount?: boolean } = {}
  ): Promise<void> {
    if (!this.connection || !this.initialized) {
      throw new Error('LSP client not initialized');
    }

    const uri = toUri(filePath);
    const content = options.content ?? (await fs.readFile(filePath, 'utf-8'));

    const existing = this.openFiles.get(uri);
    if (existing) {
      if (options.incrementRefCount) {
        existing.refCount += 1;
      }
      if (existing.content !== content) {
        await this.changeDocument(uri, existing, content);
      }
      return;
    }

    if (!this.textDocumentSync.openClose) {
      throw new Error('Language server does not support opening documents');
    }

    const detectedLanguageId = detectLanguageId(filePath);
    const languageId =
      detectedLanguageId === 'plaintext'
        ? (this.config.languageId ?? detectedLanguageId)
        : detectedLanguageId;

    const params: DidOpenTextDocumentParams = {
      textDocument: {
        uri,
        languageId,
        version: 1,
        text: content,
      } as TextDocumentItem,
    };

    await this.connection.sendNotification('textDocument/didOpen', params);
    this.openFiles.set(uri, {
      version: 1,
      refCount: options.incrementRefCount ? 1 : 0,
      content,
    });
  }

  async closeDocument(filePath: string): Promise<void> {
    if (!this.connection || !this.initialized) {
      return;
    }

    const uri = toUri(filePath);
    const existing = this.openFiles.get(uri);
    if (!existing) {
      return;
    }

    existing.refCount -= 1;
    if (existing.refCount > 0) {
      return;
    }

    const params: DidCloseTextDocumentParams = {
      textDocument: { uri } as TextDocumentIdentifier,
    };

    await this.connection.sendNotification('textDocument/didClose', params);
    this.openFiles.delete(uri);
  }

  async closeAllDocuments(): Promise<void> {
    for (const uri of Array.from(this.openFiles.keys())) {
      try {
        const params: DidCloseTextDocumentParams = {
          textDocument: { uri } as TextDocumentIdentifier,
        };
        await this.connection?.sendNotification(
          'textDocument/didClose',
          params
        );
        this.openFiles.delete(uri);
      } catch {
        this.openFiles.delete(uri);
      }
    }
  }

  isDocumentOpen(filePath: string): boolean {
    const uri = toUri(filePath);
    return this.openFiles.has(uri);
  }

  getOpenDocumentUris(): string[] {
    return Array.from(this.openFiles.keys());
  }

  getOpenDocumentRefCount(filePath: string): number {
    const uri = toUri(filePath);
    return this.openFiles.get(uri)?.refCount ?? 0;
  }

  private async changeDocument(
    uri: string,
    state: OpenDocumentState,
    content: string
  ): Promise<void> {
    state.version += 1;
    const change = this.textDocumentSync.change;

    if (change === TextDocumentSyncKind.None) {
      if (!this.textDocumentSync.openClose) {
        throw new Error('Language server does not support document changes');
      }
      await this.connection!.sendNotification('textDocument/didClose', {
        textDocument: { uri } as TextDocumentIdentifier,
      } satisfies DidCloseTextDocumentParams);
      await this.connection!.sendNotification('textDocument/didOpen', {
        textDocument: {
          uri,
          languageId: detectLanguageId(fromUri(uri)) ?? this.config.languageId,
          version: state.version,
          text: content,
        } as TextDocumentItem,
      } satisfies DidOpenTextDocumentParams);
      state.content = content;
      return;
    }

    const params: DidChangeTextDocumentParams = {
      textDocument: {
        uri,
        version: state.version,
      } as VersionedTextDocumentIdentifier,
      contentChanges:
        change === TextDocumentSyncKind.Incremental
          ? [
              {
                range: wholeDocumentRange(state.content),
                text: content,
              },
            ]
          : [{ text: content }],
    };

    await this.connection!.sendNotification('textDocument/didChange', params);
    state.content = content;
  }
}

function normalizeTextDocumentSync(value: unknown): NormalizedTextDocumentSync {
  if (typeof value === 'number') {
    return { openClose: true, change: value as TextDocumentSyncKind };
  }

  if (value && typeof value === 'object') {
    const sync = value as { openClose?: unknown; change?: unknown };
    return {
      openClose: sync.openClose !== false,
      change:
        typeof sync.change === 'number'
          ? (sync.change as TextDocumentSyncKind)
          : TextDocumentSyncKind.Full,
    };
  }

  return { openClose: true, change: TextDocumentSyncKind.Full };
}

function wholeDocumentRange(content: string) {
  const lines = content.split(/\r?\n/);
  const lastLineIndex = Math.max(0, lines.length - 1);
  const lastLine = lines[lastLineIndex] ?? '';
  return {
    start: { line: 0, character: 0 },
    end: { line: lastLineIndex, character: lastLine.length },
  };
}
