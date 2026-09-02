import { posix } from 'node:path';
import { contextUtils } from '../utils/contextUtils.js';
import { isValidJsSymbolName } from '../utils/jsSymbolNames.js';
import { resolveImportSpecifier } from './importResolver.js';
import type { FileFacts, FileGraphEdgeKind, FileNode } from './types.js';
import { buildWorkspacePackageExports } from './workspacePackageResolver.js';

export const DEFAULT_DEAD_CODE_EXCLUDE_DIRS = [
  'node_modules',
  'dist',
  'build',
  'out',
  'coverage',
  '.git',
  'target',
  '.next',
  '.cache',
];

/** Keep generated/vendor defaults while allowing callers to add repo-specific exclusions. */
export function resolveGraphExcludeDirs(
  custom: readonly string[] | undefined
): string[] {
  return [...new Set([...DEFAULT_DEAD_CODE_EXCLUDE_DIRS, ...(custom ?? [])])];
}

export interface WalkResult {
  facts: Map<string, FileFacts>;
  fileGraph: Map<string, FileNode>;
  filesScanned: number;
  filesSkipped: number;
  truncated: boolean;
  /**
   * Files that are the target of a `export * from '...'` somewhere in the
   * scan. A star re-export re-publishes its target's entire surface, so once
   * such a target is reachable its exports are the public API — same
   * treatment as an entrypoint file, not individually retention-checked.
   */
  starReexportTargets: Set<string>;
  /**
   * Files that are the target of a string-literal dynamic `import('./x')`
   * somewhere in the scan. A dynamic import resolves the whole module
   * namespace, not a specific named binding, so — unlike a static
   * `import { x } from './y'` — there is no reliable way to attribute which
   * of the target's exports are actually used. Treated the same as
   * `starReexportTargets`: reachable, and not individually retention-checked,
   * rather than risk a false-positive "unreferenced export" on a name that's
   * genuinely read off the dynamically-imported namespace object.
   */
  dynamicImportTargets: Set<string>;
  /**
   * Reverse star-re-export edges: target file → the files that
   * `export * from` it. Needed for retention: a NAMED re-export whose barrel
   * is itself star-re-exported (barrel → `export *` → entrypoint) is public
   * API — `isBindingLive` must be able to hop the star edge, which carries no
   * per-name entry in the named `reexportIndex`. Unfiltered (all scanned
   * files); the scan filters to reachable re-exporters before use.
   */
  starReexporters: Map<string, string[]>;
}

interface RawGraphFacts {
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
  }>;
  calls?: Array<{ caller: string; callee: string; kind?: string }>;
  exports?: Array<{
    name: string;
    line: number;
    localName?: string;
    /** Present only for a re-export (`export ... from '...'`). */
    source?: string;
  }>;
}

/**
 * Scan every graph-fact-capable file under `rootAbsolutePath` in one native
 * worker-pool operation, then resolve import specifiers into a file-level graph.
 * Cross-file policy stays here; filesystem I/O and parsing stay in Rust.
 */
