import type {
  CallHierarchyItem,
  IncomingCall,
  LSPRange,
  OutgoingCall,
} from '@octocodeai/octocode-engine/lsp/types';
import {
  attachRawResponseChars,
  countSerializedChars,
} from '../../../utils/response/charSavings.js';
import {
  type CompactLocation,
  type LspGetSemanticsQuery,
  type LspSemanticEnvelope,
} from '../shared/semanticTypes.js';

const MAX_RANGE_SAMPLES = 8;

export type CompactCallTarget = {
  name: string;
  kind: string;
  uri: string;
  line: number;
  endLine: number;
  selectionLine?: number;
};

export type CompactCall = {
  direction: 'incoming' | 'outgoing';
  item: CompactCallTarget;
  ranges: Array<{ line: number; character: number }>;
  rangeCount: number;
  rangeSampleCount: number;
  contentPreview?: string;
};

export function attachSemanticRawEvidence<T extends object>(result: T): T {
  return attachRawResponseChars(result, countSerializedChars(result));
}

export function formatSemanticResult(
  query: LspGetSemanticsQuery,
  result: LspSemanticEnvelope | Record<string, unknown>
): LspSemanticEnvelope | Record<string, unknown> {
  if (query.format !== 'compact' || !isSemanticEnvelope(result)) return result;
  return compactSemanticEnvelope(result);
}

// Ready-to-run follow-up: read the top result location with context, so the
// agent doesn't have to assemble the localGetFileContent call from ranges.
export function withSemanticNext(
  result: LspSemanticEnvelope | Record<string, unknown>
): LspSemanticEnvelope | Record<string, unknown> {
  if (!isSemanticEnvelope(result)) return result;
  const payload = result.payload as {
    locations?: Array<{
      uri?: string;
      displayRange?: { startLine?: number; endLine?: number };
    }>;
  };
  const loc = payload.locations?.[0];
  const start = loc?.displayRange?.startLine;
  if (!loc?.uri || typeof start !== 'number') return result;
  const path = loc.uri.startsWith('file://')
    ? decodeURIComponent(loc.uri.slice('file://'.length))
    : loc.uri;
  return {
    ...result,
    next: {
      readSite: {
        tool: 'localGetFileContent',
        query: {
          path,
          startLine: Math.max(1, start - 3),
          endLine: (loc.displayRange?.endLine ?? start) + 10,
        },
        why: 'Read the top result location with surrounding context',
        confidence: 'exact',
      },
    },
  };
}

export function isSemanticEnvelope(
  value: LspSemanticEnvelope | Record<string, unknown>
): value is LspSemanticEnvelope {
  return (
    isRecord(value) &&
    typeof value.type === 'string' &&
    typeof value.uri === 'string' &&
    isRecord(value.payload)
  );
}

export function compactSemanticEnvelope(
  envelope: LspSemanticEnvelope
): LspSemanticEnvelope {
  return {
    ...envelope,
    format: 'compact',
    payload: compactSemanticPayload(envelope.payload),
  };
}

export function compactSemanticPayload(
  payload: LspSemanticEnvelope['payload']
): LspSemanticEnvelope['payload'] {
  switch (payload.kind) {
    case 'definition':
    case 'typeDefinition':
    case 'implementation':
      return {
        kind: payload.kind,
        locations: payload.locations.map(formatLocationRow),
      };
    case 'references':
      return {
        kind: 'references',
        ...(payload.byFile
          ? { byFile: payload.byFile.map(formatReferenceFileRow) }
          : { locations: (payload.locations ?? []).map(formatLocationRow) }),
        totalReferences: payload.totalReferences,
        totalFiles: payload.totalFiles,
      };
    case 'callers':
    case 'callees':
    case 'callHierarchy':
      return {
        kind: payload.kind,
        ...(payload.root ? { root: formatCallTargetRow(payload.root) } : {}),
        direction: payload.direction,
        calls: payload.calls.map(formatCallRow),
        ...(payload.incomingCalls !== undefined
          ? { incomingCalls: payload.incomingCalls }
          : {}),
        ...(payload.outgoingCalls !== undefined
          ? { outgoingCalls: payload.outgoingCalls }
          : {}),
        completeness: payload.completeness,
      };
    case 'documentSymbols':
      return {
        kind: 'documentSymbols',
        symbols: payload.symbols.map(formatSymbolRow),
      };
    case 'hover':
    case 'empty':
    case 'workspaceSymbol':
    case 'typeHierarchy':
    case 'diagnostic':
      return payload;
  }
}

