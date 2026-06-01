/**
 * Validated narrowing for HTTP response headers.
 *
 * Octokit/provider responses type `headers` as
 * `{ [name: string]: string | number | undefined }` (and at the wire boundary
 * it is genuinely untrusted `unknown`). Downstream code wants a clean
 * `Record<string, string>`. Rather than an `as unknown as Record<string,string>`
 * cast — which silently lies if a non-string slips through — this validates
 * each entry at the boundary: string values pass through, finite numbers are
 * stringified, and everything else (undefined/null/objects) is dropped.
 */
export function normalizeResponseHeaders(
  headers: unknown
): Record<string, string> {
  const out: Record<string, string> = {};
  if (!headers || typeof headers !== 'object') return out;
  for (const [key, value] of Object.entries(
    headers as Record<string, unknown>
  )) {
    if (typeof value === 'string') {
      out[key] = value;
    } else if (typeof value === 'number' && Number.isFinite(value)) {
      out[key] = String(value);
    }
    // undefined / null / arrays / objects are intentionally dropped — a header
    // map is string→string by contract; anything else is not a usable header.
  }
  return out;
}
