import { promises as fs } from 'fs';
import { MessageConnection } from 'vscode-jsonrpc/node.js';
import {
  DidOpenTextDocumentParams,
  DidCloseTextDocumentParams,
  TextDocumentItem,
  TextDocumentIdentifier,
} from 'vscode-languageserver-protocol';
import { toUri } from './uri.js';
import { detectLanguageId } from './config.js';
import type { LanguageServerConfig } from './types.js';

interface OpenDocumentState {
  version: number;
  refCount: number;
}

export class LSPDocumentManager {
  private openFiles = new Map<string, OpenDocumentState>();
  private connection: MessageConnection | null = null;
  private initialized = false;
  private config: LanguageServerConfig;

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

  async openDocument(filePath: string): Promise<void> {
    if (!this.connection || !this.initialized) {
      throw new Error('LSP client not initialized');
    }

    const uri = toUri(filePath);

    const existing = this.openFiles.get(uri);
    if (existing) {
      existing.refCount += 1;
      return;
    }

    const content = await fs.readFile(filePath, 'utf-8');
    const languageId = this.config.languageId ?? detectLanguageId(filePath);

    const params: DidOpenTextDocumentParams = {
      textDocument: {
        uri,
        languageId,
        version: 1,
        text: content,
      } as TextDocumentItem,
    };

    await this.connection.sendNotification('textDocument/didOpen', params);
    this.openFiles.set(uri, { version: 1, refCount: 1 });
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
}
