import path from 'node:path';
import type { ParsedArgs } from '../../types.js';
import { getBool, getString } from '../../options.js';
import { buildShorthandInput } from '@octocodeai/octocode-tools-core/oql';
import {
  pullRequestTextQuery,
  pullRequestPatchPath,
  resolveGithubDiffShortcut,
  parsePullRequestRef,
} from './prShorthand.js';
import {
  isSinglePositionalTarget,
  hasTargetIntent,
  inferTarget,
} from './targetInference.js';
import {
  emitSearchInputWarnings,
  validateShorthandOptions,
  parseNumericOptions,
  isParseError,
  listOption,
  parseBooleanString,
} from './inputParsing.js';
import {
  resolveCorpus,
  normalizeEntryType,
  resolveArtifactMode,
  resolveSemanticsOp,
} from './corpusResolution.js';
import type { CliSearchShorthand, Resolved } from './types.js';

/**
 * Read shorthand from argv and delegate the lowering to tools-core's
 * `buildShorthandInput`. The CLI owns ONLY argv parsing and resolving the
 * target string to a corpus (the filesystem-dependent step); predicate
 * selection / dialect / assembly live in the brain.
 *   search "<term>" [path|owner/repo]            -> text
 *   search --regex '<re>' [target] [--pcre2]     -> regex
 *   search --pattern '<shape>' [target] --lang t -> structural pattern
 *   search --rule '<json|yaml>' [target] --lang t -> structural rule
 */
export function buildSugar(args: ParsedArgs): Resolved {
  const { options } = args;
  const positionals = args.args.filter(a => !a.startsWith('-'));
  const pullRequestQueryText = pullRequestTextQuery(args);
  const pullRequestFilePatch = pullRequestPatchPath(args);
  const pattern = getString(options, 'pattern') || undefined;
  const ruleText = getString(options, 'rule') || undefined;
  const regex = getString(options, 'regex') || undefined;

  const validation = validateShorthandOptions(args, {
    pattern,
    ruleText,
    regex,
  });
  if (validation) return { error: validation };

  // When the predicate comes from a flag the positional is the TARGET; for a
  // bare text term positional[0] is the term and [1] the target.
  const fromFlag = Boolean(pattern || ruleText || regex);
  const explicitTarget = getString(options, 'target') || undefined;
  const repoOption = getString(options, 'repo') || undefined;
  const ownerOption = getString(options, 'owner') || undefined;
  const pathOption = getString(options, 'path') || undefined;
  const githubDiff = resolveGithubDiffShortcut(
    positionals,
    options,
    explicitTarget,
    fromFlag,
    repoOption
  );
  // Two LOCAL files (no GitHub refs) -> local file-vs-file diff. The head file
  // must be ABSOLUTE: it flows to params.path -> localGetFileContent, which
  // rejects relative basenames as "outside allowed directories". The base file
  // (corpus.path) is already absolutized by resolveCorpus/resolveRef.
  const localDiffPath =
    explicitTarget === 'diff' &&
    !githubDiff &&
    !fromFlag &&
    !repoOption &&
    positionals.length >= 2
      ? path.resolve(positionals[1]!)
      : undefined;
  const diffPath = githubDiff?.path ?? localDiffPath;
  const targetOnly = diffPath
    ? false
    : isSinglePositionalTarget(args, fromFlag);
  const text =
    pullRequestQueryText ??
    (fromFlag || targetOnly || diffPath ? undefined : positionals[0]);
  const positionalTargetArg = githubDiff
    ? positionals[0]
    : diffPath
      ? positionals[0]
      : positionals[fromFlag || targetOnly ? 0 : 1];
  const targetArg = positionalTargetArg ?? pathOption;

  // Surface otherwise-silent input mistakes instead of quietly ignoring them.
  emitSearchInputWarnings({
    positionals,
    text,
    fromFlag,
    targetOnly,
    hasDiff: Boolean(diffPath),
    explicitTarget,
    positionalTargetArg,
  });

  if (
    !fromFlag &&
    !text &&
    !targetOnly &&
    !diffPath &&
    !hasTargetIntent(options)
  )
    return undefined; // nothing to search for

  let rule: unknown;
  if (ruleText !== undefined) {
    const trimmedRule = ruleText.trim();
    if (trimmedRule.startsWith('{') || trimmedRule.startsWith('[')) {
      try {
        rule = JSON.parse(ruleText);
      } catch (err) {
        return { error: `--rule JSON is invalid: ${(err as Error).message}` };
      }
    } else {
      rule = ruleText;
    }
  }

  const hasSearchPredicate = Boolean(text || pattern || ruleText || regex);
  const target =
    getString(options, 'target') ||
    inferTarget(args, targetArg, { hasSearchPredicate });
  const materialize = getString(options, 'materialize') || undefined;

  const numeric = parseNumericOptions(options);
  if (isParseError(numeric)) return numeric;

  const prTarget =
    target === 'pullRequests' || target === 'diff'
      ? parsePullRequestRef(repoOption ?? targetArg, getString(options, 'pr'))
      : undefined;
  const corpus = prTarget
    ? ({
        kind: 'github',
        repo: `${prTarget.owner}/${prTarget.repo}`,
      } as const)
    : resolveCorpus(
        targetArg,
        target,
        repoOption,
        ownerOption,
        getString(options, 'source'),
        pathOption
      );
  const resolvedCorpus = githubDiff?.corpus ?? corpus;

  const entry = normalizeEntryType(getString(options, 'entry'));
  const artifactMode = resolveArtifactMode(options);
  const contentView = getString(options, 'content-view') || undefined;
  const view =
    getString(options, 'view') ||
    (getBool(options, 'concise') && target !== 'repositories'
      ? 'discovery'
      : undefined);
  const op = resolveSemanticsOp(options);
  const parts: CliSearchShorthand = {
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
    ...(artifactMode ? { artifactMode } : {}),
    ...(getBool(options, 'detailed') ? { detailed: true } : {}),
    ...(getBool(options, 'verbose') ? { verbose: true } : {}),
    ...(getString(options, 'match') && target === 'artifacts'
      ? { matchString: getString(options, 'match') }
      : {}),
    ...(getString(options, 'extract')
      ? { archiveFile: getString(options, 'extract'), artifactMode: 'extract' }
      : {}),
    ...(getString(options, 'archive-file')
      ? { archiveFile: getString(options, 'archive-file') }
      : {}),
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

  const result = buildShorthandInput(parts as never);
  return 'error' in result ? { error: result.error } : { input: result.input };
}
