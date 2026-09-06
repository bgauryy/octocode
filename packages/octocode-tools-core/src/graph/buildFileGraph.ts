import { posix } from 'node:path';
import { contextUtils } from '../utils/contextUtils.js';
import { isValidJsSymbolName } from '../utils/jsSymbolNames.js';
import type {
  FileFacts,
  FileGraphEdgeKind,
  FileNode,
  GraphCoverage,
  RawGraphFacts,
} from './types.js';
import { prepareRustResolver } from './rustWorkspace.js';
import { buildWorkspacePackageExports } from './workspacePackageResolver.js';
import { prepareMetadataImports } from './metadataImports.js';
import { canonicalGraphDiagnostics } from './coverage.js';
import { createFileImportLinker } from './fileImportLinker.js';

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
  coverage?: GraphCoverage;
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
   * Targets consumed as whole namespaces (dynamic/CommonJS loads, namespace
   * imports, and C headers). Retain their public surface because syntax facts
   * cannot attribute the particular exported bindings consumed downstream.
   */
  namespaceImportTargets: Set<string>;
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

/**
 * Scan every graph-fact-capable file under `rootAbsolutePath` in one native
 * worker-pool operation, then resolve import specifiers into a file-level graph.
 * Cross-file policy stays here; filesystem I/O and parsing stay in Rust.
 */
