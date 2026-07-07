import { promises as fs } from 'node:fs';

import { nativeBinding, type NativeLspClientBinding } from './native.js';
import { validateLSPServerPath } from './validation.js';
import type {
  CallHierarchyItem,
  CodeSnippet,
  ExactPosition,
  IncomingCall,
  LanguageServerConfig,
  LspReadiness,
  OutgoingCall,
} from './types.js';

export class LSPClient {
  private readonly nativeClient: NativeLspClientBinding;
  private readonly command: string;
  private initialized = false;
  private lastReadiness: LspReadiness | undefined;

  constructor(config: LanguageServerConfig) {
    this.command = config.command;
    this.nativeClient = new nativeBinding.NativeLspClient({
      command: config.command,
      args: config.args,
      workspaceRoot: config.workspaceRoot,
      languageId: config.languageId,
      initializationOptions: config.initializationOptions,
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
   * readiness signal. Runtime tolerance: until the native addon is rebuilt with
   * the readiness return, the old binding resolves to `undefined` — treat that
   * as the conservative `settledFallback` (we cannot confirm indexing finished).
   */
  async waitForReady(timeoutMs = 45_000): Promise<LspReadiness> {
    const readiness = await this.nativeClient.waitForReady(timeoutMs);
    this.lastReadiness = readiness ?? 'settledFallback';
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
    return (
      this.initialized &&
      (this.nativeClient.hasCapability?.(capability) ?? true)
    );
  }

  getRecentStderr(): string[] {
    return this.nativeClient.getRecentStderr?.() ?? [];
  }

  async openDocument(filePath: string, content?: string): Promise<void> {
    await this.nativeClient.openDocument(
      filePath,
      content ?? (await fs.readFile(filePath, 'utf8'))
    );
  }

  async closeDocument(filePath: string): Promise<void> {
    // Drives the native `textDocument/didClose` and clears the document's
    // version state, so a later openDocument starts a fresh didOpen. A no-op
    // here leaves the server holding stale in-memory documents forever.
    await this.nativeClient.closeDocument?.(filePath);
  }
}
