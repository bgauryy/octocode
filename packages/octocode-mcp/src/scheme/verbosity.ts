export function isVerbose(query: { verbose?: boolean }): boolean {
  return query.verbose === true;
}
