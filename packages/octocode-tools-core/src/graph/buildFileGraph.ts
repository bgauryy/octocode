import { readFileSync } from 'node:fs';
import { posix } from 'node:path';
import { contextUtils } from '../utils/contextUtils.js';
import { isValidJsSymbolName } from '../utils/jsSymbolNames.js';
import { resolveImportSpecifier } from './importResolver.js';
import type { FileFacts, FileGraphEdgeKind, FileNode } from './types.js';
import { buildWorkspacePackageExports } from './workspacePackageResolver.js';

// Mirrors the engine's own MAX_STRUCTURAL_CONTENT_BYTES backstop: a file this
// large is skipped rather than risking a slow parse for a dead-code scan
// where speed at repo scale is the point.
const MAX_FILE_BYTES = 1_000_000;

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

function escapeRegExp(literal: string): string {
  return literal.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Count whole-word occurrences of `name` in `content`, including its own
 * declaration line — callers compare against 1 (declaration-only) to detect
 * same-file usage. Deliberately text-based rather than AST-based: it can
 * over-count (a comment or string mentioning the name) but never under-counts
 * a real reference, which is the safe direction for a dead-code signal.
 */
function countWholeWordOccurrences(content: string, name: string): number {
  const matches = content.match(new RegExp(`\\b${escapeRegExp(name)}\\b`, 'g'));
  return matches ? matches.length : 0;
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
 * Walk every graph-fact-capable file under `rootAbsolutePath`, extract native
 * per-file facts, and resolve import specifiers into a file-level graph. One
 * native call per file, zero LSP round-trips — this is what makes the scan
 * scale with repo size instead of candidate count.
 */
export async function buildFileGraph(
  rootAbsolutePath: string,
  excludeDir: string[],
  maxFiles: number
): Promise<WalkResult> {
  const supportedExtensions = contextUtils.getSupportedGraphFactExtensions();

  const queryResult = await contextUtils.queryFileSystem({
    path: rootAbsolutePath,
    recursive: true,
    showHidden: false,
    entryType: 'f',
    extensions: supportedExtensions,
    excludeDir,
    stopAtLimit: true,
    limit: maxFiles,
  });

  const entries = queryResult.entries;
  const truncated = entries.length >= maxFiles;

  const knownFiles = new Set(
    entries.map(entry =>
      posix.normalize(entry.relativePath.split('\\').join('/'))
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
  let filesSkipped = 0;

  for (const entry of entries) {
    const relativePath = posix.normalize(
      entry.relativePath.split('\\').join('/')
    );
    if ((entry.size ?? 0) > MAX_FILE_BYTES) {
      filesSkipped++;
      continue;
    }

    let content: string;
    try {
      content = readFileSync(entry.path, 'utf-8');
    } catch {
      filesSkipped++;
      continue;
    }

    const rawJson = contextUtils.extractGraphFacts(content, relativePath);
    if (!rawJson) {
      filesSkipped++;
      continue;
    }

    let parsed: RawGraphFacts;
    try {
      parsed = JSON.parse(rawJson) as RawGraphFacts;
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

    const referenceCounts = new Map<string, number>();
    for (const decl of declarations) {
      if (!decl.exported || referenceCounts.has(decl.name)) continue;
      referenceCounts.set(
        decl.name,
        countWholeWordOccurrences(content, decl.name)
      );
    }

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
      const target = resolveImportSpecifier(
        imp.specifier,
        relativePath,
        knownFiles,
        workspacePackageExports
      );
      if (target)
        addEdge(
          target,
          imp.importKind === 'type' ? 'type-import' : 'static-import'
        );
    }
    for (const reexport of fileFacts.namedReexports) {
      const target = resolveImportSpecifier(
        reexport.specifier,
        relativePath,
        knownFiles,
        workspacePackageExports
      );
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
