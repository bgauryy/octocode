import type {
  LSPRange,
  ReferenceLocation,
  ReferencesByFile,
  CodeSnippet,
} from '@octocodeai/octocode-engine/lsp/types';
import {
  compactLocation,
  compactResolvedSymbol,
  type LspSemanticEnvelope,
  type SymbolAnchoredSemanticQuery,
  type ConsumerWarmupStats,
} from '../../shared/semanticTypes.js';
import type { SymbolAnchor } from '../../shared/resolveSymbolAnchor.js';
import {
  DEFAULT_LOCATIONS_PER_PAGE,
  paginateItems,
} from './envelopeHelpers.js';

export function locationsEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  kind: 'definition' | 'typeDefinition' | 'implementation',
  provider: string,
  locations: CodeSnippet[],
  warmupStats?: ConsumerWarmupStats
): LspSemanticEnvelope {
  const complete = locations.length > 0;
  const warmup =
    kind === 'implementation' && warmupStats ? { warmup: warmupStats } : {};
  const compactLocations = locations.map(compactLocation);
  const { pageItems, pagination } = paginateItems(
    compactLocations,
    query.page ?? 1,
    query.pageSize ?? DEFAULT_LOCATIONS_PER_PAGE,
    query,
    locations
  );
  return {
    type: query.type,
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable: true, provider },
    payload: complete
      ? { kind, locations: pageItems, ...warmup }
      : {
          kind: 'empty',
          category: 'noLocations',
          reason: `${provider} returned no locations`,
          ...warmup,
        },
    ...(complete ? { pagination } : {}),
  };
}

export function referencesEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  locations: CodeSnippet[],
  warmupStats?: ConsumerWarmupStats
): LspSemanticEnvelope {
  // A query anchor may be a usage. LSP references return locations without
  // declaration identity; matching the anchor cannot establish a definition.
  const refs: ReferenceLocation[] = stableReferenceLocations(locations);
  const byFile = query.groupByFile ? buildReferencesByFile(refs) : undefined;
  const referenceGroups = new Map<string, ReferenceLocation[]>();
  if (byFile) {
    for (const ref of refs) {
      const group = referenceGroups.get(ref.uri) ?? [];
      group.push(ref);
      referenceGroups.set(ref.uri, group);
    }
  }
  const referenceItems = byFile ?? refs.map(compactLocation);
  const { pageItems, pagination } = paginateItems(
    referenceItems,
    query.page ?? 1,
    query.pageSize ?? DEFAULT_LOCATIONS_PER_PAGE,
    query,
    byFile ? byFile.map(file => referenceGroups.get(file.uri)) : refs
  );
  const empty =
    refs.length === 0
      ? {
          category: 'noReferences' as const,
          reason: 'referencesProvider returned no references',
        }
      : undefined;

  const warmupWarnings =
    warmupStats?.possiblyTruncated === true
      ? [
          `Reference warmup opened ${warmupStats.warmedFiles}/${warmupStats.candidates} candidate file(s) and was incomplete (${warmupStats.incompleteReasons?.join(', ') ?? 'file cap'}); narrow workspaceRoot/path or confirm with localSearch operation:"text" before unused/safe-delete claims.`,
        ]
      : [];

  return {
    type: 'references',
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: {
      serverAvailable: true,
      provider: 'referencesProvider',
      source: 'lsp',
    },
    payload: {
      kind: 'references',
      ...(byFile ? { byFile: pageItems } : { locations: pageItems }),
      totalReferences: refs.length,
      totalFiles: new Set(refs.map(ref => ref.uri)).size,
      // Def-only is NOT proof of absence: the index scope may be narrow, the
      // warmup may have missed consumers, or the symbol may be public API
      // (re-exported). Typed signal — responses carry no warnings channel.
      ...(refs.length > 0 && refs.every(ref => ref.isDefinition)
        ? { definitionOnly: true }
        : {}),
      ...(warmupStats ? { warmup: warmupStats } : {}),
      ...(empty ? { empty } : {}),
    },
    pagination,
    ...(warmupWarnings.length > 0 ? { warnings: warmupWarnings } : {}),
  };
}

function referenceUri(uri: string): string {
  if (uri.startsWith('file:')) return pathToFileURL(fileURLToPath(uri)).href;
  return path.isAbsolute(uri) ? pathToFileURL(path.normalize(uri)).href : uri;
}

// Providers can return the same identities in a different order on each
// request. Canonical identity and ordering must precede grouping/pagination.
function stableReferenceLocations(
  locations: readonly CodeSnippet[]
): CodeSnippet[] {
  const unique = new Map<string, CodeSnippet>();
  for (const location of locations) {
    const { start, end } = location.range;
    const uri = referenceUri(location.uri);
    const key = JSON.stringify([
      uri,
      start.line,
      start.character,
      end.line,
      end.character,
    ]);
    if (!unique.has(key)) unique.set(key, { ...location, uri });
  }
  return [...unique.values()].sort(
    (left, right) =>
      (left.uri < right.uri ? -1 : left.uri > right.uri ? 1 : 0) ||
      left.range.start.line - right.range.start.line ||
      left.range.start.character - right.range.start.character ||
      left.range.end.line - right.range.end.line ||
      left.range.end.character - right.range.end.character
  );
}

export async function hoverEnvelope(
  _query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  hover: unknown
): Promise<LspSemanticEnvelope> {
  const normalized = normalizeHover(hover);
  const complete = Boolean(normalized.markdown || normalized.text);

  return {
    type: 'hover',
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable: true, provider: 'hoverProvider' },
    payload: complete
      ? { kind: 'hover', ...normalized }
      : {
          kind: 'empty',
          category: 'noHover',
          reason: 'hoverProvider returned no hover content',
        },
  };
}

export function buildReferencesByFile(
  locations: readonly ReferenceLocation[]
): ReferencesByFile[] {
  const byUri = new Map<string, ReferencesByFile>();
  for (const loc of locations) {
    const lineNumber = loc.range.start.line + 1;
    const existing = byUri.get(loc.uri);
    if (existing) {
      existing.count += 1;
      existing.lines.push(lineNumber);
      if (loc.isDefinition) existing.hasDefinition = true;
      continue;
    }
    byUri.set(loc.uri, {
      uri: loc.uri,
      count: 1,
      firstLine: lineNumber,
      firstCharacter: loc.range.start.character,
      lines: [lineNumber],
      ...(loc.isDefinition ? { hasDefinition: true } : {}),
    });
  }
  return [...byUri.values()];
}

export function normalizeHover(hover: unknown): {
  markdown?: string;
  text?: string;
  range?: LSPRange;
} {
  if (!hover || typeof hover !== 'object') return {};
  const value = hover as { contents?: unknown; range?: unknown };
  const content = value.contents;
  if (typeof content === 'string') return { text: content.trim() };
  if (Array.isArray(content)) {
    return {
      markdown: content
        .map(part => stringifyHoverPart(part))
        .join('\n')
        .trim(),
    };
  }
  if (content && typeof content === 'object') {
    const part = content as { kind?: unknown; value?: unknown };
    if (typeof part.value === 'string') {
      return part.kind === 'markdown'
        ? { markdown: part.value.trim() }
        : { text: part.value.trim() };
    }
  }
  return {};
}

export function stringifyHoverPart(part: unknown): string {
  if (typeof part === 'string') return part;
  if (part && typeof part === 'object') {
    const value = (part as { value?: unknown }).value;
    if (typeof value === 'string') return value;
  }
  return String(part);
}
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
