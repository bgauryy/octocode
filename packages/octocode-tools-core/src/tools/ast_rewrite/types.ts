import type { AstRewriteQuery as PublicAstRewriteQuery } from '@octocodeai/octocode-core/schema';

/** Shared match shape: produced by the native engine and consumed by prepare. */
export interface AstGrepJsonMatch {
  file: string;
  text: string;
  replacement: string;
  range: {
    byteOffset: { start: number; end: number };
    start: { line: number; column: number };
    end: { line: number; column: number };
  };
  replacementOffsets?: { start: number; end: number };
  metaVariables?: {
    single: Record<string, { text: string }>;
    multi: Record<string, Array<{ text: string }>>;
    transformed: Record<string, string>;
  };
}

export type AstRewriteQuery = PublicAstRewriteQuery;

export interface AstRewriteCapture {
  kind: 'single' | 'multi' | 'transformed';
  texts: string[];
}

export interface AstRewriteMatch {
  id: string;
  path: string;
  byteRange: { start: number; end: number };
  range: {
    start: { line: number; column: number };
    end: { line: number; column: number };
  };
  text: string;
  replacement: string;
  captures: Record<string, AstRewriteCapture>;
}

export interface AstRewriteFile {
  path: string;
  absolutePath: string;
  beforeHash: string;
  afterHash: string;
  matchCount: number;
  patch: string;
  patchBytes: number;
}

export interface PreparedFile extends AstRewriteFile {
  before: Buffer;
  after: Buffer;
  mode: number;
  matches: AstRewriteMatch[];
}

export interface AstRewriteSuccess {
  status?: undefined;
  operation: 'rewrite';
  mode: 'preview' | 'apply';
  root: string;
  snapshot: string;
  executable: AstRewriteExecutableReceipt;
  isolation: AstRewriteIsolationReceipt;
  totalMatches: number;
  affectedFiles: number;
  matches: AstRewriteMatch[];
  files: AstRewriteFile[];
  complete: boolean;
  isPartial: boolean;
  pagination: {
    currentPage: number;
    totalPages: number;
    pageSize: number;
    hasMore: boolean;
  };
  next?: {
    nextPage: {
      tool: 'astRewrite';
      query: AstRewriteQuery;
      confidence: 'exact';
    };
  };
  transaction?: {
    id: string;
    committed: true;
    files: number;
    cleanupWarnings?: string[];
    beforeHashes: Record<string, string>;
    afterHashes: Record<string, string>;
  };
}

export interface AstRewriteEmpty {
  status: 'empty';
  operation: 'rewrite';
  mode: 'preview' | 'apply';
  root: string;
  executable: AstRewriteExecutableReceipt;
  isolation: AstRewriteIsolationReceipt;
  totalMatches: 0;
  affectedFiles: 0;
  matches: [];
  files: [];
  complete: true;
  isPartial: false;
}

export interface AstRewriteError {
  status: 'error';
  operation: 'rewrite';
  errorCode: string;
  error: string;
  complete?: false;
  isPartial?: true;
  terminalLimit?: boolean;
  details?: Record<string, unknown>;
  next?: {
    restart: {
      tool: 'astRewrite';
      query: AstRewriteQuery;
      confidence: 'exact';
    };
  };
}

export type AstRewriteResult =
  AstRewriteSuccess | AstRewriteEmpty | AstRewriteError;

export interface AstRewriteRuntimeDeps {
  executable?: string;
  allowApply?: boolean;
  timeoutMs?: number;
  maxProcessOutputBytes?: number;
  maxPatchBytes?: number;
  rename?: (from: string, to: string) => Promise<void>;
  lockTimeoutMs?: number;
  lockPollMs?: number;
}

export interface AstRewriteExecutableReceipt {
  path: string;
  version: string;
  sha256: string;
  capabilityContract: 1;
  capabilityDigest: string;
  capabilities: string[];
}

export interface AstRewriteIsolationReceipt {
  workingDirectory: 'ephemeral';
  inheritedHome: false;
  repositoryConfig: 'not-discovered';
}
