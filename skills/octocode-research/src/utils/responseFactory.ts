import { isObject, hasProperty, isArray, hasStringProperty, hasNumberProperty } from '../types/guards.js';

export function safeString(obj: unknown, key: string, fallback = ''): string {
  if (hasStringProperty(obj, key)) {
    return obj[key];
  }
  return fallback;
}

export function safeNumber(obj: unknown, key: string, fallback = 0): number {
  if (hasNumberProperty(obj, key)) {
    return obj[key];
  }
  return fallback;
}

export function safeArray<T>(obj: unknown, key: string): T[] {
  if (isObject(obj) && hasProperty(obj, key) && isArray(obj[key])) {
    return obj[key] as T[];
  }
  return [];
}

export function extractMatchLocations(matches: unknown[]): Array<{
  line: number;
  column?: number;
  value?: string;
  byteOffset?: number;
  charOffset?: number;
}> {
  return matches.map((m) => {
    if (!isObject(m)) return { line: 0 };
    return {
      line: safeNumber(m, 'line', 0),
      column: hasNumberProperty(m, 'column') ? m.column : undefined,
      value: hasStringProperty(m, 'value') ? m.value.trim() : undefined,
      byteOffset: hasNumberProperty(m, 'byteOffset') ? m.byteOffset : undefined,
      charOffset: hasNumberProperty(m, 'charOffset') ? m.charOffset : undefined,
    };
  });
}

export function transformPagination(pagination: unknown): { page: number; total: number; hasMore: boolean } | undefined {
  if (!isObject(pagination)) return undefined;
  
  const currentPage = safeNumber(pagination, 'currentPage', 1);
  const totalPages = safeNumber(pagination, 'totalPages', 1);
  const hasMore = hasProperty(pagination, 'hasMore') && pagination.hasMore === true;
  
  return { page: currentPage, total: totalPages, hasMore };
}
