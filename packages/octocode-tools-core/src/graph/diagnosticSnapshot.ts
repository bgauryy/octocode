import { createHash } from 'node:crypto';
import { canonicalGraphDiagnostics } from './coverage.js';
import type { GraphCoverage } from './types.js';

type Diagnostic = GraphCoverage['diagnostics'][number];
interface PreparedDiagnostics {
  diagnostics: Diagnostic[];
  resultId: string;
  diagnosticCounts: Readonly<Record<string, number>>;
}

// Only inventories created and frozen below become keys. Mutable caller arrays
// are never cached, and a new scan always receives a new inventory identity.
const preparedInventories = new WeakMap<
  readonly Diagnostic[],
  PreparedDiagnostics
>();

/** An immutable diagnostic inventory belonging to one completed graph scan. */
export function prepareGraphDiagnostics(
  items: readonly Diagnostic[]
): PreparedDiagnostics {
  const existing = preparedInventories.get(items);
  if (existing) return existing;
  const diagnostics = canonicalGraphDiagnostics(items).map(item =>
    Object.freeze({ ...item })
  );
  Object.freeze(diagnostics);
  const resultId = createHash('sha256')
    .update(
      JSON.stringify(
        diagnostics.map(item => [
          item.file,
          item.line ?? null,
          item.code,
          item.message,
        ])
      )
    )
    .digest('hex');
  const diagnosticCounts: Record<string, number> = {};
  for (const item of diagnostics)
    diagnosticCounts[item.code] = (diagnosticCounts[item.code] ?? 0) + 1;
  const prepared = Object.freeze({
    diagnostics,
    resultId,
    diagnosticCounts: Object.freeze(diagnosticCounts),
  });
  preparedInventories.set(diagnostics, prepared);
  return prepared;
}
