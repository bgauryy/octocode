/**
 * Verbose helper — single boolean detail switch shared by all tools.
 *
 *   verbose: false (default) — efficient research data.
 *   verbose: true            — research data plus extended metadata.
 */

export function isVerbose(query: { verbose?: boolean }): boolean {
  return query.verbose === true;
}
