import type {
  FileSystemEntry,
  FileSystemQueryResult,
} from '../../utils/contextUtils.js';

/**
 * Expand shell-style brace alternation (`{a,b,c}`) in a glob pattern into the
 * cartesian product of concrete patterns. The native glob compiler (a
 * hand-rolled glob-to-regex translator, see `octocode-engine`'s `fs_query.rs`)
 * has no brace support — a pattern like `packages/{react,react-reconciler}/**`
 * previously compiled to a regex that silently matched nothing, with no error.
 * Returns the input unchanged as a single-element array when it has no brace
 * groups, so patterns without braces see no behavior change.
 */
export function expandBracePattern(pattern: string): string[] {
  const braceStart = pattern.indexOf('{');
  if (braceStart === -1) return [pattern];

  const braceEnd = pattern.indexOf('}', braceStart);
  if (braceEnd === -1) return [pattern];

  const before = pattern.slice(0, braceStart);
  const alternatives = pattern.slice(braceStart + 1, braceEnd).split(',');
  const after = pattern.slice(braceEnd + 1);

  // Re-assemble with each alternative substituted, then recurse so a further
  // brace group later in the pattern (in `after`) also expands — `before` and
  // `alt` are already brace-free at this point, so the next `indexOf('{')`
  // in the recursive call can only find a group still in `after`.
  return alternatives.flatMap(alt =>
    expandBracePattern(`${before}${alt}${after}`)
  );
}

/**
 * Merge multiple native `queryFileSystem` results (one per expanded brace
 * alternative) into a single result shape, deduping entries that matched more
 * than one alternative. `totalDiscovered`/`skipped`/`permissionDenied` are
 * summed per-walk (not deduped) — each expanded pattern walks the tree
 * independently, so this is an upper bound on true unique discovery, not an
 * exact count; `wasCapped` is true if any alternative hit its cap.
 */
export function mergeFileSystemQueryResults(
  results: readonly FileSystemQueryResult[]
): FileSystemQueryResult {
  const entriesByPath = new Map<string, FileSystemEntry>();
  let totalDiscovered = 0;
  let wasCapped = false;
  let skipped = 0;
  let permissionDenied = 0;
  const warnings = new Set<string>();

  for (const result of results) {
    for (const entry of result.entries) {
      if (!entriesByPath.has(entry.path)) entriesByPath.set(entry.path, entry);
    }
    totalDiscovered += result.totalDiscovered;
    wasCapped = wasCapped || result.wasCapped;
    skipped += result.skipped;
    permissionDenied += result.permissionDenied;
    for (const warning of result.warnings) warnings.add(warning);
  }

  return {
    entries: [...entriesByPath.values()],
    totalDiscovered,
    wasCapped,
    skipped,
    permissionDenied,
    warnings: [...warnings],
  };
}
