/**
 * Shorthand lowering for `octocode search`.
 *
 * The CLI reads argv and resolves a target string to a corpus (local path vs
 * GitHub ref — the only step that needs the filesystem). Everything else —
 * which predicate the flags select, dialect, lang requirements, and assembling
 * the sugar object the normalizer accepts — lives here so it is owned once in
 * tools-core (not re-implemented in the interface) and is unit-testable without
 * argv or a terminal.
 */
import type { OqlInputQueryV1 } from './types.js';

/** Corpus already classified by the caller (local path vs GitHub ref). */
export type ShorthandCorpus =
  | { kind: 'local'; path: string }
  | { kind: 'github'; repo: string; path?: string; ref?: string };

export interface SearchShorthand {
  /** literal text term (used when no pattern/rule/regex flag is set) */
  text?: string;
  /** regex pattern (rust dialect unless pcre2) */
  regex?: string;
  pcre2?: boolean;
  /** structural AST pattern (requires lang) */
  pattern?: string;
  /** structural rule, already parsed from JSON (requires lang) */
  rule?: unknown;
  lang?: string;
  /** scope language/extension (maps to scope.language) */
  type?: string;
  corpus: ShorthandCorpus;
  materialize?: 'never' | 'auto' | 'required';
}

export type ShorthandResult = { input: OqlInputQueryV1 } | { error: string };

/**
 * Lower shorthand parts into the OQL sugar object. Predicate precedence:
 * pattern > rule > regex > text. Returns a typed error for invalid combos
 * (e.g. structural without `lang`) instead of throwing.
 */
export function buildShorthandInput(parts: SearchShorthand): ShorthandResult {
  const sugar: Record<string, unknown> = {};

  if (parts.pattern !== undefined) {
    if (!parts.lang)
      return { error: '--pattern requires --lang (e.g. --lang ts).' };
    sugar.pattern = parts.pattern;
    sugar.lang = parts.lang;
  } else if (parts.rule !== undefined) {
    if (!parts.lang)
      return { error: '--rule requires --lang (e.g. --lang ts).' };
    sugar.rule = parts.rule;
    sugar.lang = parts.lang;
  } else if (parts.regex !== undefined) {
    sugar.where = {
      kind: 'regex',
      value: parts.regex,
      ...(parts.pcre2 ? { dialect: 'pcre2' } : {}),
    };
  } else if (parts.text !== undefined && parts.text !== '') {
    sugar.text = parts.text;
  } else {
    return {
      error: 'No search term: provide text, --regex, --pattern, or --rule.',
    };
  }

  // corpus
  if (parts.corpus.kind === 'github') {
    sugar.repo = parts.corpus.repo;
    if (parts.corpus.path) sugar.path = parts.corpus.path;
    if (parts.corpus.ref) sugar.ref = parts.corpus.ref;
  } else {
    sugar.path = parts.corpus.path;
  }

  if (parts.type) sugar.langType = parts.type;
  if (parts.materialize) sugar.materialize = parts.materialize;

  return { input: sugar as OqlInputQueryV1 };
}
