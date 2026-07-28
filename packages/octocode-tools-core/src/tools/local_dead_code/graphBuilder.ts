import { readFileSync } from 'node:fs';
import { posix } from 'node:path';
import { contextUtils } from '../../utils/contextUtils.js';
import { resolveImportSpecifier } from './importResolver.js';
import type { FileFacts, FileNode } from './types.js';

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
  }>;
  calls?: Array<{ caller: string; callee: string }>;
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
export function buildFileGraph(
  rootAbsolutePath: string,
  excludeDir: string[],
  maxFiles: number
): WalkResult {
  const supportedExtensions = contextUtils.getSupportedGraphFactExtensions();

  const queryResult = contextUtils.queryFileSystem({
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

  const facts = new Map<string, FileFacts>();
  const fileGraph = new Map<string, FileNode>();
  const starReexportTargets = new Set<string>();
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

    const declarations = (parsed.declarations ?? []).map(d => ({
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
      }));

    const fileFacts: FileFacts = {
      relativePath,
      declarations,
      imports: (parsed.imports ?? []).map(i => ({
        specifier: i.specifier,
        localName: i.localName,
        importedName: i.importedName,
        line: i.line,
      })),
      namedReexports,
      calls: (parsed.calls ?? []).map(c => ({
        caller: c.caller,
        callee: c.callee,
      })),
      referenceCounts,
    };
    facts.set(relativePath, fileFacts);

    const importsFiles = new Set<string>();
    for (const imp of [...fileFacts.imports, ...fileFacts.namedReexports]) {
      const target = resolveImportSpecifier(
        imp.specifier,
        relativePath,
        knownFiles
      );
      if (target && target !== relativePath) importsFiles.add(target);
    }
    for (const exp of parsed.exports ?? []) {
      if (!exp.source || exp.name !== '*') continue;
      const target = resolveImportSpecifier(
        exp.source,
        relativePath,
        knownFiles
      );
      if (target && target !== relativePath) {
        importsFiles.add(target);
        starReexportTargets.add(target);
      }
    }
    fileGraph.set(relativePath, { relativePath, importsFiles });
  }

  return {
    facts,
    fileGraph,
    filesScanned: facts.size,
    filesSkipped,
    truncated,
    starReexportTargets,
  };
}