export function formatSymbolRow(value: unknown): string {
  if (!isRecord(value)) return String(value);
  const line = numberField(value, 'line');
  const character = numberField(value, 'character');
  const endLine = numberField(value, 'endLine');
  const kind = stringField(value, 'kind');
  const name = stringField(value, 'name');
  const childCount = numberField(value, 'childCount');
  const containerName = stringField(value, 'containerName');
  return [
    `${line}:${character}${endLine !== line ? `-${endLine}` : ''}`,
    kind,
    name,
    containerName ? `< ${containerName}` : '',
    childCount > 0 ? `children=${childCount}` : '',
  ]
    .filter(Boolean)
    .join(' ');
}

export function formatLocationRow(location: CompactLocation | string): string {
  if (typeof location === 'string') return location;
  const range = location.displayRange
    ? `${location.displayRange.startLine}-${location.displayRange.endLine}`
    : '?';
  const definition = location.isDefinition ? ' def' : '';
  const content = location.content
    ? ` | ${oneLine(location.content, 180)}`
    : '';
  return `${location.uri}:${range}${definition}${content}`;
}

export function formatReferenceFileRow(value: unknown): string {
  if (!isRecord(value)) return String(value);
  const uri = stringField(value, 'uri');
  const firstLine = numberField(value, 'firstLine');
  const firstCharacter = numberField(value, 'firstCharacter');
  const count = numberField(value, 'count');
  const lines = arrayField(value, 'lines')
    .map(line => (typeof line === 'number' ? line : undefined))
    .filter(line => line !== undefined)
    .join(',');
  const definition = value.hasDefinition === true ? ' def' : '';
  return `${uri}:${firstLine}:${firstCharacter} count=${count} lines=${lines}${definition}`;
}

export function formatCallRow(value: unknown): string {
  if (!isRecord(value)) return String(value);
  const direction = stringField(value, 'direction');
  const item = formatCallTargetRow(value.item);
  const ranges = arrayField(value, 'ranges').map(formatRangeRow).join(',');
  const rangeCount = numberField(value, 'rangeCount');
  const rangeSampleCount = numberField(value, 'rangeSampleCount');
  const preview = stringField(value, 'contentPreview');
  return [
    direction,
    item,
    ranges ? `ranges=${ranges}` : '',
    rangeCount > rangeSampleCount ? `totalRanges=${rangeCount}` : '',
    preview ? `| ${oneLine(preview, 180)}` : '',
  ]
    .filter(Boolean)
    .join(' ');
}

export function formatCallTargetRow(value: unknown): string {
  if (!isRecord(value)) return String(value);
  const name = stringField(value, 'name');
  const kind = stringField(value, 'kind');
  const uri = stringField(value, 'uri');
  const line = numberField(value, 'line');
  const endLine = numberField(value, 'endLine');
  const selectionLine = numberField(value, 'selectionLine');
  const selection = selectionLine > 0 ? ` sel=${selectionLine}` : '';
  return `${name} ${kind} ${uri}:${line}-${endLine}${selection}`;
}

