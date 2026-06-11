import { promises as fs } from 'fs';
import { MessageConnection } from 'vscode-jsonrpc/node.js';
import {
  DefinitionParams,
  ReferenceParams,
  Location,
  LocationLink,
  HoverParams,
  Hover,
  TypeDefinitionParams,
  ImplementationParams,
  DocumentSymbolParams,
  DocumentSymbol,
  SymbolInformation,
  CallHierarchyPrepareParams,
  CallHierarchyItem as LSPCallHierarchyItem,
  CallHierarchyIncomingCallsParams,
  CallHierarchyOutgoingCallsParams,
  CallHierarchyIncomingCall,
  CallHierarchyOutgoingCall,
  SymbolKind as LSPSymbolKind,
} from 'vscode-languageserver-protocol';
import { toUri, fromUri } from './uri.js';
import { convertSymbolKind, toLSPSymbolKind } from './symbols.js';
import type {
  ExactPosition,
  CodeSnippet,
  CallHierarchyItem,
  IncomingCall,
  OutgoingCall,
} from './types.js';
import { LSPDocumentManager } from './lspDocumentManager.js';
import { PathValidator } from 'octocode-security-utils/pathValidator';
import { sendRequestWithCancellationOnTimeout } from './cancellableRequest.js';

export class LSPOperations {
  private connection: MessageConnection | null = null;
  private initialized = false;
  private documentManager: LSPDocumentManager;
  private pathValidator: PathValidator;
  private projectReadyWaiter: (() => Promise<void>) | null = null;

  constructor(documentManager: LSPDocumentManager, workspaceRoot?: string) {
    this.documentManager = documentManager;
    this.pathValidator = new PathValidator({
      workspaceRoot: workspaceRoot ?? process.cwd(),
      includeHomeDir: true,
    });
  }

  setConnection(
    connection: MessageConnection | null,
    initialized: boolean
  ): void {
    this.connection = connection;
    this.initialized = initialized;
  }

  /**
   * Awaited after a document is opened and before any request is sent, so
   * project-scoped requests (references, implementations, call hierarchy)
   * see the fully loaded project rather than file-scoped partial results.
   */
  setProjectReadyWaiter(waiter: (() => Promise<void>) | null): void {
    this.projectReadyWaiter = waiter;
  }

  private requireConnection(): MessageConnection {
    if (!this.connection || !this.initialized) {
      throw new Error('LSP client not initialized');
    }
    return this.connection;
  }

  private async withDocument<T>(
    filePath: string,
    fn: (connection: MessageConnection) => Promise<T>,
    content?: string
  ): Promise<T> {
    const connection = this.requireConnection();
    // Open increments refCount; documents stay open across repeated queries on
    // the same pooled client, saving the server a re-parse on every call.
    // closeAllDocuments() is called when the pool evicts the client (stop()).
    await this.documentManager.openDocument(filePath, content);
    // Project load starts on didOpen — wait for indexing before the request.
    if (this.projectReadyWaiter) await this.projectReadyWaiter();
    return fn(connection);
  }

  async gotoDefinition(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    return this.withDocument(
      filePath,
      async connection => {
        const params: DefinitionParams = {
          textDocument: { uri: toUri(filePath) },
          position: {
            line: position.line,
            character: position.character,
          },
        };

        const result = await sendRequestWithCancellationOnTimeout<
          Location | Location[] | LocationLink[] | null
        >(connection, 'textDocument/definition', params);

        return this.locationsToSnippets(result);
      },
      content
    );
  }

  async findReferences(
    filePath: string,
    position: ExactPosition,
    includeDeclaration = true,
    content?: string
  ): Promise<CodeSnippet[]> {
    return this.withDocument(
      filePath,
      async connection => {
        const params: ReferenceParams = {
          textDocument: { uri: toUri(filePath) },
          position: {
            line: position.line,
            character: position.character,
          },
          context: { includeDeclaration },
        };

        const result = await sendRequestWithCancellationOnTimeout<
          Location[] | null
        >(connection, 'textDocument/references', params);

        return this.locationsToSnippets(result);
      },
      content
    );
  }

