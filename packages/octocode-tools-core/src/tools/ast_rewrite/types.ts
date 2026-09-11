export interface AstRewriteQuery {
  path: string;
  langType: string;
  pattern: string;
  rewrite: string;
  include?: string[];
  exclude?: string[];
  apply?: boolean;
  expectedHashes?: Record<string, string>;
  maxFiles?: number;
  page?: number;
  pageSize?: number;
  snapshot?: string;
  goal?: string;
  reasoning?: string;
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

export interface AstRewriteSuccess {
  status?: undefined;
  operation: 'rewrite';
  mode: 'preview' | 'apply';
  root: string;
  snapshot: string;
  executable: { path: string; version: string };
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
  executable: { path: string; version: string };
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
  complete: false;
  isPartial: true;
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
}
