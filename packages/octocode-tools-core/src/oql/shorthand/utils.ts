/**
 * Small stateless helpers shared across the shorthand lowering builders.
 */

/** Drop `undefined` values so callers don't emit sparse-but-present keys. */
export function clean(input: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(input)) {
    if (value !== undefined) out[key] = value;
  }
  return out;
}

export function listFromComma(value: string): string[] {
  return value
    .split(',')
    .map(v => v.trim().replace(/^\./, ''))
    .filter(Boolean);
}

export function isTreeSort(
  value: string | undefined
): value is 'name' | 'size' | 'time' | 'extension' {
  return (
    value === 'name' ||
    value === 'size' ||
    value === 'time' ||
    value === 'extension'
  );
}
