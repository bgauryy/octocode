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
  locations: CodeSnippet[]
): LspSemanticEnvelope {
  const complete = locations.length > 0;
  const compactLocations = locations.map(compactLocation);
  const { pageItems, pagination } = paginateItems(
    compactLocations,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_LOCATIONS_PER_PAGE
  );
  return {
    type: query.type,
    uri: anchor.uri,
    resolvedSymbol: compactResolvedSymbol(anchor.resolvedSymbol),
    lsp: { serverAvailable: true, provider },
    payload: complete
      ? { kind, locations: pageItems }
      : {
          kind: 'empty',
          category: 'noLocations',
          reason: `${provider} returned no locations`,
        },
    ...(complete ? { pagination } : {}),
  };
}

export function referencesEnvelope(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  locations: CodeSnippet[]
): LspSemanticEnvelope {
  const refs = locations.map((location): ReferenceLocation => {
    const isDefinition =
      location.uri === anchor.uri &&
      location.range.start.line === anchor.resolvedSymbol.position.line &&
      location.range.start.character ===
        anchor.resolvedSymbol.position.character;
    return { ...location, ...(isDefinition ? { isDefinition: true } : {}) };
  });
  const byFile = query.groupByFile ? buildReferencesByFile(refs) : undefined;
  const referenceItems = byFile ?? refs.map(compactLocation);
  const { pageItems, pagination } = paginateItems(
    referenceItems,
    query.page ?? 1,
    query.itemsPerPage ?? DEFAULT_LOCATIONS_PER_PAGE
  );
  const empty =
    refs.length === 0
      ? {
          category: 'noReferences' as const,
          reason: 'referencesProvider returned no references',
        }
      : undefined;

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
      ...(empty ? { empty } : {}),
    },
    pagination,
  };
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
