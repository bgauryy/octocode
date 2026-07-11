export const DEFAULT_IMPORTANCE: Record<string, number> = {
  BUG: 8,
  GOTCHA: 7,
  IMPROVEMENT: 7,
  SECURITY: 9,
  INCIDENT: 9,
  RELEASE: 8,
  DECISION: 6,
  ARCHITECTURE: 6,
};

export function defaultImportance(label: string | undefined): number {
  return DEFAULT_IMPORTANCE[label?.toUpperCase() ?? ''] ?? 5;
}

export function normalizeSupersedes(value: unknown): string[] {
  if (Array.isArray(value)) return value as string[];
  return value ? [value as string] : [];
}

export function requireText(
  params: Record<string, unknown>,
  key: string,
  type: string,
): string {
  const value = params[key];
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error(`memory ${type} requires ${key}`);
  }
  return value;
}

export function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string' && v.trim().length > 0) : [];
}

export function scopeReferences(request: Record<string, unknown>): string[] {
  const references = stringArray(request['references']);
  const file = typeof request['file'] === 'string' ? [`file:${request['file']}`] : [];
  const files = stringArray(request['files']).map((p) => `file:${p}`);
  const folders = stringArray(request['folders']).map((p) => `dir:${p}`);
  return [...references, ...file, ...files, ...folders];
}

export function optionalQuery(request: Record<string, unknown>): string {
  const query = request['query'];
  if (query == null) return '';
  if (typeof query !== 'string') throw new Error('memory recall query must be a string');
  return query;
}
