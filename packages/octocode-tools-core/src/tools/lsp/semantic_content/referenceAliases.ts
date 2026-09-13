import { fileURLToPath } from 'node:url';
import type { LSPClient } from '@octocodeai/octocode-engine/lsp/client';
import type {
  CodeSnippet,
  LSPRange,
} from '@octocodeai/octocode-engine/lsp/types';
import { decodeGraphFactsJson } from '../../../graph/scanContract.js';
import { contextUtils } from '../../../utils/contextUtils.js';
import {
  resolveFileAnchor,
  type SymbolAnchor,
} from '../shared/resolveSymbolAnchor.js';
import type {
  LspSemanticEnvelope,
  SymbolAnchoredSemanticQuery,
} from '../shared/semanticTypes.js';
import { DEFAULT_LOCATIONS_PER_PAGE } from './semanticEnvelopes/envelopeHelpers.js';

function localPath(uri: string): string {
  return uri.startsWith('file:') ? fileURLToPath(uri) : uri;
}

function identity(location: { uri: string; range: LSPRange }): string {
  const { start, end } = location.range;
  return JSON.stringify([
    localPath(location.uri),
    start.line,
    start.character,
    end.line,
    end.character,
  ]);
}

/** Syntax finds bindings; matching provider definitions alone proves identity. */
export async function withReferenceAliasContinuations(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  client: LSPClient,
  locations: CodeSnippet[],
  envelope: LspSemanticEnvelope
): Promise<LspSemanticEnvelope> {
  if (envelope.payload.kind !== 'references') return envelope;
  const payload = envelope.payload;
  const visible = payload.locations ?? payload.byFile ?? [];
  const files = new Set(
    visible.flatMap(item =>
      typeof item === 'object' &&
      item !== null &&
      'uri' in item &&
      typeof item.uri === 'string'
        ? [localPath(item.uri)]
        : []
    )
  );
  const next = { ...envelope.next };
  const providerIdentities = new Set(locations.map(identity));
  const pageLocations = payload.byFile
    ? locations.filter(location => files.has(localPath(location.uri)))
    : visible.flatMap(item =>
        typeof item === 'object' &&
        item !== null &&
        'range' in item &&
        item.range
          ? [item as CodeSnippet]
          : []
      );
  const pageIdentities = new Set(pageLocations.map(identity));
  const deferred = Boolean(
    anchor.resolvedSymbol.name &&
    payload.byFile &&
    pageIdentities.size > (query.pageSize ?? DEFAULT_LOCATIONS_PER_PAGE)
  );
  if (deferred) {
    const { snapshot: _snapshot, ...restart } = query;
    next.nextAliasReferences = {
      tool: 'lspSearch',
      query: { ...restart, groupByFile: false, page: 1 },
      why: 'Inspect individual reference pages for alias bindings; grouped file counts do not bound this work.',
      confidence: 'exact',
    };
  }
  let verified = 0;
  let unverified = 0;
  let uninspectedFiles = 0;
  let definitions: Set<string> | undefined;
  const inspectedBindings = new Set<string>();
  for (const file of deferred || !anchor.resolvedSymbol.name ? [] : files) {
    const source = await resolveFileAnchor({ uri: file }, 'lspSearch');
    if (!source.ok) {
      uninspectedFiles += 1;
      continue;
    }
    let facts: string | null;
    try {
      facts = contextUtils.extractGraphFacts(source.value.content, file);
    } catch {
      uninspectedFiles += 1;
      continue;
    }
    if (!facts) {
      uninspectedFiles += 1;
      continue;
    }
    const decoded = decodeGraphFactsJson<{
      imports?: Array<{
        localName?: string;
        importedName?: string;
        importedRange?: LSPRange;
        localRange?: LSPRange;
      }>;
    }>(facts);
    if (!decoded.ok) {
      uninspectedFiles += 1;
      continue;
    }
    const aliases = (decoded.parsed.imports ?? []).filter(
      binding =>
        binding.importedName === anchor.resolvedSymbol.name &&
        binding.localName &&
        binding.localName !== binding.importedName
    );
    for (const binding of aliases) {
      if (!binding.importedRange || !binding.localRange) {
        unverified += 1;
        continue;
      }
      // The original import token owns the work on exactly one reference page.
      // Syntax supplies coordinates, never a substitute semantic identity.
      if (
        !pageIdentities.has(
          identity({ uri: file, range: binding.importedRange })
        )
      )
        continue;
      const bindingIdentity = identity({
        uri: file,
        range: binding.localRange,
      });
      if (inspectedBindings.has(bindingIdentity)) continue;
      inspectedBindings.add(bindingIdentity);
      // No extra query for bindings already in this provider's reference set.
      // Coverage remains provider-scoped, never an exhaustive-use claim.
      if (providerIdentities.has(bindingIdentity)) continue;
      try {
        if (!client.hasCapability('definitionProvider')) {
          unverified += 1;
          continue;
        }
        definitions ??= new Set(
          (
            await client.gotoDefinition(
              anchor.absolutePath,
              anchor.resolvedSymbol.position,
              anchor.content
            )
          ).map(identity)
        );
        if (definitions.size === 0) {
          unverified += 1;
          continue;
        }
        const targets = await client.gotoDefinition(
          file,
          binding.localRange.start,
          source.value.content
        );
        if (targets.length === 0) {
          unverified += 1;
          continue;
        }
        if (!targets.some(target => definitions!.has(identity(target))))
          continue;
        const targetIdentities = new Set(targets.map(identity));
        if (
          targetIdentities.size !== definitions.size ||
          !targets.every(target => definitions!.has(identity(target)))
        ) {
          unverified += 1;
          continue;
        }
        next[`searchAliasReferences${verified}`] = {
          tool: 'lspSearch',
          query: {
            operation: 'references',
            uri: file,
            position: binding.localRange.start,
            ...(query.workspaceRoot && { workspaceRoot: query.workspaceRoot }),
            ...(query.rustContext && { rustContext: query.rustContext }),
            includeDeclaration: query.includeDeclaration ?? true,
            ...(query.pageSize && { pageSize: query.pageSize }),
            ...(query.format && { format: query.format }),
            ...(query.contextLines !== undefined && {
              contextLines: query.contextLines,
            }),
            ...(query.groupByFile !== undefined && {
              groupByFile: query.groupByFile,
            }),
          },
          why: 'This import alias resolves to the same definition but is absent from the provider reference set. Query its references separately; retain both sets.',
          confidence: 'exact',
        };
        verified += 1;
      } catch {
        unverified += 1;
      }
    }
  }
  return {
    ...envelope,
    payload: {
      ...payload,
      coverage: {
        scope: 'languageServer',
        exhaustive: false,
        ...(verified > 0 && { verifiedAliasBindings: verified }),
        ...(unverified > 0 && { unverifiedAliasBindings: unverified }),
        ...(uninspectedFiles > 0 && { uninspectedFiles }),
        ...(deferred && { deferredAliasInspection: true }),
      },
    },
    ...((verified > 0 || unverified > 0 || deferred) && {
      incompleteResults: true,
      partialReasons: [
        ...(envelope.partialReasons ?? []),
        'aliasReferences' as const,
      ],
    }),
    ...(unverified > 0 && { terminalLimit: true }),
    ...(Object.keys(next).length > 0 && { next }),
  };
}
