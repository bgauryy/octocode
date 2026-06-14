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
