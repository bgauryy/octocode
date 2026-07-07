import { createRequire } from 'node:module';

import type { LspReadiness } from './types.js';

const require = createRequire(import.meta.url);

export type NativeLspClientBinding = {
  start(): Promise<void>;
  stop(): Promise<void>;
  // The native addon resolves to an `LspReadiness` string once rebuilt; an
  // older addon (pre-readiness) resolves to `undefined`, which the LSPClient
  // wrapper coerces to the conservative `settledFallback`.
  waitForReady(timeoutMs?: number): Promise<LspReadiness | undefined>;
  hasCapability?(capability: string): boolean;
  getRecentStderr?(): string[];
  openDocument(filePath: string, content: string): Promise<void>;
  closeDocument?(filePath: string): Promise<void>;
  getDefinition(
    filePath: string,
    line: number,
    character: number
  ): Promise<unknown[]>;
  getReferences(
    filePath: string,
    line: number,
    character: number,
    includeDeclaration?: boolean
  ): Promise<unknown[]>;
  getHover(filePath: string, line: number, character: number): Promise<unknown>;
  getTypeDefinition(
    filePath: string,
    line: number,
    character: number
  ): Promise<unknown[]>;
  getImplementation(
    filePath: string,
    line: number,
    character: number
  ): Promise<unknown[]>;
  getDocumentSymbols(filePath: string): Promise<unknown>;
  prepareCallHierarchy(
    filePath: string,
    line: number,
    character: number
  ): Promise<unknown>;
  incomingCalls(item: unknown): Promise<unknown>;
  outgoingCalls(item: unknown): Promise<unknown>;
  workspaceSymbol(query: string): Promise<unknown>;
  prepareTypeHierarchy(
    filePath: string,
    line: number,
    character: number
  ): Promise<unknown>;
  typeHierarchySupertypes(item: unknown): Promise<unknown>;
  typeHierarchySubtypes(item: unknown): Promise<unknown>;
  getDiagnostics(filePath: string): Promise<unknown>;
};

type NativeBinding = {
  NativeLspClient: new (config: unknown) => NativeLspClientBinding;
  resolvePosition(filePath: string, fuzzy: unknown): unknown;
  resolvePositionFromContent(content: string, fuzzy: unknown): unknown;
  toUri(path: string): string;
  fromUri(uri: string): string;
  resolveWorkspaceRootForFile(filePath: string): string;
  detectLanguageId(filePath: string): string | undefined;
  getLanguageServerForFile(
    filePath: string,
    workspaceRoot: string
  ): unknown | undefined;
  isCommandAvailable(command: string): boolean;
  safeReadFile(filePath: string): string;
  validateLspServerPath(command: string): string;
  convertSymbolKind(kind?: number): string;
  toLspSymbolKind(kind: string): number;
};

export const nativeBinding = require('../../index.cjs') as NativeBinding;