export function formatRangeRow(value: unknown): string {
  if (!isRecord(value)) return String(value);
  return `${numberField(value, 'line')}:${numberField(value, 'character')}`;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function stringField(
  record: Record<string, unknown>,
  key: string,
  fallback = ''
): string {
  const value = record[key];
  return typeof value === 'string' ? value : fallback;
}

export function numberField(
  record: Record<string, unknown>,
  key: string,
  fallback = 0
): number {
  const value = record[key];
  return typeof value === 'number' ? value : fallback;
}

export function arrayField(
  record: Record<string, unknown>,
  key: string
): unknown[] {
  const value = record[key];
  return Array.isArray(value) ? value : [];
}

export function oneLine(value: string, maxLength: number): string {
  const singleLine = value.replace(/\s+/g, ' ').trim();
  return singleLine.length > maxLength
    ? `${singleLine.slice(0, Math.max(0, maxLength - 3))}... (truncated for single-line display — use charOffset or startLine to read full content)`
    : singleLine;
}

export function compactIncomingCall(
  call: IncomingCall & { direction: 'incoming' },
  contextLines: number
): CompactCall {
  const ranges = compactRanges(call.fromRanges);
  return {
    direction: 'incoming',
    item: compactCallItem(call.from),
    ranges,
    rangeCount: call.fromRanges.length,
    rangeSampleCount: ranges.length,
    ...contentPreview(call.from, contextLines),
  };
}

export function compactOutgoingCall(
  call: OutgoingCall & { direction: 'outgoing' },
  contextLines: number
): CompactCall {
  const ranges = compactRanges(call.fromRanges);
  return {
    direction: 'outgoing',
    item: compactCallItem(call.to),
    ranges,
    rangeCount: call.fromRanges.length,
    rangeSampleCount: ranges.length,
    ...contentPreview(call.to, contextLines),
  };
}

export function compactCallItem(item: CallHierarchyItem): CompactCallTarget {
  return {
    name: item.name,
    kind: symbolKindName(item.kind),
    uri: item.uri,
    line: item.range.start.line + 1,
    endLine: item.range.end.line + 1,
    ...(item.selectionRange
      ? { selectionLine: item.selectionRange.start.line + 1 }
      : {}),
  };
}

export function compactRanges(ranges: readonly LSPRange[]) {
  const seen = new Set<string>();
  const compact: Array<{ line: number; character: number }> = [];
  for (const range of ranges) {
    const line = range.start.line + 1;
    const character = range.start.character;
    const key = `${line}:${character}`;
    if (seen.has(key)) continue;
    seen.add(key);
    compact.push({ line, character });
    if (compact.length >= MAX_RANGE_SAMPLES) break;
  }
  return compact;
}

export function contentPreview(
  item: CallHierarchyItem,
  contextLines: number
): { contentPreview?: string } {
  if (contextLines <= 0 || !item.content) return {};
  return { contentPreview: item.content };
}

export function symbolKindName(kind: unknown): string {
  if (typeof kind === 'string') return kind;
  const numericKind = typeof kind === 'number' ? kind : undefined;
  switch (numericKind) {
    case 1:
      return 'file';
    case 2:
      return 'module';
    case 3:
      return 'namespace';
    case 4:
      return 'package';
    case 5:
      return 'class';
    case 6:
      return 'method';
    case 7:
      return 'property';
    case 8:
      return 'field';
    case 9:
      return 'constructor';
    case 10:
      return 'enum';
    case 11:
      return 'interface';
    case 12:
      return 'function';
    case 13:
      return 'variable';
    case 14:
      return 'constant';
    case 15:
      return 'string';
    case 16:
      return 'number';
    case 17:
      return 'boolean';
    case 18:
      return 'array';
    case 19:
      return 'object';
    case 20:
      return 'key';
    case 21:
      return 'null';
    case 22:
      return 'enumMember';
    case 23:
      return 'struct';
    case 24:
      return 'event';
    case 25:
      return 'operator';
    case 26:
      return 'typeParameter';
    default:
      return 'unknown';
  }
}
