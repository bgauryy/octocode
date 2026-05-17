const RAW_RESPONSE_CHARS = Symbol.for('octocode.rawResponseChars');

export function countSerializedChars(value: unknown): number {
  if (typeof value === 'string') return value.length;

  try {
    return JSON.stringify(value)?.length ?? 0;
  } catch {
    return String(value).length;
  }
}

function normalizeCharCount(value: unknown): number | undefined {
  if (typeof value !== 'number' || !Number.isFinite(value)) return undefined;
  return Math.max(0, value);
}

export function attachRawResponseChars<T extends object>(
  result: T,
  rawResponse: unknown
): T {
  const rawChars =
    typeof rawResponse === 'number'
      ? normalizeCharCount(rawResponse)
      : countSerializedChars(rawResponse);

  if (rawChars === undefined) return result;

  try {
    Object.defineProperty(result, RAW_RESPONSE_CHARS, {
      value: rawChars,
      enumerable: false,
      configurable: true,
    });
  } catch {
    // Raw-size stats are best-effort and must never affect tool output.
  }

  return result;
}

export function getRawResponseChars(value: unknown): number | undefined {
  if (typeof value !== 'object' || value === null) return undefined;
  return normalizeCharCount(
    (value as Record<typeof RAW_RESPONSE_CHARS, unknown>)[RAW_RESPONSE_CHARS]
  );
}
