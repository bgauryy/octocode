import path from 'node:path';
import { collectVerifiedAliasReferences } from './referenceAliases.js';
import { runTypedLexicalSearch } from '../../local_search/typedLexicalService.js';
import { nativeSearchPartialReasons } from '../../local_ripgrep/searchCompleteness.js';
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
  Partial<Record<SymbolAnchoredSemanticQuery['operation'], string>>
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
  if (!anchor.resolvedSymbol.name) {
    return {
      ...stats,
      possiblyTruncated: true,
      incompleteReasons: ['anchorName'],
    };
  }
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
      regex: 'literal' as const,
      wholeWord: true,
      resultView: 'files' as const,
      maxFiles: WARM_MAX_FILES,
      pageSize: WARM_MAX_FILES,
      sort: 'path',
      include: family.filter(Boolean).map(e => `*.${e}`),
    };
    let page = 1;
    // Search pagination is independent of maxFiles. Follow explicit pages,
    // retaining a fixed file/request bound even if a backend returns small pages.
    for (let requests = 0; requests < WARM_MAX_FILES; requests += 1) {
      const result = await runTypedLexicalSearch({
        ...baseQuery,
        page,
      });
      if (result.status === 'error') {
        incomplete.add('search');
        break;
      }
      const pagination = result.pagination as
        | { totalFiles?: number; hasMore?: boolean; nextPage?: number }
        | undefined;
      const searchStats = result.stats as
        | { filesMatched?: number; capped?: boolean; errorCount?: number }
        | undefined;
      const files = result.files ?? [];
      const reportedTotal = pagination?.totalFiles ?? searchStats?.filesMatched;
      if (typeof reportedTotal === 'number')
        stats.candidates = Math.max(stats.candidates, reportedTotal);
      if (
        nativeSearchPartialReasons(searchStats).length > 0 ||
        (result as { terminalLimit?: boolean }).terminalLimit === true
      )
        incomplete.add('search');
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
          await client.openDocumentFromDisk(abs, WARM_MAX_BYTES);
          stats.warmedFiles += 1;
        } catch (error) {
          if (
            error instanceof Error &&
            error.message.includes('too large for LSP document open')
          ) {
            stats.skippedLarge += 1;
          }
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
  switch (query.operation) {
    case 'definition':
      if (!client.hasCapability('definitionProvider')) {
        return emptyEnvelope(
          query.operation,
          anchor,
          'definitionProvider unsupported',
          'unsupportedOperation',
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
          query.operation,
          anchor,
          'typeDefinitionProvider unsupported',
          'unsupportedOperation',
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
          query.operation,
          anchor,
          'implementationProvider unsupported',
          'unsupportedOperation',
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
    case 'references': {
      if (!client.hasCapability('referencesProvider')) {
        return emptyEnvelope(
          query.operation,
          anchor,
          'referencesProvider unsupported',
          'unsupportedOperation',
          true
        );
      }
      const references = await client.findReferences(
        anchor.absolutePath,
        anchor.resolvedSymbol.position,
        query.includeDeclaration ?? true,
        anchor.content
      );
      const aliases = await collectVerifiedAliasReferences(
        query,
        anchor,
        client,
        references
      );
      const envelope = referencesEnvelope(
        query,
        anchor,
        aliases.locations,
        warmupStats
      );
      if (envelope.payload.kind !== 'references') return envelope;
      return {
        ...envelope,
        payload: {
          ...envelope.payload,
          coverage: {
            scope: 'languageServer',
            exhaustive: false,
            ...(aliases.verified > 0 && {
              verifiedAliasBindings: aliases.verified,
            }),
            ...(aliases.unverified > 0 && {
              unverifiedAliasBindings: aliases.unverified,
            }),
            ...(aliases.uninspectedFiles > 0 && {
              uninspectedFiles: aliases.uninspectedFiles,
            }),
          },
        },
        ...((aliases.unverified > 0 || aliases.uninspectedFiles > 0) && {
          incompleteResults: true,
          partialReasons: [
            ...(envelope.partialReasons ?? []),
            'aliasReferences' as const,
          ],
        }),
        ...((aliases.unverified > 0 || aliases.uninspectedFiles > 0) && {
          terminalLimit: true,
        }),
      };
    }
    case 'hover':
      if (!client.hasCapability('hoverProvider')) {
        return emptyEnvelope(
          query.operation,
          anchor,
          'hoverProvider unsupported',
          'unsupportedOperation',
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
          query.operation,
          anchor,
          'callHierarchyProvider unsupported',
          'unsupportedOperation',
          true
        );
      }
      return callsEnvelope(query, anchor, client, warmupStats);
    case 'supertypes':
    case 'subtypes':
      if (!client.hasCapability('typeHierarchyProvider')) {
        return emptyEnvelope(
          query.operation,
          anchor,
          'typeHierarchyProvider unsupported',
          'unsupportedOperation',
          true
        );
      }
      return typeHierarchyEnvelope(query, anchor, client);
  }
}
