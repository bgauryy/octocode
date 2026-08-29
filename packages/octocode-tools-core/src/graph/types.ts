// Internal graph types shared by every localAnalyzeGraph operation. One node per source file;
// declarations/imports/calls come from the native `extractGraphFacts` pass
// (per-file only — cross-file linking happens here, not in Rust).

export interface DeclarationFact {
  id: string;
  name: string;
  kind: string;
  line: number;
  exported: boolean;
}

export interface ImportFact {
  specifier: string;
  localName: string;
  importedName: string;
  line: number;
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
  'static-import' | 'dynamic-import' | 'named-reexport' | 'star-reexport';
