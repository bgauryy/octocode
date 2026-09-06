import path from 'node:path';
import { readFile } from 'node:fs/promises';
import { searchContentRipgrep } from '../../local_ripgrep/searchContentRipgrep.js';
import { acquirePooledClient } from '@octocodeai/octocode-engine/lsp/manager';
import type { SymbolAnchor } from '../shared/resolveSymbolAnchor.js';
import type {
  ConsumerWarmupStats,
  LspSemanticEnvelope,
  SymbolAnchoredSemanticQuery,
} from '../shared/semanticTypes.js';
import {
  callsEnvelope,
  typeHierarchyEnvelope,
} from './semanticEnvelopes/callEnvelopes.js';
import { emptyEnvelope } from './semanticEnvelopes/envelopeHelpers.js';
import {
  hoverEnvelope,
  locationsEnvelope,
  referencesEnvelope,
} from './semanticEnvelopes/locationEnvelopes.js';

// Relation queries (references/calls) are bounded by the server's open-file
// set. Before running one, open a bounded set of files that mention the
// symbol by name so cross-file relations are visible — otherwise a fresh
// server reports only same-file results and a zero reads as "unused".
export const CONSUMER_SCOPED_PROVIDERS: Readonly<
  Partial<Record<SymbolAnchoredSemanticQuery['type'], string>>
> = {
  references: 'referencesProvider',
  callers: 'callHierarchyProvider',
  callees: 'callHierarchyProvider',
  callHierarchy: 'callHierarchyProvider',
  implementation: 'implementationProvider',
};
const WARM_MAX_FILES = 100;
const WARM_MAX_BYTES = 512 * 1024;
const JS_TS_FAMILY = ['ts', 'tsx', 'mts', 'cts', 'js', 'jsx', 'mjs', 'cjs'];

export async function warmLikelyConsumers(
  client: NonNullable<Awaited<ReturnType<typeof acquirePooledClient>>>,
  anchor: SymbolAnchor,
  workspaceRoot: string
): Promise<ConsumerWarmupStats> {
  const stats: ConsumerWarmupStats = {
    candidates: 0,
    warmedFiles: 0,
    skippedLarge: 0,
    possiblyTruncated: false,
  };
  const incomplete = new Set<
    NonNullable<ConsumerWarmupStats['incompleteReasons']>[number]
  >();
  const seen = new Set<string>();
  try {
    const ext = path.extname(anchor.absolutePath).slice(1);
    const family = JS_TS_FAMILY.includes(ext) ? JS_TS_FAMILY : [ext];
    const baseQuery = {
      path: workspaceRoot,
      searchText: anchor.resolvedSymbol.name,
      regex: 'fixed',
      wholeWord: true,
      output: 'files',
      maxFiles: WARM_MAX_FILES,
      itemsPerPage: WARM_MAX_FILES,
      sort: 'path',
      include: family.filter(Boolean).map(e => `*.${e}`),
    };
    let page = 1;
    // Search pagination is independent of maxFiles. Follow explicit pages,
    // retaining a fixed file/request bound even if a backend returns small pages.
    for (let requests = 0; requests < WARM_MAX_FILES; requests += 1) {
      const result = await searchContentRipgrep({
        ...baseQuery,
        page,
      } as Parameters<typeof searchContentRipgrep>[0]);
      if (result.status === 'error') {
        incomplete.add('search');
        break;
      }
      const pagination = result.pagination as
        | { totalFiles?: number; hasMore?: boolean; nextPage?: number }
        | undefined;
      const searchStats = result.stats as
        { filesMatched?: number; capped?: boolean } | undefined;
      const files = result.files ?? [];
      const reportedTotal = pagination?.totalFiles ?? searchStats?.filesMatched;
      if (typeof reportedTotal === 'number')
        stats.candidates = Math.max(stats.candidates, reportedTotal);
      if (searchStats?.capped) incomplete.add('search');
      for (const file of files) {
        const filePath = typeof file.path === 'string' ? file.path : undefined;
        if (!filePath) {
          incomplete.add('fileRead');
          continue;
        }
        const abs = path.resolve(workspaceRoot, filePath);
        if (seen.has(abs)) continue;
        if (seen.size >= WARM_MAX_FILES) {
          incomplete.add('fileCap');
          break;
        }
        seen.add(abs);
        stats.candidates = Math.max(stats.candidates, seen.size);
        if (abs === path.resolve(anchor.absolutePath)) continue;
        try {
          const content = await readFile(abs, 'utf-8');
          if (Buffer.byteLength(content, 'utf8') > WARM_MAX_BYTES) {
            stats.skippedLarge += 1;
            incomplete.add('fileRead');
            continue;
          }
          await client.openDocument(abs, content);
          stats.warmedFiles += 1;
        } catch {
          incomplete.add('fileRead');
        }
      }
      if (!pagination?.hasMore) {
        if (!pagination && files.length >= WARM_MAX_FILES)
          incomplete.add('fileCap');
        break;
      }
      if (seen.size >= WARM_MAX_FILES) {
        incomplete.add('fileCap');
        break;
      }
      const nextPage = pagination.nextPage;
      if (
        typeof nextPage !== 'number' ||
        nextPage <= page ||
        requests + 1 >= WARM_MAX_FILES
      ) {
        incomplete.add('search');
        break;
      }
      page = nextPage;
    }
  } catch {
    incomplete.add('search');
  }
  stats.possiblyTruncated = incomplete.size > 0;
  if (incomplete.size) stats.incompleteReasons = [...incomplete];
  return stats;
}

