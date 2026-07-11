import { EXIT, classifyToolErrorText } from '../../exit-codes.js';
import {
  type OqlRunResult,
  isBatchEnvelope,
} from '@octocodeai/octocode-tools-core/oql';

export function exitCodeFor(result: OqlRunResult): number {
  const envelopes = isBatchEnvelope(result)
    ? result.children.map(c => c.envelope)
    : [result];
  for (const env of envelopes) {
    if (env.diagnostics.some(d => d.code === 'rateLimited'))
      return EXIT.RATE_LIMIT;
    // Semantic miss (symbol not found at the anchor) honors the documented
    // exit-3 contract so scripts fail fast instead of reading refs=0 as proof.
    if (env.diagnostics.some(d => d.code === 'symbolNotFound'))
      return EXIT.NOT_FOUND;
    // Classify the error-bearing diagnostics by message text the same way the
    // direct-tool path does, so genuine not-found (3) / auth (4) / rate-limit
    // (7) failures are reachable through `search`. A diagnostic that carries no
    // such signal classifies as TOOL — for an `invalidQuery` that means a truly
    // malformed query, which stays USAGE (2).
    const invalidQuery = env.diagnostics.find(d => d.code === 'invalidQuery');
    if (invalidQuery) {
      const classified = classifyToolErrorText(invalidQuery.message);
      return classified === EXIT.TOOL ? EXIT.USAGE : classified;
    }
    if (env.evidence.kind === 'unsupported') return EXIT.TOOL;
  }
  return EXIT.OK;
}
