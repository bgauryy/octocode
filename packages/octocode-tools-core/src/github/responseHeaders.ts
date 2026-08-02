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

/**
 * A one-line rate-limit warning when the quota on THIS response is low — read
 * from the `x-ratelimit-*` headers GitHub already returns, so it costs no extra
 * API call. Returns undefined when quota is healthy or the headers are absent.
 */
export function rateLimitWarning(headers: unknown): string | undefined {
  const h = normalizeResponseHeaders(headers);
  const remaining = Number(h['x-ratelimit-remaining']);
  const limit = Number(h['x-ratelimit-limit']);
  const reset = Number(h['x-ratelimit-reset']);
  if (!Number.isFinite(remaining) || !Number.isFinite(limit) || limit <= 0) {
    return undefined;
  }
  // Warn only when genuinely low: under 15% of the window, or ≤3 absolute
  // (search endpoints have a tight ~30/min budget).
  if (remaining > Math.max(3, limit * 0.15)) return undefined;
  const resetIn = Number.isFinite(reset)
    ? Math.max(0, Math.round((reset * 1000 - Date.now()) / 1000))
    : undefined;
  return `GitHub rate limit low: ${remaining}/${limit} remaining${
    resetIn !== undefined ? ` (resets in ~${resetIn}s)` : ''
  } — pace or narrow further requests.`;
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
