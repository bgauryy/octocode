// Value handling shared by the storage-dump recipe: values are redacted unless the caller asked
// for them, and only the saved file ever receives a value.

/** What a redacted value leaves behind: its type and size, never its content. */
export type RedactedValue = { redacted: true; type: string; bytes: number };

const byteLength = (value: unknown) =>
  Buffer.byteLength(
    typeof value === 'string' ? value : (JSON.stringify(value) ?? '')
  );

/** The value as written to the saved file: itself when requested, else {redacted,type,bytes}. */
export function shownValue(value: unknown, includeValues: boolean): unknown {
  if (includeValues) return value;
  return {
    redacted: true,
    type:
      value === null ? 'null' : Array.isArray(value) ? 'array' : typeof value,
    bytes: byteLength(value),
  };
}

/** A CDP key path ({type,string,array}) as a plain string, string list or null. */
export function keyPathOf(keyPath?: {
  type?: string;
  string?: string;
  array?: string[];
}): string | string[] | null {
  if (keyPath?.type === 'string') return keyPath.string ?? '';
  if (keyPath?.type === 'array') return keyPath.array ?? [];
  return null;
}
