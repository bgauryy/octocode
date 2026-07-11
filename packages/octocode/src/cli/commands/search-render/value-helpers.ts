/**
 * Shared primitive extraction/formatting helpers used by record detail
 * renderers (see record-detail.ts).
 */

export function recordValue(
  value: unknown
): Record<string, unknown> | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

export function recordArray(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter(recordValue) : [];
}

export function stringArray(value: unknown): string[] {
  return Array.isArray(value)
    ? value
        .map(item => (item === undefined || item === null ? '' : String(item)))
        .filter(Boolean)
    : [];
}

export function stringField(
  record: Record<string, unknown> | undefined,
  key: string
): string | undefined {
  const value = record?.[key];
  return value === undefined || value === null ? undefined : String(value);
}

export function countPart(
  label: string,
  value: string | undefined
): string | undefined {
  return value === undefined ? undefined : `${label}=${value}`;
}

export function previewList(items: string[], max: number): string | undefined {
  const cleaned = items.map(item => item.trim()).filter(Boolean);
  if (cleaned.length === 0) return undefined;
  const suffix = cleaned.length > max ? `, +${cleaned.length - max} more` : '';
  return `${cleaned.slice(0, max).join(', ')}${suffix}`;
}

export function renderSymbolAnchor(
  symbol: Record<string, unknown> | undefined
): string | undefined {
  const name = stringField(symbol, 'name');
  if (!name) return undefined;
  const line =
    stringField(symbol, 'line') ??
    stringField(symbol, 'foundAtLine') ??
    stringField(symbol, 'selectionLine');
  return line ? `${name}:${line}` : name;
}

export function renderSymbolSummary(symbol: Record<string, unknown>): string {
  const anchor = renderSymbolAnchor(symbol);
  const kind = stringField(symbol, 'kind');
  return [anchor, kind].filter(Boolean).join(' ');
}

export function renderLocationSummary(
  location: Record<string, unknown>
): string {
  const range = recordValue(location.displayRange);
  const line = stringField(range, 'startLine');
  const uri = stringField(location, 'uri');
  const content = stringField(location, 'content');
  return [
    uri && line ? `${uri}:${line}` : uri,
    content ? content.trim().slice(0, 80) : undefined,
  ]
    .filter(Boolean)
    .join(' ');
}

export function renderCallSummary(call: Record<string, unknown>): string {
  const item = recordValue(call.item);
  const anchor = renderSymbolAnchor(item);
  const ranges = recordArray(call.ranges);
  return [anchor, ranges.length ? `ranges=${ranges.length}` : undefined]
    .filter(Boolean)
    .join(' ');
}

export function renderDiagnosticSummary(
  diagnostic: Record<string, unknown>
): string {
  return [
    stringField(diagnostic, 'severity'),
    stringField(diagnostic, 'message')?.slice(0, 80),
  ]
    .filter(Boolean)
    .join(': ');
}

export function renderKindCounts(
  kinds: Record<string, unknown> | undefined
): string | undefined {
  if (!kinds) return undefined;
  const parts = Object.entries(kinds).map(
    ([kind, count]) => `${kind}=${count}`
  );
  return parts.length > 0 ? parts.join(' ') : undefined;
}
