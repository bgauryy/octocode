import {
  EXTENSION_SELECTORS,
  LANGUAGE_SELECTORS,
  type OqlLanguageSelector,
} from './data.js';

function normalizeLanguageInput(raw: string): string {
  return raw.trim().replace(/^\./, '').toLowerCase();
}

export function classifyLanguageSelector(
  raw: string | undefined
): OqlLanguageSelector | undefined {
  if (!raw?.trim()) return undefined;
  const normalized = normalizeLanguageInput(raw);
  const definition =
    EXTENSION_SELECTORS[normalized] ?? LANGUAGE_SELECTORS[normalized];
  if (!definition) {
    return {
      raw,
      normalized,
      kind: 'unknown',
      canonicalLanguage: raw.trim(),
    };
  }
  return {
    raw,
    normalized,
    ...definition,
  };
}

export type GithubCodeLanguageParams = {
  language?: string;
  extension?: string;
};

export type LocalSearchLanguageParams = {
  langType?: string;
  include?: string[];
};

function extensionGlobs(extensions: readonly string[] | undefined): string[] {
  return [...(extensions ?? [])].map(extension => `**/*.${extension}`);
}

function basenameGlobs(extensions: readonly string[] | undefined): string[] {
  return [...(extensions ?? [])].map(extension => `*.${extension}`);
}

export function toGithubCodeLanguageParams(
  raw: string | undefined
): GithubCodeLanguageParams {
  const selector = classifyLanguageSelector(raw);
  if (!selector) return {};
  if (selector.kind === 'extension' && selector.extension) {
    return { extension: selector.extension };
  }
  if (selector.canonicalLanguage) {
    return { language: selector.canonicalLanguage };
  }
  return {};
}

export function toLocalSearchLanguageParams(
  raw: string | undefined
): LocalSearchLanguageParams {
  const selector = classifyLanguageSelector(raw);
  if (!selector) return {};
  if (selector.kind === 'extension' && selector.extension) {
    return { include: extensionGlobs([selector.extension]) };
  }
  if (selector.kind === 'language') {
    return { langType: selector.normalized };
  }
  return selector.normalized ? { langType: selector.normalized } : {};
}

export function toLocalFileLanguageGlobs(raw: string | undefined): string[] {
  const selector = classifyLanguageSelector(raw);
  if (!selector) return [];
  if (selector.kind === 'extension' && selector.extension) {
    return basenameGlobs([selector.extension]);
  }
  if (selector.kind === 'language') {
    return basenameGlobs(selector.extensions);
  }
  return selector.normalized ? basenameGlobs([selector.normalized]) : [];
}

export function toStructuralSearchIncludeGlobs(
  raw: string | undefined
): string[] | undefined {
  const selector = classifyLanguageSelector(raw);
  if (!selector) return undefined;
  const extensions = selector.extensions?.length
    ? selector.extensions
    : [selector.normalized.replace(/^[.*]+/, '')];
  const globs = extensions.filter(Boolean).map(ext => `*.${ext}`);
  return globs.length ? globs : undefined;
}

export function toGithubRepositoryLanguage(
  raw: string | undefined
): string | undefined {
  const selector = classifyLanguageSelector(raw);
  return selector?.canonicalLanguage;
}