  async prepareCallHierarchy(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CallHierarchyItem[]> {
    return this.withDocument(
      filePath,
      async connection => {
        const params: CallHierarchyPrepareParams = {
          textDocument: { uri: toUri(filePath) },
          position: {
            line: position.line,
            character: position.character,
          },
        };

        const result = await sendRequestWithCancellationOnTimeout<
          LSPCallHierarchyItem[] | null
        >(connection, 'textDocument/prepareCallHierarchy', params);

        if (!result || !Array.isArray(result)) {
          return [];
        }

        return result.map(item => this.convertCallHierarchyItem(item));
      },
      content
    );
  }

  async hover(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<Hover | null> {
    return this.withDocument(
      filePath,
      async connection => {
        const params: HoverParams = {
          textDocument: { uri: toUri(filePath) },
          position,
        };

        return sendRequestWithCancellationOnTimeout<Hover | null>(
          connection,
          'textDocument/hover',
          params
        );
      },
      content
    );
  }

  async typeDefinition(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    return this.withDocument(
      filePath,
      async connection => {
        const params: TypeDefinitionParams = {
          textDocument: { uri: toUri(filePath) },
          position,
        };

        const result = await sendRequestWithCancellationOnTimeout<
          Location | Location[] | LocationLink[] | null
        >(connection, 'textDocument/typeDefinition', params);

        return this.locationsToSnippets(result);
      },
      content
    );
  }

  async implementation(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    return this.withDocument(
      filePath,
      async connection => {
        const params: ImplementationParams = {
          textDocument: { uri: toUri(filePath) },
          position,
        };

        const result = await sendRequestWithCancellationOnTimeout<
          Location | Location[] | LocationLink[] | null
        >(connection, 'textDocument/implementation', params);

        return this.locationsToSnippets(result);
      },
      content
    );
  }

  async documentSymbols(
    filePath: string,
    content?: string
  ): Promise<Array<DocumentSymbol | SymbolInformation>> {
    return this.withDocument(
      filePath,
      async connection => {
        const params: DocumentSymbolParams = {
          textDocument: { uri: toUri(filePath) },
        };

        const result = await sendRequestWithCancellationOnTimeout<Array<
          DocumentSymbol | SymbolInformation
        > | null>(connection, 'textDocument/documentSymbol', params);

        return Array.isArray(result) ? result : [];
      },
      content
    );
  }

  async getIncomingCalls(item: CallHierarchyItem): Promise<IncomingCall[]> {
    const connection = this.requireConnection();

    const params: CallHierarchyIncomingCallsParams = {
      item: this.toProtocolCallHierarchyItem(item),
    };

    const result = await sendRequestWithCancellationOnTimeout<
      CallHierarchyIncomingCall[] | null
    >(connection, 'callHierarchy/incomingCalls', params);

    if (!result || !Array.isArray(result)) {
      return [];
    }

    return result
      .filter(call => call?.from && call?.fromRanges)
      .map(call => ({
        from: this.convertCallHierarchyItem(call.from),
        fromRanges: (call.fromRanges ?? []).map(r => ({
          start: {
            line: r?.start?.line ?? 0,
            character: r?.start?.character ?? 0,
          },
          end: { line: r?.end?.line ?? 0, character: r?.end?.character ?? 0 },
        })),
      }));
  }

  async getOutgoingCalls(item: CallHierarchyItem): Promise<OutgoingCall[]> {
    const connection = this.requireConnection();

    const params: CallHierarchyOutgoingCallsParams = {
      item: this.toProtocolCallHierarchyItem(item),
    };

    const result = await sendRequestWithCancellationOnTimeout<
      CallHierarchyOutgoingCall[] | null
    >(connection, 'callHierarchy/outgoingCalls', params);

    if (!result || !Array.isArray(result)) {
      return [];
    }

    return result
      .filter(call => call?.to && call?.fromRanges)
      .map(call => ({
        to: this.convertCallHierarchyItem(call.to),
        fromRanges: (call.fromRanges ?? []).map(r => ({
          start: {
            line: r?.start?.line ?? 0,
            character: r?.start?.character ?? 0,
          },
          end: { line: r?.end?.line ?? 0, character: r?.end?.character ?? 0 },
        })),
      }));
  }

  private async locationsToSnippets(
    result: Location | Location[] | LocationLink[] | null
  ): Promise<CodeSnippet[]> {
    if (!result) return [];

    const locations = Array.isArray(result) ? result : [result];
    const snippets: CodeSnippet[] = [];

    for (const loc of locations) {
      if (!loc) continue;

      const uri = 'targetUri' in loc ? loc.targetUri : loc.uri;
      const range = 'targetRange' in loc ? loc.targetRange : loc.range;

      if (!uri || !range?.start || !range?.end) continue;

      const filePath = fromUri(uri);

      const pathValidation = this.pathValidator.validate(filePath);
      if (!pathValidation.isValid) {
        continue;
      }

      let content = '';

      try {
        const safePath = pathValidation.sanitizedPath ?? filePath;
        const fileContent = await fs.readFile(safePath, 'utf-8');
        const lines = fileContent.split(/\r?\n/);
        const startLine = range.start.line;
        const endLine = range.end.line;
        content = lines
          .slice(startLine, endLine + 1)
          .map((line, i) => `${startLine + i + 1}\t${line}`)
          .join('\n');
      } catch {
        content = `[Could not read file: ${filePath}]`;
      }

      snippets.push({
        uri: filePath,
        range: {
          start: { line: range.start.line, character: range.start.character },
          end: { line: range.end.line, character: range.end.character },
        },
        content,
        displayRange: {
          startLine: range.start.line + 1,
          endLine: range.end.line + 1,
        },
      });
    }

    return snippets;
  }

  private convertCallHierarchyItem(
    item: LSPCallHierarchyItem
  ): CallHierarchyItem {
    const defaultPos = { line: 0, character: 0 };
    const defaultRange = { start: defaultPos, end: defaultPos };

    const range = item.range ?? defaultRange;
    const selectionRange = item.selectionRange ?? range;

    const startLine = range.start?.line ?? 0;
    const startChar = range.start?.character ?? 0;
    const endLine = range.end?.line ?? 0;
    const endChar = range.end?.character ?? 0;

    const selStartLine = selectionRange.start?.line ?? 0;
    const selStartChar = selectionRange.start?.character ?? 0;
    const selEndLine = selectionRange.end?.line ?? 0;
    const selEndChar = selectionRange.end?.character ?? 0;

    return {
      name: item.name ?? '',
      kind: convertSymbolKind(item.kind),
      uri: fromUri(item.uri ?? ''),
      range: {
        start: { line: startLine, character: startChar },
        end: { line: endLine, character: endChar },
      },
      selectionRange: {
        start: { line: selStartLine, character: selStartChar },
        end: { line: selEndLine, character: selEndChar },
      },
      displayRange: {
        startLine: startLine + 1,
        endLine: endLine + 1,
      },
    };
  }

  private toProtocolCallHierarchyItem(
    item: CallHierarchyItem
  ): LSPCallHierarchyItem {
    return {
      name: item.name,
      kind: item.kind ? toLSPSymbolKind(item.kind) : LSPSymbolKind.Function,
      uri: toUri(item.uri),
      range: {
        start: {
          line: item.range.start.line,
          character: item.range.start.character,
        },
        end: { line: item.range.end.line, character: item.range.end.character },
      },
      selectionRange: item.selectionRange
        ? {
            start: {
              line: item.selectionRange.start.line,
              character: item.selectionRange.start.character,
            },
            end: {
              line: item.selectionRange.end.line,
              character: item.selectionRange.end.character,
            },
          }
        : {
            start: {
              line: item.range.start.line,
              character: item.range.start.character,
            },
            end: {
              line: item.range.end.line,
              character: item.range.end.character,
            },
          },
    };
  }
}
