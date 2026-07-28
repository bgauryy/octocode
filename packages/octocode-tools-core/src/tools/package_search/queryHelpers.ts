/**
 * Fold a `keywords` field (string as-is, or an array joined with spaces) into a
 * single registry query string. Sibling tools (ghSearchCode/localSearchCode)
 * take keyword arrays, so agents pass arrays here too — the scheme accepts both
 * shapes and this is where the array is collapsed. Returns undefined when empty.
 */
export function foldKeywords(
  keywords: string | string[] | undefined
): string | undefined {
  if (keywords === undefined) return undefined;
  const joined = Array.isArray(keywords) ? keywords.join(' ') : keywords;
  const trimmed = joined.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

// A genuine "no such package" from the registry (404) is a guided empty, not a
// failure. Network/transport errors ("fetch failed", ENOTFOUND, timeouts) must
// stay hard errors so we never dress a connectivity problem up as "not found".
const PACKAGE_NOT_FOUND_ERROR =
  /\b(?:404|e404)\b|not\s+found|no\s+such\s+package|does\s+not\s+exist/i;

export function isPackageNotFoundError(message: string): boolean {
  return PACKAGE_NOT_FOUND_ERROR.test(message);
}
