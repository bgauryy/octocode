export type OqlLanguageSelectorKind = 'extension' | 'language' | 'unknown';

export type OqlLanguageSelector = {
  raw: string;
  normalized: string;
  kind: OqlLanguageSelectorKind;
  canonicalLanguage?: string;
  extension?: string;
  extensions?: readonly string[];
};

type SelectorDefinition = Omit<OqlLanguageSelector, 'raw' | 'normalized'>;

const EXTENSION_SELECTORS: Readonly<Record<string, SelectorDefinition>> = {
  cjs: {
    kind: 'extension',
    canonicalLanguage: 'JavaScript',
    extension: 'cjs',
    extensions: ['cjs'],
  },
  cts: {
    kind: 'extension',
    canonicalLanguage: 'TypeScript',
    extension: 'cts',
    extensions: ['cts'],
  },
  js: {
    kind: 'extension',
    canonicalLanguage: 'JavaScript',
    extension: 'js',
    extensions: ['js'],
  },
  jsx: {
    kind: 'extension',
    canonicalLanguage: 'JavaScript',
    extension: 'jsx',
    extensions: ['jsx'],
  },
  md: {
    kind: 'extension',
    canonicalLanguage: 'Markdown',
    extension: 'md',
    extensions: ['md'],
  },
  mdx: {
    kind: 'extension',
    canonicalLanguage: 'MDX',
    extension: 'mdx',
    extensions: ['mdx'],
  },
  mjs: {
    kind: 'extension',
    canonicalLanguage: 'JavaScript',
    extension: 'mjs',
    extensions: ['mjs'],
  },
  mts: {
    kind: 'extension',
    canonicalLanguage: 'TypeScript',
    extension: 'mts',
    extensions: ['mts'],
  },
  py: {
    kind: 'extension',
    canonicalLanguage: 'Python',
    extension: 'py',
    extensions: ['py'],
  },
  rs: {
    kind: 'extension',
    canonicalLanguage: 'Rust',
    extension: 'rs',
    extensions: ['rs'],
  },
  ts: {
    kind: 'extension',
    canonicalLanguage: 'TypeScript',
    extension: 'ts',
    extensions: ['ts'],
  },
  tsx: {
    kind: 'extension',
    canonicalLanguage: 'TypeScript',
    extension: 'tsx',
    extensions: ['tsx'],
  },
};

const LANGUAGE_SELECTORS: Readonly<Record<string, SelectorDefinition>> = {
  javascript: {
    kind: 'language',
    canonicalLanguage: 'JavaScript',
    extensions: ['js', 'jsx', 'mjs', 'cjs'],
  },
  markdown: {
    kind: 'language',
    canonicalLanguage: 'Markdown',
    extensions: ['md', 'markdown'],
  },
  python: {
    kind: 'language',
    canonicalLanguage: 'Python',
    extensions: ['py', 'pyi'],
  },
  rust: {
    kind: 'language',
    canonicalLanguage: 'Rust',
    extensions: ['rs'],
  },
  typescript: {
    kind: 'language',
    canonicalLanguage: 'TypeScript',
    extensions: ['ts', 'tsx', 'mts', 'cts'],
  },
};

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

export function toGithubRepositoryLanguage(
  raw: string | undefined
): string | undefined {
  const selector = classifyLanguageSelector(raw);
  return selector?.canonicalLanguage;
}