export async function buildFileGraph(
  rootAbsolutePath: string,
  excludeDir: string[],
  maxFiles: number,
  rustWorkspace: 'syntax' | 'cargo' = 'syntax'
): Promise<WalkResult> {
  const scanResult = await contextUtils.scanGraphFacts({
    path: rootAbsolutePath,
    excludeDir,
    maxFiles,
    maxFileBytes: 1_000_000,
  });

  const entries = scanResult.entries;

  const knownFiles = new Set(
    scanResult.candidatePaths.map(relativePath =>
      posix.normalize(relativePath.split('\\').join('/'))
    )
  );
  const workspacePackageExports = buildWorkspacePackageExports(
    rootAbsolutePath,
    knownFiles
  );
  const metadata = prepareMetadataImports(
    rootAbsolutePath,
    knownFiles,
    excludeDir,
    maxFiles
  );

  const facts = new Map<string, FileFacts>();
  const fileGraph = new Map<string, FileNode>();
  const starReexportTargets = new Set<string>();
  const namespaceImportTargets = new Set<string>();
  const starReexporters = new Map<string, string[]>();
  let filesSkipped = scanResult.filesSkipped;
  const coverage: GraphCoverage = {
    basis: 'syntactic',
    referenceBasis: 'lexical-occurrence',
    languages: [],
    imports: {
      resolved: 0,
      external: 0,
      unresolvedInternal: 0,
      unsupported: 0,
    },
    diagnostics: [],
  };

  // Parse once. The module declaration inventory must precede linking so a
  // #[path]/conditional module cannot accidentally resolve via a conventional file.
  const parsedEntries: Array<{
    relativePath: string;
    parsed: RawGraphFacts;
    referenceCounts: (typeof entries)[number]['referenceCounts'];
  }> = [];
  for (const entry of entries) {
    const relativePath = posix.normalize(
      entry.relativePath.split('\\').join('/')
    );
    try {
      const parsed = JSON.parse(entry.factsJson) as RawGraphFacts;
      parsedEntries.push({
        relativePath,
        parsed,
        referenceCounts: entry.referenceCounts,
      });
    } catch {
      filesSkipped++;
    }
  }

  const resolveRust = await prepareRustResolver(
    rootAbsolutePath,
    knownFiles,
    parsedEntries,
    rustWorkspace,
    coverage
  );

  for (const entry of parsedEntries) {
    const { relativePath, parsed } = entry;

    const {
      extension,
      javascript,
      python,
      cFamily,
      auxiliaryTargets,
      resolve,
    } = createFileImportLinker({
      relativePath,
      parsed,
      knownFiles,
      workspacePackageExports,
      metadata,
      resolveRust,
      coverage,
    });

    const declarations = (parsed.declarations ?? [])
      // Drop declarations the native extractor mis-emitted with a reserved-word
      // name (e.g. a Flow file whose `if`/`let` statements parsed as exported
      // functions) — they can never be a real export, and left in they became
      // bogus `if`/`let` dead-export candidates.
      .filter(d => !javascript || isValidJsSymbolName(d.name))
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
        importKind:
          exp.exportKind === 'type' ? ('type' as const) : ('value' as const),
        resolvedTarget: resolve(exp.source as string, exp.line),
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
        resolvedTarget: resolve(
          i.specifier,
          i.line,
          i.importKind === 'module',
          i.resolutionHint,
          i.moduleScope,
          i.importedName
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
      if (extension === 'rs' && target === relativePath) return;
      importsFiles.add(target);
      const kinds = edgeKinds.get(target) ?? new Set<FileGraphEdgeKind>();
      kinds.add(kind);
      if (metadata.targets.has(target) && !kind.startsWith('type-'))
        kinds.add('metadata-import');
      edgeKinds.set(target, kinds);
    };
    for (const imp of fileFacts.imports) {
      const target = imp.resolvedTarget;
      if (target) {
        addEdge(
          target,
          imp.importKind === 'type'
            ? 'type-import'
            : python
              ? 'python-import'
              : cFamily
                ? 'c-include'
                : 'static-import'
        );
        if (imp.importedName === '*' || cFamily)
          namespaceImportTargets.add(target);
      }
    }
    for (const target of auxiliaryTargets) {
      addEdge(target, 'python-import');
      namespaceImportTargets.add(target);
    }
    for (const reexport of fileFacts.namedReexports) {
      const target = reexport.resolvedTarget;
      if (target)
        addEdge(
          target,
          reexport.importKind === 'type'
            ? 'type-named-reexport'
            : 'named-reexport'
        );
    }
    for (const specifier of dynamicImportSpecifiers) {
      const target = resolve(specifier, 0);
      if (target) {
        addEdge(target, 'dynamic-import');
        namespaceImportTargets.add(target);
      }
    }
    for (const load of parsed.commonJs ?? []) {
      if (!load.specifier) {
        coverage.imports.unsupported++;
        coverage.diagnostics.push({
          file: relativePath,
          line: load.line,
          code: 'unsupported-linking',
          message: `CommonJS require cannot be linked (${load.reason ?? 'missing-native-provenance'}).`,
        });
        continue;
      }
      const target = resolve(load.specifier, load.line);
      if (target) {
        addEdge(
          target,
          load.binding === 'create-require'
            ? 'create-require'
            : 'commonjs-require'
        );
        namespaceImportTargets.add(target);
      }
    }
    for (const exp of parsed.exports ?? []) {
      if (!exp.source || exp.name !== '*') continue;
      const target = resolve(exp.source, exp.line);
      if (target) {
        addEdge(
          target,
          exp.exportKind === 'type' ? 'type-star-reexport' : 'star-reexport'
        );
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

  for (const relativePath of metadata.targets) {
    facts.set(relativePath, {
      relativePath,
      declarations: [],
      imports: [],
      namedReexports: [],
      calls: [],
      referenceCounts: new Map(),
    });
    fileGraph.set(relativePath, {
      relativePath,
      importsFiles: new Set(),
      dynamicImportsFiles: new Set(),
      edgeKinds: new Map(),
    });
  }
  if (metadata.targets.size)
    coverage.languages.push({
      language: 'json',
      files: metadata.targets.size,
      linking: 'metadata',
    });

  coverage.diagnostics = canonicalGraphDiagnostics(coverage.diagnostics);
  return {
    coverage,
    facts,
    fileGraph,
    filesScanned: facts.size,
    filesSkipped,
    truncated: scanResult.truncated || metadata.truncated(),
    starReexportTargets,
    namespaceImportTargets,
    starReexporters,
  };
}
