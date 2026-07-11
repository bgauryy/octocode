/** Quotes a GitHub search keyword when it contains whitespace, so multi-word phrases stay one term. */
export function quoteSearchKeyword(kw: string): string {
  if (kw.startsWith('"')) return kw;
  if (/\s/.test(kw)) return `"${kw.replace(/"/g, '\\"')}"`;
  return kw;
}
