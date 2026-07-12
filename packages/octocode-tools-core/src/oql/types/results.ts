/**
 * OQL result rows — the per-kind row shapes a result envelope's `results[]`
 * can hold (code/file/tree/content matches, and the generic `record` row
 * used by typed research objects; see record-data.ts / record-rows.ts for
 * the `recordType`-specific `data` payloads and aliases).
 */

import type { QuerySource } from './predicates.js';
import type { OqlContinuation } from './envelope.js';

export type OqlProofGrade =
  'candidate' | 'text' | 'structural' | 'semantic' | 'graph' | 'missing';

interface OqlProofGradedRow {
  /**
   * Mandatory on rows emitted by `runOqlSearch`; adapters may omit it while the
   * runner computes the final proof grade from query semantics and row shape.
   */
  proofGrade?: OqlProofGrade;
}

export interface OqlCodeResultRow extends OqlProofGradedRow {
  kind: 'code';
  source?: QuerySource;
  path: string;
  /**
   * 1-based match line. Optional because some providers (GitHub code search)
   * return path-level matches with no line — never fabricate one.
   */
  line?: number;
  endLine?: number;
  column?: number;
  snippet?: string;
  /** File-level count payloads from local search count modes. */
  totalMatchedLines?: number;
  totalOccurrences?: number;
  /** Provider snippet offsets when available, e.g. GitHub code search indices. */
  matchIndices?: Array<{ start: number; end: number; lineOffset?: number }>;
  /** Row-level provider/context metadata that is useful but not identity. */
  metadata?: Record<string, unknown>;
  /**
   * Structural (AST) metavariable captures. `$X` → single-element list; `$$$X`
   * → node list. Keyed by bare metavar name. Present only for structural
   * matches that captured; never fabricated.
   */
  metavars?: Record<string, string[]>;
  /** Per-capture source ranges (uri+line ready for lspGetSemantics handoff). */
  metavarRanges?: Record<
    string,
    {
      text: string;
      line: number;
      column: number;
      endLine: number;
      endColumn: number;
    }[]
  >;
  /**
   * Deterministic AST match-kind label from the engine classifier
   * (declaration|import|export|callsite|identifier|comment|string|…).
   * Lexical coverage signal — not semantic identity.
   */
  matchKind?: string;
  /** Fixed table derived from matchKind (0.0..1.0); not relevance ranking. */
  scoreHint?: number;
  /**
   * Executable follow-up continuations. Keys are dotted *domain names*
   * (`next.<domain>`, e.g. `next.fetch`, `next.semantic`, `next.charRange`),
   * NOT nested object paths — the registry in run.ts owns the key set.
   */
  next?: Record<string, OqlContinuation>;
}

export interface OqlFileResultRow extends OqlProofGradedRow {
  kind: 'file';
  source?: QuerySource;
  path: string;
  entryType: 'file' | 'directory';
  size?: number;
  modified?: string;
  next?: Record<string, OqlContinuation>;
}

export interface OqlTreeResultRow extends OqlProofGradedRow {
  kind: 'tree';
  source?: QuerySource;
  path: string;
  entryType: 'file' | 'directory';
  depth: number;
  size?: number;
  children?: OqlTreeResultRow[];
  next?: Record<string, OqlContinuation>;
}

export interface OqlContentResultRow extends OqlProofGradedRow {
  kind: 'content';
  source?: QuerySource;
  path: string;
  content: string;
  range?: {
    startLine?: number;
    endLine?: number;
    charOffset?: number;
    charLength?: number;
  };
  contentView: 'none' | 'standard' | 'symbols';
  next?: Record<string, OqlContinuation>;
}

/**
 * Generic record row for targets whose payload is a typed research object
 * (repository, package, PR, commit, symbol/location, diff, packet).
 * `recordType` names the family; `data` is the row payload.
 */
export interface OqlRecordResultRow extends OqlProofGradedRow {
  kind: 'record';
  recordType:
    | 'semantics'
    | 'repository'
    | 'package'
    | 'pullRequest'
    | 'commit'
    | 'diff'
    | 'research'
    | 'graph'
    | 'materialized';
  /** Stable, citeable identity (repo, name@version, #PR, SHA, path, uri). */
  id?: string;
  source?: QuerySource;
  /** Parent/query metadata preserved from the backing tool payload. */
  metadata?: Record<string, unknown>;
  data: Record<string, unknown>;
  next?: Record<string, OqlContinuation>;
}
