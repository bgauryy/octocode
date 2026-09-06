// Internal graph types shared by every localAnalyzeGraph operation. One node per source file;
// declarations/imports/calls come from the native `extractGraphFacts` pass
// (per-file only — cross-file linking happens here, not in Rust).

export interface ImportResolution {
  target: string | null;
  status: 'resolved' | 'external' | 'unresolvedInternal' | 'unsupported';
  additionalTargets?: string[];
  /** Stable explanation for a bounded resolver's unavailable target or context. */
  reason?: string;
}

export interface DeclarationFact {
  /** Source occurrence identity; never a canonical cross-file symbol ID. */
  id: string;
  name: string;
  kind: string;
  line: number;
  exported: boolean;
}

export interface GraphCoverage {
  /** Public response pagination; graph construction retains the full diagnostic list. */
  diagnosticsPagination?: {
    currentPage: number;
    totalPages: number;
    entriesPerPage: number;
    totalEntries: number;
    hasMore: boolean;
    resultId: string;
    outOfRange?: boolean;
    terminalLimit?: boolean;
  };
  diagnosticCounts?: Record<string, number>;
  basis: 'syntactic';
  referenceBasis: 'lexical-occurrence';
  languages: Array<{
    language: string;
    files: number;
    linking:
      | 'javascript-relative'
      | 'rust-modules'
      | 'python-modules'
      | 'c-relative-includes'
      | 'metadata'
      | 'unsupported';
  }>;
  imports: {
    resolved: number;
    external: number;
    unresolvedInternal: number;
    unsupported: number;
  };
  diagnostics: Array<{
    file: string;
    line?: number;
    code:
      | 'parse-recovery'
      | 'unsupported-linking'
      | 'unresolved-internal'
      | 'syntax-only';
    message: string;
  }>;
}

export interface ImportFact {
  specifier: string;
  localName: string;
  importedName: string;
  line: number;
  /** Native syntax classification; `module` denotes a Rust module declaration. */
  importKind: 'type' | 'value' | 'module';
  /**
   * Canonical target resolved during graph construction with the complete
   * workspace package-export map. `null` means the specifier is external or
   * unsupported by the bounded resolver.
   */
  resolvedTarget: string | null;
}

export interface CallEdgeFact {
  caller: string;
  callee: string;
}

export interface FileFacts {
  relativePath: string;
  declarations: DeclarationFact[];
  imports: ImportFact[];
  /**
   * Named re-exports (`export { x } from './y.js'`) declared in this file.
   * Kept separate from `imports`: a re-export is a pass-through, not a
   * consuming usage — `x` is only actually live if *this file's own* export
   * of it (`localName`) is itself consumed somewhere, directly or through a
   * further re-export. Folding these into `imports` would make merely being
   * re-exported look like proof of use, which false-negatives on exactly the
   * "barrel re-exports more than anyone downstream imports" case.
   */
  namedReexports: ImportFact[];
  calls: CallEdgeFact[];
  /**
   * Whole-word occurrence count of each declared name in the file's raw
   * source. `calls` only covers function-invocation edges; a name used by
   * value reference (spread, property access, passed as an argument) never
   * appears there, so this is the fallback that catches same-file usage of
   * exported constants/types. A count > 1 means the name appears somewhere
   * besides its own declaration line.
   */
  referenceCounts: Map<string, number>;
}

export interface FileNode {
  relativePath: string;
  /** Resolved relative paths of files this file imports from — static and
   * string-literal dynamic `import()` targets alike. */
  importsFiles: Set<string>;
  /**
   * Subset of `importsFiles` reached only through a string-literal dynamic
   * `import()`, not a static `import`/`export ... from`. Used to compute a
   * static-only reachability pass so a file reachable exclusively through
   * dynamic import can be flagged as lower-confidence rather than treated
   * with the same certainty as a statically-proven path.
   */
  dynamicImportsFiles: Set<string>;
  /** Exact syntactic provenance for every resolved file edge. */
  edgeKinds: Map<string, Set<FileGraphEdgeKind>>;
}

export type FileGraphEdgeKind =
  | 'static-import'
  | 'type-import'
  | 'dynamic-import'
  | 'named-reexport'
  | 'star-reexport'
  | 'type-named-reexport'
  | 'type-star-reexport'
  | 'commonjs-require'
  | 'create-require'
  | 'metadata-import'
  | 'python-import'
  | 'rust-module'
  | 'rust-use'
  | 'c-include';

export interface RawGraphFacts {
  language?: string;
  commonJs?: Array<{
    specifier?: string;
    line: number;
    kind: 'commonjs-require';
    binding: 'unshadowed-global' | 'create-require';
    reason?: string;
  }>;
  diagnostics?: string[];
  rustRootUnsupported?: boolean;
  modules?: Array<{
    name: string;
    line: number;
    scope: string[];
    inline: boolean;
    path?: string;
    unsupported?: boolean;
  }>;
  declarations?: Array<{
    id: string;
    name: string;
    kind: string;
    line: number;
    exported?: boolean;
  }>;
  imports?: Array<{
    specifier: string;
    localName: string;
    importedName: string;
    line: number;
    importKind?: string;
    resolutionHint?: string;
    moduleScope?: string[];
  }>;
  calls?: Array<{
    caller: string;
    callee: string;
    kind?: string;
    line?: number;
  }>;
  exports?: Array<{
    name: string;
    line: number;
    localName?: string;
    exportKind?: string;
    /** Present only for a re-export (`export ... from '...'`). */
    source?: string;
  }>;
}
