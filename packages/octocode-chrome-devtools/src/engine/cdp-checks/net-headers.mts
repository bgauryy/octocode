// Header redaction shared by the network recipes: names stay, credential values do not.

const SENSITIVE = new Set([
  'authorization',
  'cookie',
  'set-cookie',
  'proxy-authorization',
]);

/** Headers with every credential value replaced by `[redacted N chars]`; other values are kept. */
export function redactHeaders(
  headers: Record<string, unknown> | undefined
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [name, raw] of Object.entries(headers ?? {})) {
    const value = String(raw);
    out[name] = SENSITIVE.has(name.toLowerCase())
      ? `[redacted ${value.length} chars]`
      : value;
  }
  return out;
}