export async function buildFileGraph(
  rootAbsolutePath: string,
  excludeDir: string[],
  maxFiles: number
): Promise<WalkResult> {
  const scanResult = await contextUtils.scanGraphFacts({
    path: rootAbsolutePath,
    excludeDir,
    maxFiles,
    maxFileBytes: 1_000_000,
  });

  const entries = scanResult.entries;
  const truncated = scanResult.truncated;

  const knownFiles = new Set(
    scanResult.candidatePaths.map(relativePath =>
      posix.normalize(relativePath.split('\\').join('/'))
    )
  );
  const workspacePackageExports = buildWorkspacePackageExports(
    rootAbsolutePath,
    knownFiles
  );

  const facts = new Map<string, FileFacts>();
  const fileGraph = new Map<string, FileNode>();
  const starReexportTargets = new Set<string>();
  const dynamicImportTargets = new Set<string>();
  const starReexporters = new Map<string, string[]>();
  let filesSkipped = scanResult.filesSkipped;

  for (const entry of entries) {
    const relativePath = posix.normalize(
      entry.relativePath.split('\\').join('/')
    );
    let parsed: RawGraphFacts;
    try {
      parsed = JSON.parse(entry.factsJson) as RawGraphFacts;
    } catch {
      filesSkipped++;
      continue;
    }

    const declarations = (parsed.declarations ?? [])
      // Drop declarations the native extractor mis-emitted with a reserved-word
      // name (e.g. a Flow file whose `if`/`let` statements parsed as exported
      // functions) — they can never be a real export, and left in they became
      // bogus `if`/`let` dead-export candidates.
      .filter(d => isValidJsSymbolName(d.name))
      .map(d => ({
        id: d.id,
        name: d.name,
        kind: d.kind,
        line: d.line,
        exported: d.exported ?? false,
      }));

    const referenceCounts = new Map(
      entry.referenceCounts.map(({ name, count }) => [name, count])
    );

    // `export { x } from './y.js'` re-exports a specific name from another
    // file. Kept separate from `imports` (see FileFacts.namedReexports) —
    // it's a pass-through, not a consuming usage. `export * from` has no
    // single name and is handled separately below.
    const namedReexports = (parsed.exports ?? [])
      .filter(exp => exp.source && exp.name !== '*')
      .map(exp => ({
        specifier: exp.source as string,
        localName: exp.localName ?? exp.name,
        importedName: exp.name,
        line: exp.line,
        importKind: 'value' as const,
        resolvedTarget: resolveImportSpecifier(
          exp.source as string,
          relativePath,
          knownFiles,
          workspacePackageExports
        ),
      }));

    // String-literal dynamic `import('./x')` specifiers resolve to a file the
    // same way a static import does — kept separate from `calls` (which is
    // real call-graph edges, not module linking) and from `imports` (which
    // carries local/imported binding names a dynamic import doesn't have).
    const dynamicImportSpecifiers = (parsed.calls ?? [])
      .filter(c => c.kind === 'dynamic-import')
      .map(c => c.callee);

    const fileFacts: FileFacts = {
      relativePath,
      declarations,
      imports: (parsed.imports ?? []).map(i => ({
        specifier: i.specifier,
        localName: i.localName,
        importedName: i.importedName,
        line: i.line,
        importKind: i.importKind === 'type' ? 'type' : 'value',
        resolvedTarget: resolveImportSpecifier(
          i.specifier,
          relativePath,
          knownFiles,
          workspacePackageExports
        ),
      })),
      namedReexports,
      calls: (parsed.calls ?? [])
        .filter(c => c.kind !== 'dynamic-import')
        .map(c => ({
          caller: c.caller,
          callee: c.callee,
        })),
      referenceCounts,
    };
    facts.set(relativePath, fileFacts);

    const importsFiles = new Set<string>();
    const dynamicImportsFiles = new Set<string>();
    const edgeKinds = new Map<string, Set<FileGraphEdgeKind>>();
    const addEdge = (target: string, kind: FileGraphEdgeKind): void => {
      importsFiles.add(target);
      const kinds = edgeKinds.get(target) ?? new Set<FileGraphEdgeKind>();
      kinds.add(kind);
      edgeKinds.set(target, kinds);
    };
    for (const imp of fileFacts.imports) {
      const target = imp.resolvedTarget;
      if (target)
        addEdge(
          target,
          imp.importKind === 'type' ? 'type-import' : 'static-import'
        );
    }
    for (const reexport of fileFacts.namedReexports) {
      const target = reexport.resolvedTarget;
      if (target) addEdge(target, 'named-reexport');
    }
    for (const specifier of dynamicImportSpecifiers) {
      const target = resolveImportSpecifier(
        specifier,
        relativePath,
        knownFiles,
        workspacePackageExports
      );
      if (target) {
        addEdge(target, 'dynamic-import');
        dynamicImportTargets.add(target);
      }
    }
    for (const exp of parsed.exports ?? []) {
      if (!exp.source || exp.name !== '*') continue;
      const target = resolveImportSpecifier(
        exp.source,
        relativePath,
        knownFiles,
        workspacePackageExports
      );
      if (target) {
        addEdge(target, 'star-reexport');
        starReexportTargets.add(target);
        const list = starReexporters.get(target) ?? [];
        list.push(relativePath);
        starReexporters.set(target, list);
      }
    }
    for (const [target, kinds] of edgeKinds) {
      // This set intentionally means dynamic-only. A target reached by both a
      // static and dynamic edge remains statically proven.
      if (kinds.size === 1 && kinds.has('dynamic-import')) {
        dynamicImportsFiles.add(target);
      }
    }
    fileGraph.set(relativePath, {
      relativePath,
      importsFiles,
      dynamicImportsFiles,
      edgeKinds,
    });
  }

  return {
    facts,
    fileGraph,
    filesScanned: facts.size,
    filesSkipped,
    truncated,
    starReexportTargets,
    dynamicImportTargets,
    starReexporters,
  };
}
