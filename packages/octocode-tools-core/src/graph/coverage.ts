import type { GraphCoverage, RawGraphFacts } from './types.js';

type Diagnostic = GraphCoverage['diagnostics'][number];

export function recordUnlinkedCommonJsCalls(
  coverage: GraphCoverage,
  file: string,
  calls: RawGraphFacts['calls']
): void {
  for (const call of calls ?? []) {
    if (call.callee !== 'require' && call.callee !== 'module.require') continue;
    // Native call facts retain callee/line but not require arguments or
    // binding identity. Do not invent an edge from a spelling alone.
    coverage.imports.unsupported++;
    coverage.diagnostics.push({
      file,
      ...(call.line !== undefined ? { line: call.line } : {}),
      code: 'unsupported-linking',
      message:
        'CommonJS require calls are not linked: native call facts do not establish module arguments or binding identity. Verify dependencies with LSP.',
    });
  }
}

/** One diagnostic per source occurrence, independent of binding or scan order. */
export function canonicalGraphDiagnostics(
  diagnostics: readonly Diagnostic[]
): Diagnostic[] {
  const unique = new Map<string, Diagnostic>();
  for (const item of diagnostics) {
    const key = JSON.stringify([
      item.file,
      item.line ?? null,
      item.code,
      item.message,
    ]);
    if (!unique.has(key)) unique.set(key, item);
  }
  const compare = (left: string, right: string): number =>
    left < right ? -1 : left > right ? 1 : 0;
  return [...unique.values()].sort(
    (left, right) =>
      compare(left.file, right.file) ||
      (left.line ?? -1) - (right.line ?? -1) ||
      compare(left.code, right.code) ||
      compare(left.message, right.message)
  );
}
