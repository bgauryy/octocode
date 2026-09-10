import { getGrammarCapabilities } from '../../grammarCapabilities.js';

function normalizeLanguageInput(raw: string): {
  normalized: string;
  exactExtension: boolean;
} {
  const trimmed = raw.trim().toLowerCase();
  if (trimmed.startsWith('*.')) {
    return { normalized: trimmed.slice(2), exactExtension: true };
  }
  if (trimmed.startsWith('.')) {
    return { normalized: trimmed.slice(1), exactExtension: true };
  }
  return { normalized: trimmed, exactExtension: false };
}

function capabilityMatchesSelector(
  selector: string,
  capability: ReturnType<typeof getGrammarCapabilities>[number]
): boolean {
  if (capability.language.toLowerCase() === selector) return true;
  if (capability.languageId?.toLowerCase() === selector) return true;
  return capability.selectorAliases.some(
    alias => alias.toLowerCase() === selector
  );
}

function extensionsForSelector(raw: string): string[] {
  const { normalized, exactExtension } = normalizeLanguageInput(raw);
  if (!normalized) return [];
  if (exactExtension) return [normalized];

  const familyExtensions = getGrammarCapabilities()
    .filter(capability => capabilityMatchesSelector(normalized, capability))
    .flatMap(capability => capability.extensions);
  if (familyExtensions.length > 0) {
    return [...new Set(familyExtensions)];
  }
  return [normalized];
}

export function toStructuralSearchIncludeGlobs(
  raw: string | undefined
): string[] | undefined {
  if (!raw?.trim()) return undefined;
  const extensions = extensionsForSelector(raw);
  const globs = extensions.filter(Boolean).map(ext => `*.${ext}`);
  return globs.length ? globs : undefined;
}
