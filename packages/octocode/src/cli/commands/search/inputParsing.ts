import type { ParsedArgs } from '../../types.js';
import { getBool, getString, intFlag, isFlagError } from '../../options.js';
import { c, dim } from '../../../utils/colors.js';
import { normalizeEntryType } from './corpusResolution.js';
import type { CliSearchShorthand } from './types.js';

/**
 * Map friendly `--target` abbreviations to the canonical OQL enum. The top-level
 * help and agent prompt advertise short forms (`repos`, `PRs`); without this an
 * agent copying them hits "--target must be one of …". Unknown values pass
 * through unchanged so the OQL layer still validates real typos. Mutates in place.
 */
const TARGET_ALIASES: Record<string, string> = {
  repo: 'repositories',
  repos: 'repositories',
  pr: 'pullRequests',
  prs: 'pullRequests',
  pullrequest: 'pullRequests',
  pullrequests: 'pullRequests',
  commit: 'commits',
  package: 'packages',
  pkg: 'packages',
  npm: 'packages',
};
export function normalizeTargetAlias(options: Record<string, unknown>): void {
  const raw = options['target'];
  if (typeof raw !== 'string') return;
  const canonical = TARGET_ALIASES[raw.trim().toLowerCase()];
  if (canonical) options['target'] = canonical;
}

export function parseOqlQueryJson(text: string): unknown {
  const parsed = JSON.parse(text) as unknown;
  return Array.isArray(parsed) ? { schema: 'oql', queries: parsed } : parsed;
}

export function looksLikeJsonText(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.startsWith('{') || trimmed.startsWith('[');
}

export function looksLikeJsonFile(value: string): boolean {
  return /\.(?:json|jsonc)$/i.test(value) || /\.oql$/i.test(value);
}

export function listOption(value: string): string[] {
  return value
    .split(',')
    .map(v => v.trim())
    .filter(Boolean);
}

export function parseBooleanString(
  value: string | undefined
): boolean | undefined {
  if (!value) return undefined;
  const normalized = value.trim().toLowerCase();
  if (['true', '1', 'yes', 'y'].includes(normalized)) return true;
  if (['false', '0', 'no', 'n'].includes(normalized)) return false;
  return undefined;
}

export function isParseError(value: unknown): value is { error: string } {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { error?: unknown }).error === 'string'
  );
}

export function parseNumericOptions(
  options: ParsedArgs['options']
): Partial<CliSearchShorthand> | { error: string } {
  const parsed: Record<string, number> = {};
  const fields: Array<[string, string, number]> = [
    ['limit', 'limit', 1],
    ['page', 'page', 1],
    ['items-per-page', 'itemsPerPage', 1],
    ['min-depth', 'minDepth', 0],
    ['max-depth', 'maxDepth', 0],
    ['context-lines', 'contextLines', 0],
    ['context', 'contextLines', 0],
    ['match-window', 'matchWindow', 0],
    ['match-length', 'matchContentLength', 1],
    ['max-matches', 'maxMatchesPerFile', 1],
    ['match-page', 'matchPage', 1],
    ['max-files', 'maxFiles', 1],
    ['start-line', 'startLine', 1],
    ['end-line', 'endLine', 1],
    ['char-offset', 'charOffset', 0],
    ['char-length', 'charLength', 1],
    ['line', 'line', 1],
    ['depth', 'depth', 0],
    ['pr', 'prNumber', 1],
    ['file-page', 'filePage', 1],
    ['comment-page', 'commentPage', 1],
    ['commit-page', 'commitPage', 1],
    ['proof-limit', 'proofLimit', 1],
  ];
  for (const [flag, prop, min] of fields) {
    const raw = getString(options, flag);
    const value = intFlag(raw, flag, { min });
    if (isFlagError(value)) return value;
    if (value !== undefined) parsed[prop] = value;
  }
  const start = parsed.startLine;
  const end = parsed.endLine;
  if (start !== undefined && end !== undefined && end < start) {
    return {
      error: '--end-line must be greater than or equal to --start-line.',
    };
  }
  return parsed as Partial<CliSearchShorthand>;
}

