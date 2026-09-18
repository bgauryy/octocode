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
import type { SymbolAnchoredSemanticQuery } from '../shared/semanticTypes.js';

const MAX_ALIAS_INSPECTIONS = 32;
const MAX_ALIAS_FILES = 100;

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
export async function collectVerifiedAliasReferences(
  query: SymbolAnchoredSemanticQuery,
  anchor: SymbolAnchor,
  client: LSPClient,
  locations: CodeSnippet[]
): Promise<{
  locations: CodeSnippet[];
  verified: number;
  unverified: number;
  uninspectedFiles: number;
}> {
  const files = anchor.resolvedSymbol.name
    ? [...new Set(locations.map(item => localPath(item.uri)))].sort()
    : [];
  const providerIdentities = new Set(locations.map(identity));
  let verified = 0;
  let unverified = 0;
  let uninspectedFiles = Math.max(0, files.length - MAX_ALIAS_FILES);
  let definitions: Set<string> | undefined;
  const inspectedBindings = new Set<string>();
  const recoveredLocations: CodeSnippet[] = [];
  for (const file of files.slice(0, MAX_ALIAS_FILES)) {
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
      // Only inspect imports already present in the provider's reference set.
      // Syntax supplies coordinates, never a substitute semantic identity.
      if (
        !providerIdentities.has(
          identity({ uri: file, range: binding.importedRange })
        )
      )
        continue;
      const bindingIdentity = identity({
        uri: file,
        range: binding.localRange,
      });
      if (inspectedBindings.has(bindingIdentity)) continue;
      // Providers that already include the local binding need no extra query.
      if (providerIdentities.has(bindingIdentity)) continue;
      if (inspectedBindings.size >= MAX_ALIAS_INSPECTIONS) {
        // The remaining syntax candidates are deliberately not queried.  They
        // are typed as incomplete below; no cursor can honestly resume an
        // unmaterialized definition-verification worklist.
        unverified += 1;
        continue;
      }
      inspectedBindings.add(bindingIdentity);
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
        // Query the semantically identical local binding now, so its references
        // enter the same canonical dedupe/order/pagination pass as provider refs.
        // Syntax only proposes this position; definition-set equality authorizes it.
        const aliasReferences = await client.findReferences(
          file,
          binding.localRange.start,
          query.includeDeclaration ?? true,
          source.value.content
        );
        recoveredLocations.push(...aliasReferences);
        verified += 1;
      } catch {
        unverified += 1;
      }
    }
  }
  return {
    locations: [...locations, ...recoveredLocations],
    verified,
    unverified,
    uninspectedFiles,
  };
}
