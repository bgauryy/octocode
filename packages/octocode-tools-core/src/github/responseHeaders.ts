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
  }
  return out;
}

/** Prefer strong ETag; fall back to weak / case variants GitHub may emit. */
export function extractEtag(headers: unknown): string | undefined {
  const normalized = normalizeResponseHeaders(headers);
  const etag =
    normalized.etag ||
    normalized.ETag ||
    normalized['Etag'] ||
    Object.entries(normalized).find(([k]) => k.toLowerCase() === 'etag')?.[1];
  return etag && etag.length > 0 ? etag : undefined;
}
