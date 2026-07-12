import type { ParsedArgs } from '../../types.js';
import { getBool, getString } from '../../options.js';
import { listOption, parseBooleanString } from './inputParsing.js';
import type {
  CliSearchShorthand,
  CliShorthandCorpus,
  GithubDiffShortcut,
} from './types.js';

export interface ShorthandPartsContext {
  options: ParsedArgs['options'];
  text: string | undefined;
  regex: string | undefined;
  pattern: string | undefined;
  rule: unknown;
  target: string | undefined;
  view: string | undefined;
  contentView: string | undefined;
  resolvedCorpus: CliShorthandCorpus;
  materialize: string | undefined;
  entry: 'file' | 'directory' | undefined;
  op: string | undefined;
  prTarget: { owner: string; repo: string; prNumber?: number } | undefined;
  pullRequestFilePatch: string | undefined;
  githubDiff: GithubDiffShortcut | undefined;
  diffPath: string | undefined;
  numeric: Partial<CliSearchShorthand>;
}

/**
 * Assemble the CLI sugar object from argv options plus the already-resolved
 * shorthand context (corpus, target, predicate, PR/diff shortcuts, numeric
 * flags). Pure object construction — `buildSugar` computes the context above
 * this then hands off to `buildShorthandInput` (tools-core) for lowering.
 */
