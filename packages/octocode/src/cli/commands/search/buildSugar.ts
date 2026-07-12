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
} from './inputParsing.js';
import {
  resolveCorpus,
  normalizeEntryType,
  resolveSemanticsOp,
} from './corpusResolution.js';
import { buildShorthandParts } from './shorthandParts.js';
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
  const contentView = getString(options, 'content-view') || undefined;
  const view =
    getString(options, 'view') ||
    (getBool(options, 'concise') && target !== 'repositories'
      ? 'discovery'
      : undefined);
  const op = resolveSemanticsOp(options);
  const parts: CliSearchShorthand = buildShorthandParts({
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
  });

  const result = buildShorthandInput(parts as never);
  return 'error' in result ? { error: result.error } : { input: result.input };
}
