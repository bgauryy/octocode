import { createRequire } from 'node:module';

import type { ExactPosition, FuzzyPosition, LspReadiness } from './types.js';

const require = createRequire(import.meta.url);

// The wrapper and addon ship together; these methods are the required contract.
export type NativeLspClientBinding = {
  start(): Promise<void>;
  stop(): Promise<void>;
  waitForReady(timeoutMs?: number): Promise<LspReadiness>;
  hasCapability(capability: string): boolean;
  isAlive(): Promise<boolean>;
  getRecentStderr(): string[];
  openDocument(filePath: string, content: string): Promise<void>;
  closeDocument(filePath: string): Promise<void>;
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

interface ResolvedSymbol {
  position: ExactPosition;
  foundAtLine: number;
  lineOffset: number;
  lineContent: string;
}

type NativeBinding = {
  NativeLspClient: new (config: unknown) => NativeLspClientBinding;
  resolvePosition(filePath: string, fuzzy: FuzzyPosition): ResolvedSymbol;
  resolvePositionFromContent(content: string, fuzzy: FuzzyPosition): ResolvedSymbol;
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
  safeReadLineWindow(
    filePath: string,
    lineZeroBased: number,
    contextLines: number
  ): string;
  validateLspServerPath(command: string): string;
};

// Embedded single-file builds (Node SEA) pre-load the addon and publish it on
// globalThis; the createRequire path below cannot resolve once this file is
// inlined into a bundle with no package files on disk.
export const nativeBinding = ((
  globalThis as { __OCTOCODE_ENGINE_BINDING__?: unknown }
).__OCTOCODE_ENGINE_BINDING__ ?? require('../../index.cjs')) as NativeBinding;
