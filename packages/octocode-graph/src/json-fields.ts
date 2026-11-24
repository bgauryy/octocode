// Checked reads of parsed JSON fields for provider artifacts: a wrong shape is an error naming the field, never a
// silent default. Shared by the importer (enrich.ts) and every provider adapter.
export function record(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value))
    throw new Error('Expected a JSON object');
  return value as Record<string, unknown>;
}
export function list(value: unknown, field: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`Expected an array for ${field}`);
  return value;
}
export function string(value: unknown, field: string): string {
  if (typeof value !== 'string' || !value.length)
    throw new Error(`Expected a nonempty string for ${field}`);
  return value;
}
