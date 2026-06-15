import { promises as fs } from 'node:fs';

import { nativeBinding, type NativeLspClientBinding } from './native.js';
import type {
  CallHierarchyItem,
  CodeSnippet,
  ExactPosition,
  IncomingCall,
  LanguageServerConfig,
  OutgoingCall,
} from './types.js';

export class LSPClient {
  private readonly nativeClient: NativeLspClientBinding;
  private initialized = false;

  constructor(config: LanguageServerConfig) {
    this.nativeClient = new nativeBinding.NativeLspClient({
      command: config.command,
      args: config.args,
      workspaceRoot: config.workspaceRoot,
      languageId: config.languageId,
      initializationOptions: config.initializationOptions,
    });
  }

  async start(): Promise<void> {
    await this.nativeClient.start();
    this.initialized = true;
  }

  async stop(): Promise<void> {
    await this.nativeClient.stop();
    this.initialized = false;
  }

  async waitForReady(timeoutMs = 45_000): Promise<void> {
    await this.nativeClient.waitForReady(timeoutMs);
  }

  async gotoDefinition(
    filePath: string,
    position: ExactPosition
  ): Promise<CodeSnippet[]> {
    return this.getDefinition(filePath, position);
  }

  async getDefinition(
    filePath: string,
    position: ExactPosition
  ): Promise<CodeSnippet[]> {
    await this.openDocument(filePath);
    return (await this.nativeClient.getDefinition(
      filePath,
      position.line,
      position.character
    )) as CodeSnippet[];
  }

  async findReferences(
    filePath: string,
    position: ExactPosition,
    includeDeclaration = true
  ): Promise<CodeSnippet[]> {
    await this.openDocument(filePath);
    return (await this.nativeClient.getReferences(
      filePath,
      position.line,
      position.character,
      includeDeclaration
    )) as CodeSnippet[];
  }

  async getHover(filePath: string, position: ExactPosition): Promise<unknown> {
    await this.openDocument(filePath);
    return this.nativeClient.getHover(
      filePath,
      position.line,
      position.character
    );
  }

  async hover(filePath: string, position: ExactPosition): Promise<unknown> {
    return this.getHover(filePath, position);
  }

  async getTypeDefinition(
    filePath: string,
    position: ExactPosition
  ): Promise<CodeSnippet[]> {
    await this.openDocument(filePath);
    return (await this.nativeClient.getTypeDefinition(
      filePath,
      position.line,
      position.character
    )) as CodeSnippet[];
  }

  async typeDefinition(
    filePath: string,
    position: ExactPosition
  ): Promise<CodeSnippet[]> {
    return this.getTypeDefinition(filePath, position);
  }

  async getImplementation(
    filePath: string,
    position: ExactPosition
  ): Promise<CodeSnippet[]> {
    await this.openDocument(filePath);
    return (await this.nativeClient.getImplementation(
      filePath,
      position.line,
      position.character
    )) as CodeSnippet[];
  }

  async implementation(
    filePath: string,
    position: ExactPosition
  ): Promise<CodeSnippet[]> {
    return this.getImplementation(filePath, position);
  }

  async getDocumentSymbols(filePath: string): Promise<unknown> {
    await this.openDocument(filePath);
    return this.nativeClient.getDocumentSymbols(filePath);
  }

  async documentSymbols(filePath: string): Promise<unknown> {
    return this.getDocumentSymbols(filePath);
  }

  async prepareCallHierarchy(
    filePath: string,
    position: ExactPosition
  ): Promise<CallHierarchyItem[]> {
    await this.openDocument(filePath);
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

  hasCapability(_capability: string): boolean {
    return this.initialized;
  }

  getRecentStderr(): string[] {
    return [];
  }

  async openDocument(filePath: string, content?: string): Promise<void> {
    await this.ensureDocumentSynced(
      filePath,
      content ?? (await fs.readFile(filePath, 'utf8'))
    );
  }

  async ensureDocumentSynced(
    filePath: string,
    content?: string
  ): Promise<void> {
    await this.nativeClient.openDocument(
      filePath,
      content ?? (await fs.readFile(filePath, 'utf8'))
    );
  }

  async closeDocument(_filePath: string): Promise<void> {
    return Promise.resolve();
  }
}