export async function dispatchAnchoredSemantic(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  client: NonNullable<Awaited<ReturnType<typeof acquirePooledClient>>>,
  warmupStats?: ConsumerWarmupStats
): Promise<LspSemanticEnvelope> {
  switch (query.type) {
    case 'definition':
      if (!client.hasCapability('definitionProvider')) {
        return emptyEnvelope(
          query.type,
          anchor,
          'definitionProvider unsupported',
          true
        );
      }
      return locationsEnvelope(
        query,
        anchor,
        'definition',
        'definitionProvider',
        await client.gotoDefinition(
          anchor.absolutePath,
          anchor.resolvedSymbol.position,
          anchor.content
        )
      );
    case 'typeDefinition':
      if (!client.hasCapability('typeDefinitionProvider')) {
        return emptyEnvelope(
          query.type,
          anchor,
          'typeDefinitionProvider unsupported',
          true
        );
      }
      return locationsEnvelope(
        query,
        anchor,
        'typeDefinition',
        'typeDefinitionProvider',
        await client.typeDefinition(
          anchor.absolutePath,
          anchor.resolvedSymbol.position,
          anchor.content
        )
      );
    case 'implementation':
      if (!client.hasCapability('implementationProvider')) {
        return emptyEnvelope(
          query.type,
          anchor,
          'implementationProvider unsupported',
          true
        );
      }
      return locationsEnvelope(
        query,
        anchor,
        'implementation',
        'implementationProvider',
        await client.implementation(
          anchor.absolutePath,
          anchor.resolvedSymbol.position,
          anchor.content
        ),
        warmupStats
      );
    case 'references':
      if (!client.hasCapability('referencesProvider')) {
        return emptyEnvelope(
          query.type,
          anchor,
          'referencesProvider unsupported',
          true
        );
      }
      return referencesEnvelope(
        query,
        anchor,
        await client.findReferences(
          anchor.absolutePath,
          anchor.resolvedSymbol.position,
          query.includeDeclaration ?? true,
          anchor.content
        ),
        warmupStats
      );
    case 'hover':
      if (!client.hasCapability('hoverProvider')) {
        return emptyEnvelope(
          query.type,
          anchor,
          'hoverProvider unsupported',
          true
        );
      }
      return hoverEnvelope(
        query,
        anchor,
        await client.hover(
          anchor.absolutePath,
          anchor.resolvedSymbol.position,
          anchor.content
        )
      );
    case 'callers':
    case 'callees':
    case 'callHierarchy':
      if (!client.hasCapability('callHierarchyProvider')) {
        return emptyEnvelope(
          query.type,
          anchor,
          'callHierarchyProvider unsupported',
          true
        );
      }
      return callsEnvelope(query, anchor, client, warmupStats);
    case 'supertypes':
    case 'subtypes':
      if (!client.hasCapability('typeHierarchyProvider')) {
        return emptyEnvelope(
          query.type,
          anchor,
          'typeHierarchyProvider unsupported',
          true
        );
      }
      return typeHierarchyEnvelope(query, anchor, client);
  }
}
