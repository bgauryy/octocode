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
      env: config.env,
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
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    return this.getDefinition(filePath, position, content);
  }

  async getDefinition(
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

  async getHover(
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

  async hover(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<unknown> {
    return this.getHover(filePath, position, content);
  }

  async getTypeDefinition(
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

  async typeDefinition(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    return this.getTypeDefinition(filePath, position, content);
  }

  async getImplementation(
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

  async implementation(
    filePath: string,
    position: ExactPosition,
    content?: string
  ): Promise<CodeSnippet[]> {
    return this.getImplementation(filePath, position, content);
  }

  async getDocumentSymbols(
    filePath: string,
    content?: string
  ): Promise<unknown> {
    await this.openDocument(filePath, content);
    return this.nativeClient.getDocumentSymbols(filePath);
  }

  async documentSymbols(filePath: string, content?: string): Promise<unknown> {
    return this.getDocumentSymbols(filePath, content);
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

  hasCapability(_capability: string): boolean {
    return (
      this.initialized &&
      (this.nativeClient.hasCapability?.(_capability) ?? true)
    );
  }

  getRecentStderr(): string[] {
    return this.nativeClient.getRecentStderr?.() ?? [];
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