export function buildShorthandParts(
  ctx: ShorthandPartsContext
): CliSearchShorthand {
  const {
    options,
    text,
    regex,
    pattern,
    rule,
    target,
    view,
    contentView,
    resolvedCorpus,
    materialize,
    entry,
    op,
    prTarget,
    pullRequestFilePatch,
    githubDiff,
    diffPath,
    numeric,
  } = ctx;

  return {
    ...(text !== undefined ? { text } : {}),
    ...(regex !== undefined ? { regex } : {}),
    ...(getBool(options, 'pcre2') ? { pcre2: true } : {}),
    ...(pattern !== undefined ? { pattern } : {}),
    ...(rule !== undefined ? { rule } : {}),
    ...(target ? { target } : {}),
    ...(view ? { view } : {}),
    ...(contentView ? { contentView } : {}),
    ...(getString(options, 'search')
      ? { search: getString(options, 'search') }
      : {}),
    ...(getString(options, 'lang') ? { lang: getString(options, 'lang') } : {}),
    corpus: resolvedCorpus,
    ...(materialize ? { materialize: materialize as never } : {}),
    ...(getString(options, 'branch')
      ? { branch: getString(options, 'branch') }
      : {}),
    ...(getBool(options, 'force-refresh') ? { forceRefresh: true } : {}),
    ...(getString(options, 'include')
      ? { include: listOption(getString(options, 'include')) }
      : {}),
    ...(getString(options, 'exclude')
      ? { exclude: listOption(getString(options, 'exclude')) }
      : {}),
    ...(getString(options, 'exclude-dir')
      ? { excludeDir: listOption(getString(options, 'exclude-dir')) }
      : {}),
    ...(getString(options, 'ext')
      ? { extension: getString(options, 'ext') }
      : {}),
    ...(getString(options, 'name', 'filename')
      ? { filename: getString(options, 'name', 'filename') }
      : {}),
    ...(getString(options, 'path-pattern') && !getString(options, 'regex')
      ? { pathPattern: getString(options, 'path-pattern') }
      : {}),
    ...(entry ? { entryType: entry as never } : {}),
    ...(getBool(options, 'files-only') ? { filesOnly: true } : {}),
    ...(getBool(options, 'empty') ? { empty: true } : {}),
    ...(getString(options, 'modified-within')
      ? { modifiedWithin: getString(options, 'modified-within') }
      : {}),
    ...(getString(options, 'modified-before')
      ? { modifiedBefore: getString(options, 'modified-before') }
      : {}),
    ...(getString(options, 'accessed-within')
      ? { accessedWithin: getString(options, 'accessed-within') }
      : {}),
    ...(getString(options, 'size-greater')
      ? { sizeGreater: getString(options, 'size-greater') }
      : {}),
    ...(getString(options, 'size-less')
      ? { sizeLess: getString(options, 'size-less') }
      : {}),
    ...(getString(options, 'permissions')
      ? { permissions: getString(options, 'permissions') }
      : {}),
    ...(getBool(options, 'executable') ? { executable: true } : {}),
    ...(getBool(options, 'readable') ? { readable: true } : {}),
    ...(getBool(options, 'writable') ? { writable: true } : {}),
    ...(getBool(options, 'details') ? { details: true } : {}),
    ...(getBool(options, 'show-modified') ? { showModified: true } : {}),
    ...(getBool(options, 'hidden') ? { hidden: true } : {}),
    ...(getBool(options, 'no-ignore') ? { noIgnore: true } : {}),
    ...(getBool(options, 'case-insensitive') ? { caseInsensitive: true } : {}),
    ...(getBool(options, 'case-sensitive') ? { caseSensitive: true } : {}),
    ...(getBool(options, 'whole-word') ? { wholeWord: true } : {}),
    ...(getBool(options, 'fixed') ? { fixedString: true } : {}),
    ...(getBool(options, 'multiline') ? { multiline: true } : {}),
    ...(getBool(options, 'multiline-dotall') ? { multilineDotall: true } : {}),
    ...(getBool(options, 'files-without-match')
      ? { filesWithoutMatch: true }
      : {}),
    ...(getBool(options, 'count-lines') ? { countLinesPerFile: true } : {}),
    ...(getBool(options, 'count-matches') ? { countMatchesPerFile: true } : {}),
    ...(getBool(options, 'only-matching') ? { onlyMatching: true } : {}),
    ...(getBool(options, 'unique') ? { unique: true } : {}),
    ...(getBool(options, 'count') ? { countUnique: true } : {}),
    ...(getBool(options, 'invert-match') ? { invertMatch: true } : {}),
    ...(getBool(options, 'sort-reverse') ? { sortReverse: true } : {}),
    ...(getBool(options, 'debug-ranking') ? { debugRanking: true } : {}),
    ...(getString(options, 'sort') ? { sort: getString(options, 'sort') } : {}),
    ...(getString(options, 'ranking-profile')
      ? { rankingProfile: getString(options, 'ranking-profile') }
      : {}),
    ...(getString(options, 'match-string')
      ? { matchString: getString(options, 'match-string') }
      : {}),
    ...(getBool(options, 'match-regex') ? { matchRegex: true } : {}),
    ...(getBool(options, 'match-case-sensitive')
      ? { matchCaseSensitive: true }
      : {}),
    ...(getBool(options, 'full-content') ? { fullContent: true } : {}),
    ...(getBool(options, 'tree') ? { tree: true } : {}),
    ...(getBool(options, 'include-sizes') ? { includeSizes: true } : {}),
    ...(op ? { op } : {}),
    ...(getString(options, 'symbol')
      ? { symbol: getString(options, 'symbol') }
      : {}),
    ...(getString(options, 'kind')
      ? { symbolKind: getString(options, 'kind') }
      : {}),
    ...(getString(options, 'uri') ? { uri: getString(options, 'uri') } : {}),
    ...(target === 'semantics' && getString(options, 'order')
      ? { order: Number.parseInt(getString(options, 'order'), 10) }
      : {}),
    ...(getString(options, 'workspace-root')
      ? { workspaceRoot: getString(options, 'workspace-root') }
      : {}),
    ...(getString(options, 'format')
      ? { format: getString(options, 'format') }
      : {}),
    ...(getString(options, 'owner')
      ? { owner: getString(options, 'owner') }
      : {}),
    ...(getString(options, 'topic')
      ? { topic: listOption(getString(options, 'topic')) }
      : {}),
    ...(getString(options, 'stars')
      ? { stars: getString(options, 'stars') }
      : {}),
    ...(getString(options, 'forks')
      ? { forks: getString(options, 'forks') }
      : {}),
    ...(getString(options, 'good-first-issues')
      ? { goodFirstIssues: getString(options, 'good-first-issues') }
      : {}),
    ...(getString(options, 'license')
      ? { license: getString(options, 'license') }
      : {}),
    ...(getString(options, 'created')
      ? { created: getString(options, 'created') }
      : {}),
    ...(getString(options, 'updated')
      ? { updated: getString(options, 'updated') }
      : {}),
    ...(getString(options, 'closed')
      ? { closed: getString(options, 'closed') }
      : {}),
    ...(getString(options, 'merged-at')
      ? { mergedAt: getString(options, 'merged-at') }
      : {}),
    ...(getString(options, 'size') ? { size: getString(options, 'size') } : {}),
    ...(getString(options, 'match')
      ? { match: listOption(getString(options, 'match')) }
      : {}),
    ...(getString(options, 'archived')
      ? { archived: parseBooleanString(getString(options, 'archived')) }
      : {}),
    ...(getString(options, 'visibility')
      ? { visibility: getString(options, 'visibility') }
      : {}),
    ...(getBool(options, 'concise') ? { concise: true } : {}),
    ...(getString(options, 'state')
      ? { state: getString(options, 'state') }
      : {}),
    ...(getString(options, 'author')
      ? { author: getString(options, 'author') }
      : {}),
    ...(getString(options, 'label')
      ? { label: getString(options, 'label') }
      : {}),
    ...(prTarget?.prNumber !== undefined
      ? { prNumber: prTarget.prNumber }
      : {}),
    ...(getString(options, 'base') ? { base: getString(options, 'base') } : {}),
    ...(getString(options, 'head') ? { head: getString(options, 'head') } : {}),
    ...(target === 'pullRequests' && getString(options, 'order')
      ? { orderDirection: getString(options, 'order') }
      : {}),
    ...(getBool(options, 'draft') ? { draft: true } : {}),
    ...(getBool(options, 'comments') ? { commentsContent: true } : {}),
    ...(getBool(options, 'commits') ? { commitsContent: true } : {}),
    ...(getBool(options, 'deep') ? { deep: true } : {}),
    ...(pullRequestFilePatch ? { patchFile: pullRequestFilePatch } : {}),
    ...(getString(options, 'review-mode')
      ? { reviewMode: getString(options, 'review-mode') }
      : {}),
    ...(getString(options, 'since')
      ? { since: getString(options, 'since') }
      : {}),
    ...(getString(options, 'until')
      ? { until: getString(options, 'until') }
      : {}),
    ...(getBool(options, 'patches') ? { patches: true } : {}),
    ...(githubDiff
      ? { baseRef: githubDiff.baseRef, headRef: githubDiff.headRef }
      : {}),
    ...(!githubDiff && getString(options, 'base-ref')
      ? { baseRef: getString(options, 'base-ref') }
      : {}),
    ...(!githubDiff && getString(options, 'head-ref')
      ? { headRef: getString(options, 'head-ref') }
      : {}),
    ...(target === 'diff' && getString(options, 'base')
      ? { baseRef: getString(options, 'base') }
      : {}),
    ...(target === 'diff' && getString(options, 'head')
      ? { headRef: getString(options, 'head') }
      : {}),
    ...(diffPath ? { diffPath } : {}),
    ...(getBool(options, 'detailed') ? { detailed: true } : {}),
    ...(getBool(options, 'verbose') ? { verbose: true } : {}),
    ...(getString(options, 'intent')
      ? { intent: getString(options, 'intent') }
      : {}),
    ...(getString(options, 'facets')
      ? { facets: listOption(getString(options, 'facets')) }
      : {}),
    ...(getString(options, 'proof')
      ? { proof: getString(options, 'proof') }
      : {}),
    ...(getBool(options, 'offsets') ? { includeOffsets: true } : {}),
    ...(getBool(options, 'include-packets') ? { includePackets: true } : {}),
    ...(getBool(options, 'include-facts') ? { includeFacts: true } : {}),
    ...(getBool(options, 'include-edges') ? { includeEdges: true } : {}),
    ...numeric,
  };
}