export function validateShorthandOptions(
  args: ParsedArgs,
  predicate: { pattern?: string; ruleText?: string; regex?: string }
): string | undefined {
  const { options } = args;
  const explicitPredicates = [
    predicate.pattern ? '--pattern' : undefined,
    predicate.ruleText ? '--rule' : undefined,
    predicate.regex ? '--regex/--path-pattern' : undefined,
  ].filter(Boolean);
  if (explicitPredicates.length > 1) {
    return `Use one predicate flag at a time, not ${explicitPredicates.join(', ')}.`;
  }
  if (
    getBool(options, 'case-insensitive') &&
    getBool(options, 'case-sensitive')
  ) {
    return 'Use either --case-insensitive or --case-sensitive, not both.';
  }
  const target = getString(options, 'target');
  if (
    predicate.pattern &&
    (getBool(options, 'tree') ||
      (target && target !== 'code' && target !== 'files'))
  ) {
    return '--pattern is for AST search on code/files. For tree or file-name globs use --name or --path-pattern.';
  }
  if (getString(options, 'op') && target && target !== 'semantics') {
    return '--op is only valid with --target semantics (or no --target).';
  }
  const search = getString(options, 'search');
  if (search && !['path', 'content', 'both'].includes(search)) {
    return '--search must be path, content, or both.';
  }
  if (
    search &&
    target &&
    target !== 'files' &&
    target !== 'code' &&
    target !== 'content'
  ) {
    return '--search is only valid for code/files/content-style queries.';
  }
  const contentView = getString(options, 'content-view');
  // 'exact'/'compact' are accepted as deprecated aliases for 'none'/'standard'
  // (pre-rename vocabulary) so an existing example or script isn't broken.
  if (
    contentView &&
    !['none', 'standard', 'symbols', 'exact', 'compact'].includes(contentView)
  ) {
    return '--content-view must be none, standard, or symbols.';
  }
  const materialize = getString(options, 'materialize');
  if (materialize && !['never', 'auto', 'required'].includes(materialize)) {
    return '--materialize must be never, auto, or required.';
  }
  const entry = getString(options, 'entry');
  if (entry && !normalizeEntryType(entry)) {
    return '--entry must be file, directory, f, or d.';
  }
  return undefined;
}

/**
 * Warn (to stderr, never fatal) about silently-ignored shorthand mistakes:
 *  • Extra path positionals — `search` takes ONE corpus, so `search t a.ts b.ts`
 *    quietly searched only `a.ts`. Conservative: text/diff lanes consume 2
 *    positionals, flag/target-only lanes consume 1, so we only flag the surplus.
 *  • A grep-style `\|` in a LITERAL text term — it matches verbatim (a no-op for
 *    alternation); point at `--regex`, which is what the user meant.
 *  • A plain-word second positional for corpus-optional targets (repositories,
 *    packages) — these never use a corpus, so the arg is silently discarded.
 * stderr keeps stdout (YAML/JSON results) clean, so this is safe in every mode.
 */
export function emitSearchInputWarnings(o: {
  positionals: string[];
  text: string | undefined;
  fromFlag: boolean;
  targetOnly: boolean;
  hasDiff: boolean;
  explicitTarget: string | undefined;
  positionalTargetArg: string | undefined;
}): void {
  const consumed = o.hasDiff ? 2 : o.fromFlag || o.targetOnly ? 1 : 2;
  const ignored = o.positionals.slice(consumed);
  if (ignored.length > 0) {
    const list = ignored.map(s => `'${s}'`).join(', ');
    process.stderr.write(
      `  ${c('yellow', '!')} ${dim(`ignored extra argument${ignored.length > 1 ? 's' : ''} ${list} — search takes a single corpus (one path or owner/repo). Search a directory, or narrow with`)} ${c('cyan', '--include <glob>')}${dim('.')}\n`
    );
  }
  if (o.text && o.text.includes('\\|')) {
    const asRegex = o.text.replace(/\\\|/g, '|');
    process.stderr.write(
      `  ${c('yellow', '!')} ${dim(`'${o.text}' is matched literally — \`\\|\` is not alternation. For OR-matching use`)} ${c('cyan', `--regex '${asRegex}'`)}${dim('.')}\n`
    );
  }
  // Corpus-optional targets (repositories, packages) never use a positional
  // corpus — it gets silently dropped. Warn when the second positional looks like
  // a keyword (no `/`), not a real owner/repo or path reference.
  if (
    (o.explicitTarget === 'repositories' || o.explicitTarget === 'packages') &&
    !o.fromFlag &&
    !o.targetOnly &&
    o.positionalTargetArg !== undefined &&
    !o.positionalTargetArg.includes('/')
  ) {
    const combined = [o.text, o.positionalTargetArg].filter(Boolean).join(' ');
    process.stderr.write(
      `  ${c('yellow', '!')} ${dim(
        `'${o.positionalTargetArg}' was treated as a corpus and ignored — ${o.explicitTarget} search has no corpus. To AND-match both words, quote them:`
      )} ${c('cyan', `'${combined}'`)}${dim('.')}\n`
    );
  }
}
