/**
 * Fold keyword terms into one registry query string. Returns undefined when
 * the list is absent or empty.
 */
export function foldKeywords(
  keywords: string[] | undefined
): string | undefined {
  if (keywords === undefined) return undefined;
  const trimmed = keywords.join(' ').trim();
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
