import path from 'node:path';
import {
  contextUtils,
  type GraphFactCapability,
} from '../../../utils/contextUtils.js';
import { FALLBACK_SOURCE_EXTENSIONS } from './types.js';

let cachedSourceExtensions: ReadonlySet<string> | undefined;
let cachedGraphCapabilities: readonly GraphFactCapability[] | undefined;

export function sourceExtensions(): ReadonlySet<string> {
  if (cachedSourceExtensions) return cachedSourceExtensions;
  try {
    const extensions = contextUtils
      .getSupportedGraphFactExtensions()
      .map(ext => (ext.startsWith('.') ? ext : `.${ext}`).toLowerCase());
    cachedSourceExtensions = new Set(
      extensions.length > 0 ? extensions : FALLBACK_SOURCE_EXTENSIONS
    );
  } catch {
    cachedSourceExtensions = new Set(FALLBACK_SOURCE_EXTENSIONS);
  }
  return cachedSourceExtensions;
}

export function graphFactCapabilities(): readonly GraphFactCapability[] {
  if (cachedGraphCapabilities) return cachedGraphCapabilities;
  try {
    const parsed = JSON.parse(
      contextUtils.getGraphFactCapabilities()
    ) as unknown;
    cachedGraphCapabilities = Array.isArray(parsed)
      ? parsed.filter(isGraphFactCapability)
      : [];
  } catch {
    cachedGraphCapabilities = [];
  }
  return cachedGraphCapabilities;
}

function isGraphFactCapability(value: unknown): value is GraphFactCapability {
  if (!value || typeof value !== 'object') return false;
  const record = value as Partial<GraphFactCapability>;
  return (
    typeof record.extension === 'string' &&
    typeof record.language === 'string' &&
    Array.isArray(record.factFamilies)
  );
}

export function countBy<T>(
  items: readonly T[],
  key: (item: T) => string
): Readonly<Record<string, number>> {
  const counts: Record<string, number> = {};
  for (const item of items) {
    const value = key(item) || 'unknown';
    counts[value] = (counts[value] ?? 0) + 1;
  }
  return counts;
}

export function resolveImport(
  dir: string,
  specifier: string,
  known: ReadonlySet<string>
): string | undefined {
  return resolveExistingPath(path.resolve(dir, specifier), known);
}

export function resolveExistingPath(
  base: string,
  known: ReadonlySet<string>
): string | undefined {
  const extensions = sourceExtensions();
  const candidates = [base];
  for (const ext of extensions) candidates.push(`${base}${ext}`);
  for (const ext of extensions) candidates.push(path.join(base, `index${ext}`));
  return candidates.find(candidate => known.has(candidate));
}

export function isRelativeSpecifier(specifier: string): boolean {
  return specifier.startsWith('./') || specifier.startsWith('../');
}

export function packageNameFromSpecifier(
  specifier: string
): string | undefined {
  if (specifier.startsWith('@')) {
    const [scope, name] = specifier.split('/');
    return scope && name ? `${scope}/${name}` : undefined;
  }
  return specifier.split('/')[0];
}

export function recordValue(
  value: unknown
): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

export function stringValue(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value : undefined;
}

export function isNonEmptyString(value: string | undefined): value is string {
  return typeof value === 'string' && value.length > 0;
}

export function isString(value: unknown): value is string {
  return typeof value === 'string';
}

export function uniqueStrings(values: readonly string[]): readonly string[] {
  return [...new Set(values)];
}

export function relative(root: string, file: string): string {
  const rel = path.relative(root, file);
  return rel || path.basename(file);
}

export function exportKind(line: string): string {
  const match = /\b(function|class|const|let|var|type|interface|enum)\b/.exec(
    line
  );
  return match?.[1] ?? 'export';
}

export function calleeRefersToSymbol(callee: string, symbol: string): boolean {
  if (callee === symbol) return true;
  if (callee.endsWith(`.${symbol}`)) return true;
  if (callee.endsWith(`::${symbol}`)) return true;
  return false;
}
