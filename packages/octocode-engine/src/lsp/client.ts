import { promises as fs } from 'node:fs';

import { nativeBinding, type NativeLspClientBinding } from './native.js';
import { validateLSPServerPath } from './validation.js';
import { TSSERVER_LANGUAGE_IDS } from './initConstants.js';
import type {
  CallHierarchyItem,
  CodeSnippet,
  ExactPosition,
  IncomingCall,
  LanguageServerConfig,
  LspReadiness,
  OutgoingCall,
} from './types.js';

const MAX_LSP_DOCUMENT_BYTES = 1_000_000;

export class LSPClient {
  private readonly nativeClient: NativeLspClientBinding;
  private readonly command: string;
  private initialized = false;
  private lastReadiness: LspReadiness | undefined;

  constructor(config: LanguageServerConfig) {
    this.command = config.command;
    const initializationOptions = config.initializationOptions;
    const tsserver = initializationOptions?.tsserver;
    this.nativeClient = new nativeBinding.NativeLspClient({
      command: config.command,
      args: config.args,
      workspaceRoot: config.workspaceRoot,
      languageId: config.languageId,
      initializationOptions: TSSERVER_LANGUAGE_IDS.has(config.languageId ?? '')
        ? {
            ...initializationOptions,
            tsserver: {
              ...(tsserver && typeof tsserver === 'object' ? tsserver : {}),
              // The syntax server answers cold definitions with local import
              // bindings while the semantic project loads. Agent requests need
              // the full provider from the first query, without guessed paths.
              useSyntaxServer: 'never',
            },
          }
        : initializationOptions,
      env: config.env,
    });
  }

  async start(): Promise<void> {
    // Security gate: never spawn a command that isn't a real, executable,
    // non-shell server binary — even one resolved from the managed download
    // cache. This is the single chokepoint before the native process spawn.
    const validation = validateLSPServerPath(this.command);
    if (!validation.isValid) {
      throw new Error(
        `Refusing to start language server: ${validation.error ?? `invalid server path '${this.command}'`}`
      );
    }
    await this.nativeClient.start();
    this.initialized = true;
  }

  async stop(): Promise<void> {
    await this.nativeClient.stop();
    this.initialized = false;
  }

  /**
   * Wait for the server to finish post-`initialized` indexing and record the
   * readiness signal supplied by the native client.
   */
  async waitForReady(timeoutMs = 45_000): Promise<LspReadiness> {
    this.lastReadiness = await this.nativeClient.waitForReady(timeoutMs);
    return this.lastReadiness;
  }

  /**
   * The readiness recorded by the most recent `waitForReady`, or `undefined`
   * if it was never called (e.g. servers that answer immediately and skip the
   * readiness wait). A zero-results semantic query on a client whose readiness
   * is not `progressIdle` may be "not indexed yet" rather than a true absence.
   */
  getReadiness(): LspReadiness | undefined {
    return this.lastReadiness;
  }

  async gotoDefinition(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    await this.openDocument(filePath, content);
    return (await this.nativeClient.getDefinition(
      filePath,
      position.line,
      position.character
    )) as CodeSnippet[];
  }

  async findReferences(
    filePath: string,
    position: ExactPosition,
    includeDeclaration = true,
    content?: string
  ): Promise<CodeSnippet[]> {
    await this.openDocument(filePath, content);
    return (await this.nativeClient.getReferences(
      filePath,
      position.line,
      position.character,
      includeDeclaration
    )) as CodeSnippet[];
  }

  async hover(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<unknown> {
    await this.openDocument(filePath, content);
    return this.nativeClient.getHover(
      filePath,
      position.line,
      position.character
    );
  }

  async typeDefinition(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    await this.openDocument(filePath, content);
    return (await this.nativeClient.getTypeDefinition(
      filePath,
      position.line,
      position.character
    )) as CodeSnippet[];
  }

  async implementation(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    await this.openDocument(filePath, content);
    return (await this.nativeClient.getImplementation(
      filePath,
      position.line,
      position.character
    )) as CodeSnippet[];
  }

  async documentSymbols(filePath: string, content?: string): Promise<unknown> {
    await this.openDocument(filePath, content);
    return this.nativeClient.getDocumentSymbols(filePath);
  }

  async prepareCallHierarchy(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CallHierarchyItem[]> {
    await this.openDocument(filePath, content);
    const result = await this.nativeClient.prepareCallHierarchy(
      filePath,
      position.line,
      position.character
    );
    return Array.isArray(result) ? (result as CallHierarchyItem[]) : [];
  }

  async getIncomingCalls(item: CallHierarchyItem): Promise<IncomingCall[]> {
    const result = await this.nativeClient.incomingCalls(item);
    return Array.isArray(result) ? (result as IncomingCall[]) : [];
  }

  async getOutgoingCalls(item: CallHierarchyItem): Promise<OutgoingCall[]> {
    const result = await this.nativeClient.outgoingCalls(item);
    return Array.isArray(result) ? (result as OutgoingCall[]) : [];
  }

  async workspaceSymbol(query: string): Promise<unknown[]> {
    const result = await this.nativeClient.workspaceSymbol(query);
    return Array.isArray(result) ? result : [];
  }

  async prepareTypeHierarchy(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<unknown[]> {
    await this.openDocument(filePath, content);
    const result = await this.nativeClient.prepareTypeHierarchy(
      filePath,
      position.line,
      position.character
    );
    return Array.isArray(result) ? result : [];
  }

  async typeHierarchySupertypes(item: unknown): Promise<unknown[]> {
    const result = await this.nativeClient.typeHierarchySupertypes(item);
    return Array.isArray(result) ? result : [];
  }

  async typeHierarchySubtypes(item: unknown): Promise<unknown[]> {
    const result = await this.nativeClient.typeHierarchySubtypes(item);
    return Array.isArray(result) ? result : [];
  }

  async getDiagnostics(filePath: string, content?: string): Promise<unknown> {
    await this.openDocument(filePath, content);
    return this.nativeClient.getDiagnostics(filePath);
  }

  hasCapability(capability: string): boolean {
    return this.initialized && this.nativeClient.hasCapability(capability);
  }

  /**
   * `false` once the server process/connection has died (crashed mid-session)
   * — lets the shared client pool evict this entry at the next `acquire()`
   * instead of returning a client whose requests will just fail until the
   * idle timer eventually reaps it.
   */
  async isAlive(): Promise<boolean> {
    return this.initialized && (await this.nativeClient.isAlive());
  }

  getRecentStderr(): string[] {
    return this.nativeClient.getRecentStderr();
  }

  async openDocument(filePath: string, content?: string): Promise<void> {
    const documentContent =
      content ?? (await this.readDocumentForOpen(filePath));
    await this.nativeClient.openDocument(filePath, documentContent);
  }

  private async readDocumentForOpen(filePath: string): Promise<string> {
    const stats = await fs.stat(filePath);
    if (stats.size > MAX_LSP_DOCUMENT_BYTES) {
      throw new Error(
        `File is too large for LSP document open: ${filePath} (${stats.size} bytes > ${MAX_LSP_DOCUMENT_BYTES} bytes)`
      );
    }
    return fs.readFile(filePath, 'utf8');
  }

  async closeDocument(filePath: string): Promise<void> {
    // Drives the native `textDocument/didClose` and clears the document's
    // version state, so a later openDocument starts a fresh didOpen. A no-op
    // here leaves the server holding stale in-memory documents forever.
    await this.nativeClient.closeDocument(filePath);
  }
}
