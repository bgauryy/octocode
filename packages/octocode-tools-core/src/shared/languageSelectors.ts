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
  bash: {
    kind: 'extension',
    canonicalLanguage: 'Shell',
    extension: 'bash',
    extensions: ['bash'],
  },
  c: {
    kind: 'extension',
    canonicalLanguage: 'C',
    extension: 'c',
    extensions: ['c'],
  },
  cc: {
    kind: 'extension',
    canonicalLanguage: 'C++',
    extension: 'cc',
    extensions: ['cc'],
  },
  cjs: {
    kind: 'extension',
    canonicalLanguage: 'JavaScript',
    extension: 'cjs',
    extensions: ['cjs'],
  },
  cpp: {
    kind: 'extension',
    canonicalLanguage: 'C++',
    extension: 'cpp',
    extensions: ['cpp'],
  },
  cs: {
    kind: 'extension',
    canonicalLanguage: 'C#',
    extension: 'cs',
    extensions: ['cs'],
  },
  cts: {
    kind: 'extension',
    canonicalLanguage: 'TypeScript',
    extension: 'cts',
    extensions: ['cts'],
  },
  cxx: {
    kind: 'extension',
    canonicalLanguage: 'C++',
    extension: 'cxx',
    extensions: ['cxx'],
  },
  go: {
    kind: 'extension',
    canonicalLanguage: 'Go',
    extension: 'go',
    extensions: ['go'],
  },
  h: {
    kind: 'extension',
    canonicalLanguage: 'C',
    extension: 'h',
    extensions: ['h'],
  },
  html: {
    kind: 'extension',
    canonicalLanguage: 'HTML',
    extension: 'html',
    extensions: ['html'],
  },
  htm: {
    kind: 'extension',
    canonicalLanguage: 'HTML',
    extension: 'htm',
    extensions: ['htm'],
  },
  hpp: {
    kind: 'extension',
    canonicalLanguage: 'C++',
    extension: 'hpp',
    extensions: ['hpp'],
  },
  js: {
    kind: 'extension',
    canonicalLanguage: 'JavaScript',
    extension: 'js',
    extensions: ['js'],
  },
  json: {
    kind: 'extension',
    canonicalLanguage: 'JSON',
    extension: 'json',
    extensions: ['json'],
  },
  jsonc: {
    kind: 'extension',
    canonicalLanguage: 'JSON',
    extension: 'jsonc',
    extensions: ['jsonc'],
  },
  jsx: {
    kind: 'extension',
    canonicalLanguage: 'JavaScript',
    extension: 'jsx',
    extensions: ['jsx'],
  },
  java: {
    kind: 'extension',
    canonicalLanguage: 'Java',
    extension: 'java',
    extensions: ['java'],
  },
  less: {
    kind: 'extension',
    canonicalLanguage: 'Less',
    extension: 'less',
    extensions: ['less'],
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
  pyi: {
    kind: 'extension',
    canonicalLanguage: 'Python',
    extension: 'pyi',
    extensions: ['pyi'],
  },
  rs: {
    kind: 'extension',
    canonicalLanguage: 'Rust',
    extension: 'rs',
    extensions: ['rs'],
  },
  scala: {
    kind: 'extension',
    canonicalLanguage: 'Scala',
    extension: 'scala',
    extensions: ['scala'],
  },
  sc: {
    kind: 'extension',
    canonicalLanguage: 'Scala',
    extension: 'sc',
    extensions: ['sc'],
  },
  scss: {
    kind: 'extension',
    canonicalLanguage: 'SCSS',
    extension: 'scss',
    extensions: ['scss'],
  },
  sh: {
    kind: 'extension',
    canonicalLanguage: 'Shell',
    extension: 'sh',
    extensions: ['sh'],
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
  toml: {
    kind: 'extension',
    canonicalLanguage: 'TOML',
    extension: 'toml',
    extensions: ['toml'],
  },
  yaml: {
    kind: 'extension',
    canonicalLanguage: 'YAML',
    extension: 'yaml',
    extensions: ['yaml'],
  },
  yml: {
    kind: 'extension',
    canonicalLanguage: 'YAML',
    extension: 'yml',
    extensions: ['yml'],
  },
  zsh: {
    kind: 'extension',
    canonicalLanguage: 'Shell',
    extension: 'zsh',
    extensions: ['zsh'],
  },
};

const LANGUAGE_SELECTORS: Readonly<Record<string, SelectorDefinition>> = {
  bash: {
    kind: 'language',
    canonicalLanguage: 'Shell',
    extensions: ['sh', 'bash', 'zsh'],
  },
  c: {
    kind: 'language',
    canonicalLanguage: 'C',
    extensions: ['c', 'h'],
  },
  'c++': {
    kind: 'language',
    canonicalLanguage: 'C++',
    extensions: ['cpp', 'hpp', 'cc', 'cxx', 'hh', 'hxx'],
  },
  cpp: {
    kind: 'language',
    canonicalLanguage: 'C++',
    extensions: ['cpp', 'hpp', 'cc', 'cxx', 'hh', 'hxx'],
  },
  csharp: {
    kind: 'language',
    canonicalLanguage: 'C#',
    extensions: ['cs'],
  },
  css: {
    kind: 'language',
    canonicalLanguage: 'CSS',
    extensions: ['css'],
  },
  go: {
    kind: 'language',
    canonicalLanguage: 'Go',
    extensions: ['go'],
  },
  html: {
    kind: 'language',
    canonicalLanguage: 'HTML',
    extensions: ['html', 'htm'],
  },
  java: {
    kind: 'language',
    canonicalLanguage: 'Java',
    extensions: ['java'],
  },
  javascript: {
    kind: 'language',
    canonicalLanguage: 'JavaScript',
    extensions: ['js', 'jsx', 'mjs', 'cjs'],
  },
  json: {
    kind: 'language',
    canonicalLanguage: 'JSON',
    extensions: ['json', 'jsonc'],
  },
  less: {
    kind: 'language',
    canonicalLanguage: 'Less',
    extensions: ['less'],
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
  scala: {
    kind: 'language',
    canonicalLanguage: 'Scala',
    extensions: ['scala', 'sc', 'sbt'],
  },
  scss: {
    kind: 'language',
    canonicalLanguage: 'SCSS',
    extensions: ['scss'],
  },
  shell: {
    kind: 'language',
    canonicalLanguage: 'Shell',
    extensions: ['sh', 'bash', 'zsh'],
  },
  typescript: {
    kind: 'language',
    canonicalLanguage: 'TypeScript',
    extensions: ['ts', 'tsx', 'mts', 'cts'],
  },
  toml: {
    kind: 'language',
    canonicalLanguage: 'TOML',
    extensions: ['toml'],
  },
  yaml: {
    kind: 'language',
    canonicalLanguage: 'YAML',
    extensions: ['yaml', 'yml'],
  },
  yml: {
    kind: 'language',
    canonicalLanguage: 'YAML',
    extensions: ['yaml', 'yml'],
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
