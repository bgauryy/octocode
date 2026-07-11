import path from 'node:path';
import { readFile } from 'node:fs/promises';
import { searchContentRipgrep } from '../../local_ripgrep/searchContentRipgrep.js';
import { acquirePooledClient } from '@octocodeai/octocode-engine/lsp/manager';
import { resolveImportAliasDefinitions } from '@octocodeai/octocode-engine/lsp/resolver';
import type { SymbolAnchor } from '../shared/resolveSymbolAnchor.js';
import type {
  LspSemanticEnvelope,
  SymbolAnchoredSemanticQuery,
} from '../shared/semanticTypes.js';
import {
  callsEnvelope,
  emptyEnvelope,
  hoverEnvelope,
  locationsEnvelope,
  referencesEnvelope,
  typeHierarchyEnvelope,
} from './semanticEnvelopes.js';

// Relation queries (references/calls) are bounded by the server's open-file
// set. Before running one, open a bounded set of files that mention the
// symbol by name so cross-file relations are visible — otherwise a fresh
// server reports only same-file results and a zero reads as "unused".
const CONSUMER_SCOPED_TYPES: ReadonlySet<string> = new Set([
  'references',
  'callers',
  'callees',
  'callHierarchy',
  'implementation',
]);
const WARM_MAX_FILES = 12;
const WARM_MAX_BYTES = 512 * 1024;
const JS_TS_FAMILY = ['ts', 'tsx', 'mts', 'cts', 'js', 'jsx', 'mjs', 'cjs'];

export { CONSUMER_SCOPED_TYPES };

export async function warmLikelyConsumers(
  client: NonNullable<Awaited<ReturnType<typeof acquirePooledClient>>>,
  anchor: SymbolAnchor,
  workspaceRoot: string
): Promise<void> {
  if (typeof client.openDocument !== 'function') return;
  try {
    const ext = path.extname(anchor.uri).slice(1);
    const family = JS_TS_FAMILY.includes(ext) ? JS_TS_FAMILY : [ext];
    const result = await searchContentRipgrep({
      path: workspaceRoot,
      keywords: anchor.resolvedSymbol.name,
      fixedString: true,
      wholeWord: true,
      filesOnly: true,
      maxFiles: WARM_MAX_FILES,
      include: family.filter(Boolean).map(e => `*.${e}`),
    } as Parameters<typeof searchContentRipgrep>[0]);
    for (const file of result.files ?? []) {
      const filePath = typeof file.path === 'string' ? file.path : undefined;
      if (!filePath) continue;
      const abs = path.isAbsolute(filePath)
        ? filePath
        : path.join(workspaceRoot, filePath);
      if (path.resolve(abs) === path.resolve(anchor.uri)) continue;
      try {
        const content = await readFile(abs, 'utf-8');
        if (content.length > WARM_MAX_BYTES) continue;
        await client.openDocument(abs, content);
      } catch {
        // best-effort warm: unreadable candidates are skipped
      }
    }
  } catch {
    // best-effort warm: the relation query still runs on the anchor alone
  }
}

export async function dispatchAnchoredSemantic(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  client: NonNullable<Awaited<ReturnType<typeof acquirePooledClient>>>
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
        await resolveImportAliasDefinitions({
          anchorUri: anchor.uri,
          symbolName: anchor.resolvedSymbol.name,
          locations: await client.gotoDefinition(
            anchor.uri,
            anchor.resolvedSymbol.position,
            anchor.content
          ),
        })
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
          anchor.uri,
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
          anchor.uri,
          anchor.resolvedSymbol.position,
          anchor.content
        )
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
          anchor.uri,
          anchor.resolvedSymbol.position,
          query.includeDeclaration ?? true,
          anchor.content
        )
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
          anchor.uri,
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
      return callsEnvelope(query, anchor, client);
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
