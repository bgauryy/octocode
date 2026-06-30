import { existsSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import type { CLICommand, ParsedArgs } from '../types.js';
import { getBool, getString, intFlag, isFlagError } from '../options.js';
import { c, dim } from '../../utils/colors.js';
import { EXIT, classifyToolErrorText } from '../exit-codes.js';
import { printCliError } from '../cli-error.js';
import { resolveRef, isGithubRef, cloneCommandFor } from '../routing.js';
import { outlineSymbols } from './symbol-outline.js';
import { render, renderRawContent } from './search-render.js';
import {
  formatMaterializationHints,
  materializeRemoteForCli,
  withMaterializationHints,
} from '../remote-local.js';
import {
  runOqlSearch,
  oqlSchemaText,
  oqlCompactSchemeText,
  oqlCompactSchemeJson,
  sanitizeStructuredContent,
  buildShorthandInput,
  type OqlRunResult,
  isBatchEnvelope,
} from '@octocodeai/octocode-tools-core/oql';

type CliShorthandCorpus =
  | { kind: 'local'; path: string }
  | { kind: 'github'; repo: string; path?: string; ref?: string }
  | { kind: 'npm' };

type CliSearchShorthand = Record<string, unknown> & {
  corpus: CliShorthandCorpus;
};

/**
 * `octocode search` — the universal OQL runner. Thin: read the query (full OQL
 * JSON or CLI shorthand), delegate to runOqlSearch in tools-core, render the
 * typed envelope. No OQL logic lives here (the brain owns it); shorthand only
 * builds the sugar object the core normalizer already accepts.
 */
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
  artifact: 'artifacts',
};
function normalizeTargetAlias(options: Record<string, unknown>): void {
  const raw = options['target'];
  if (typeof raw !== 'string') return;
  const canonical = TARGET_ALIASES[raw.trim().toLowerCase()];
  if (canonical) options['target'] = canonical;
}

export const searchCommand: CLICommand = {
  name: 'search',
  options: [
    { name: 'query', hasValue: true },
    { name: 'file', hasValue: true },
    { name: 'stdin' },
    { name: 'scheme' },
    { name: 'explain' },
    { name: 'dry-run' },
    { name: 'json' },
    { name: 'compact' },
    { name: 'raw' },
    { name: 'concise' },
    // shorthand sugar (normalized by core, shown canonical via --explain)
    { name: 'target', hasValue: true },
    { name: 'op', hasValue: true },
    { name: 'view', hasValue: true },
    { name: 'content-view', hasValue: true },
    { name: 'search', hasValue: true },
    { name: 'limit', hasValue: true },
    { name: 'page', hasValue: true },
    { name: 'items-per-page', hasValue: true },
    { name: 'pattern', hasValue: true },
    { name: 'rule', hasValue: true },
    { name: 'regex', hasValue: true },
    { name: 'pcre2' },
    { name: 'lang', hasValue: true },
    { name: 'repo', hasValue: true },
    { name: 'source', hasValue: true },
    { name: 'path', hasValue: true },
    { name: 'materialize', hasValue: true },
    { name: 'branch', hasValue: true },
    { name: 'force-refresh' },
    { name: 'include', hasValue: true },
    { name: 'exclude', hasValue: true },
    { name: 'ext', hasValue: true },
    { name: 'exclude-dir', hasValue: true },
    { name: 'name', hasValue: true },
    { name: 'filename', hasValue: true },
    { name: 'path-pattern', hasValue: true },
    { name: 'entry', hasValue: true },
    { name: 'min-depth', hasValue: true },
    { name: 'max-depth', hasValue: true },
    { name: 'empty' },
    { name: 'modified-within', hasValue: true },
    { name: 'modified-before', hasValue: true },
    { name: 'accessed-within', hasValue: true },
    { name: 'size-greater', hasValue: true },
    { name: 'size-less', hasValue: true },
    { name: 'permissions', hasValue: true },
    { name: 'executable' },
    { name: 'readable' },
    { name: 'writable' },
    { name: 'details' },
    { name: 'show-modified' },
    { name: 'hidden' },
    { name: 'no-ignore' },
    { name: 'case-insensitive' },
    { name: 'case-sensitive' },
    { name: 'whole-word' },
    { name: 'fixed' },
    { name: 'multiline' },
    { name: 'multiline-dotall' },
    { name: 'files-only' },
    { name: 'files-without-match' },
    { name: 'count-lines' },
    { name: 'count-matches' },
    { name: 'only-matching' },
    { name: 'unique' },
    { name: 'count' },
    { name: 'context-lines', hasValue: true },
    { name: 'context', hasValue: true },
    { name: 'invert-match' },
    { name: 'match-window', hasValue: true },
    { name: 'match-length', hasValue: true },
    { name: 'max-matches', hasValue: true },
    { name: 'match-page', hasValue: true },
    { name: 'max-files', hasValue: true },
    { name: 'sort', hasValue: true },
    { name: 'sort-reverse' },
    { name: 'ranking-profile', hasValue: true },
    { name: 'debug-ranking' },
    { name: 'match-string', hasValue: true },
    { name: 'match-regex' },
    { name: 'match-case-sensitive' },
    { name: 'start-line', hasValue: true },
    { name: 'end-line', hasValue: true },
    { name: 'char-offset', hasValue: true },
    { name: 'char-length', hasValue: true },
    { name: 'full-content' },
    { name: 'tree' },
    { name: 'include-sizes' },
    { name: 'symbols' },
    { name: 'kind', hasValue: true },
    { name: 'symbol', hasValue: true },
    { name: 'uri', hasValue: true },
    { name: 'line', hasValue: true },
    { name: 'order', hasValue: true },
    { name: 'depth', hasValue: true },
    { name: 'workspace-root', hasValue: true },
    { name: 'format', hasValue: true },
    { name: 'owner', hasValue: true },
    { name: 'topic', hasValue: true },
    { name: 'stars', hasValue: true },
    { name: 'forks', hasValue: true },
    { name: 'good-first-issues', hasValue: true },
    { name: 'license', hasValue: true },
    { name: 'created', hasValue: true },
    { name: 'updated', hasValue: true },
    { name: 'closed', hasValue: true },
    { name: 'merged-at', hasValue: true },
    { name: 'size', hasValue: true },
    { name: 'match', hasValue: true },
    { name: 'archived', hasValue: true },
    { name: 'visibility', hasValue: true },
    { name: 'state', hasValue: true },
    { name: 'author', hasValue: true },
    { name: 'label', hasValue: true },
    { name: 'pr', hasValue: true },
    { name: 'base', hasValue: true },
    { name: 'head', hasValue: true },
    { name: 'draft' },
    { name: 'comments' },
    { name: 'commits' },
    { name: 'deep' },
    { name: 'review-mode', hasValue: true },
    { name: 'file-page', hasValue: true },
    { name: 'comment-page', hasValue: true },
    { name: 'commit-page', hasValue: true },
    { name: 'since', hasValue: true },
    { name: 'until', hasValue: true },
    { name: 'patches' },
    { name: 'base-ref', hasValue: true },
    { name: 'head-ref', hasValue: true },
    { name: 'inspect' },
    { name: 'detailed' },
    { name: 'list' },
    { name: 'strings' },
    { name: 'extract', hasValue: true },
    { name: 'decompress' },
    { name: 'artifact-mode', hasValue: true },
    { name: 'min-length', hasValue: true },
    { name: 'max-entries', hasValue: true },
    { name: 'entry-page', hasValue: true },
    { name: 'scan-offset', hasValue: true },
    { name: 'offsets' },
    { name: 'verbose' },
    { name: 'archive-file', hasValue: true },
    { name: 'intent', hasValue: true },
    { name: 'facets', hasValue: true },
    { name: 'proof', hasValue: true },
    { name: 'proof-limit', hasValue: true },
    { name: 'include-packets' },
    { name: 'include-facts' },
    { name: 'include-edges' },
  ],
  handler: async (args): Promise<void> => {
    const { options } = args;

    // Accept friendly `--target` abbreviations (the ones the help/agent prompt
    // use, e.g. `repos`, `PRs`) and fold them to the canonical OQL enum so an
    // agent copying the help string doesn't hit "must be one of …".
    normalizeTargetAlias(options);

    // --scheme: print the OQL schema and exit. --scheme --compact prints the
    // lean agent guide (TEXT); --scheme --json --compact prints the same guide
    // as small machine-readable JSON. Plain --scheme --json remains full.
    if (getBool(options, 'scheme')) {
      const schemeText =
        getBool(options, 'json') && getBool(options, 'compact')
          ? oqlCompactSchemeJson()
          : getBool(options, 'compact')
            ? oqlCompactSchemeText()
            : oqlSchemaText();
      process.stdout.write(`${schemeText}\n`);
      return;
    }
    if (getBool(options, 'raw') && getBool(options, 'json')) {
      printCliError('Use either --raw or --json, not both.');
      process.exitCode = EXIT.USAGE;
      return;
    }
    if (await tryHandleSymbolOutline(args)) {
      return;
    }

    const resolved = resolveInput(args);
    if (resolved === undefined) {
      printCliError(
        'search needs a query. Shorthand: `search "<text>" [path|owner/repo]` (text/regex/AST), `search <dir|owner/repo> --tree`, `search <file|owner/repo/path>` (or --pattern/--rule/--regex with --lang). Full: --query <json> | --file <path> | --stdin. See `octocode search --scheme`.'
      );
      process.exitCode = EXIT.USAGE;
      return;
    }
    if ('error' in resolved) {
      printCliError(resolved.error);
      process.exitCode = EXIT.USAGE;
      return;
    }
    const input = resolved.input;

    const explain = getBool(options, 'explain');
    const dryRun = getBool(options, 'dry-run');

    // --explain sets explain on the query so the plan is included in the
    // envelope; --dry-run plans without executing.
    const withExplain =
      explain && input && typeof input === 'object'
        ? { ...(input as Record<string, unknown>), explain: true }
        : input;

    let result: OqlRunResult;
    try {
      result = await runOqlSearch(withExplain as never, { dryRun });
    } catch (err) {
      printCliError(`OQL execution failed: ${(err as Error).message}`);
      process.exitCode = EXIT.TOOL;
      return;
    }

    // Redact secrets before ANY output path (json / raw / rendered). OQL
    // adapters return raw rows and rely on the interface layer to sanitize; the
    // MCP path does this via sanitizeCallToolResult, so the CLI must too — else
    // code snippets can leak tokens/keys to stdout.
    result = sanitizeStructuredContent(result) as OqlRunResult;

    if (getBool(options, 'json')) {
      // --compact emits single-line minified JSON (stream/parse friendly);
      // plain --json stays pretty-printed for human reading. Both are valid
      // JSON with control characters escaped.
      const json = getBool(options, 'compact')
        ? JSON.stringify(result)
        : JSON.stringify(result, null, 2);
      process.stdout.write(`${json}\n`);
    } else if (getBool(options, 'raw')) {
      const raw = renderRawContent(result);
      if (raw === undefined) {
        printCliError('--raw is only available for content result rows.');
        process.exitCode = EXIT.USAGE;
        return;
      }
      process.stdout.write(raw.endsWith('\n') ? raw : `${raw}\n`);
    } else {
      process.stdout.write(`${render(result, getBool(options, 'compact'))}\n`);
    }

    process.exitCode = exitCodeFor(result);
  },
};

type Resolved = { input: unknown } | { error: string } | undefined;

async function tryHandleSymbolOutline(args: ParsedArgs): Promise<boolean> {
  const { options } = args;
  if (!getBool(options, 'symbols')) return false;

  const positionals = args.args.filter(a => !a.startsWith('-'));
  const target = positionals[0] ?? '';
  const repoOption = getString(options, 'repo') || undefined;
  const branchOverride = getString(options, 'branch') || undefined;

  if (repoOption) {
    try {
      const materialized = await materializeRemoteForCli({
        repoRef: repoOption,
        path: target || undefined,
        branch: branchOverride,
        forceRefresh: getBool(options, 'force-refresh') || undefined,
        kind: target ? 'file' : 'repo',
      });
      if (!getBool(options, 'json')) {
        process.stderr.write(
          `  ${dim(`Outlining ${materialized.localPath} ...`)}\n`
        );
      }
      await outlineSymbols(materialized.localPath, options, {
        structured: withMaterializationHints(
          { structuredContent: {} },
          materialized
        ).structuredContent as Record<string, unknown>,
        text: formatMaterializationHints(materialized),
      });
    } catch (error) {
      printCliError(
        `Remote materialization failed: ${
          error instanceof Error ? error.message : String(error)
        }`
      );
      process.exitCode = EXIT.TOOL;
    }
    return true;
  }

  if (!target) {
    printCliError('search --symbols needs a local path or --repo target.');
    process.exitCode = EXIT.USAGE;
    return true;
  }

  const ref = resolveRef(target, branchOverride);
  if (isGithubRef(ref)) {
    printCliError(
      '--symbols is local-only — an LSP outline cannot run on GitHub. ' +
        `Clone first: \`${cloneCommandFor(ref)}\`, then \`search <local-path> --symbols\`, ` +
        'or use `search <path> --repo <owner/repo> --symbols`.'
    );
    process.exitCode = EXIT.USAGE;
    return true;
  }

  await outlineSymbols(target, options);
  return true;
}

/** Resolve the OQL input from full JSON sources or CLI shorthand sugar. */
function resolveInput(args: ParsedArgs): Resolved {
  // 1. Explicit JSON sources.
  const jsonText = readJsonText(args);
  if (jsonText && 'text' in jsonText) {
    try {
      return { input: parseOqlQueryJson(jsonText.text) };
    } catch (err) {
      return {
        error: `Could not parse OQL query JSON: ${(err as Error).message}`,
      };
    }
  }
  if (jsonText && 'error' in jsonText) return { error: jsonText.error };

  // 2. Shorthand sugar -> the sugar object the core normalizer accepts.
  return buildSugar(args);
}

function parseOqlQueryJson(text: string): unknown {
  const parsed = JSON.parse(text) as unknown;
  return Array.isArray(parsed) ? { schema: 'oql', queries: parsed } : parsed;
}

function readJsonText(
  args: ParsedArgs
): { text: string } | { error: string } | undefined {
  const { options } = args;
  const query = getString(options, 'query');
  if (query && !isPullRequestTextQuery(args, query)) return { text: query };
  const file = getString(options, 'file');
  if (file && !isPullRequestPatchPath(args, file)) {
    try {
      return { text: readFileSync(file, 'utf8') };
    } catch (err) {
      return {
        error: `Could not read --file ${file}: ${(err as Error).message}`,
      };
    }
  }
  if (getBool(options, 'stdin')) {
    try {
      return { text: readFileSync(0, 'utf8') };
    } catch {
      return { error: 'Could not read OQL query from stdin.' };
    }
  }
  // bare positional JSON, e.g. `search '{...}'`
  const first = args.args[0];
  if (first && looksLikeJsonText(first)) return { text: first };
  return undefined;
}

function looksLikeJsonText(value: string): boolean {
  const trimmed = value.trim();
  return trimmed.startsWith('{') || trimmed.startsWith('[');
}

function looksLikeJsonFile(value: string): boolean {
  return /\.(?:json|jsonc)$/i.test(value) || /\.oql$/i.test(value);
}

function isPullRequestShorthand(args: ParsedArgs): boolean {
  const target = getString(args.options, 'target');
  if (target === 'pullRequests') return true;
  if (hasPullRequestIntent(args.options)) return true;
  const targetArg = args.args.find(arg => !arg.startsWith('-'));
  return Boolean(parsePullRequestRef(targetArg, getString(args.options, 'pr')));
}

function isPullRequestTextQuery(args: ParsedArgs, value: string): boolean {
  return !looksLikeJsonText(value) && isPullRequestShorthand(args);
}

function pullRequestTextQuery(args: ParsedArgs): string | undefined {
  const value = getString(args.options, 'query');
  return value && isPullRequestTextQuery(args, value) ? value : undefined;
}

function isPullRequestPatchPath(args: ParsedArgs, value: string): boolean {
  return isPullRequestShorthand(args) && !looksLikeJsonFile(value);
}

function pullRequestPatchPath(args: ParsedArgs): string | undefined {
  const value = getString(args.options, 'file');
  return value && isPullRequestPatchPath(args, value) ? value : undefined;
}

interface GithubDiffShortcut {
  corpus: CliShorthandCorpus;
  baseRef: string;
  headRef: string;
  path: string;
}

function resolveGithubDiffShortcut(
  positionals: readonly string[],
  options: ParsedArgs['options'],
  explicitTarget: string | undefined,
  fromFlag: boolean,
  repoOption: string | undefined
): GithubDiffShortcut | undefined {
  if (
    explicitTarget !== 'diff' ||
    fromFlag ||
    repoOption ||
    positionals.length < 2
  ) {
    return undefined;
  }

  const branchOverride = getString(options, 'branch') || undefined;
  const base = resolveRef(positionals[0]!, branchOverride);
  const head = resolveRef(positionals[1]!, branchOverride);
  if (!isGithubRef(base) || !isGithubRef(head)) return undefined;
  if (base.owner !== head.owner || base.repo !== head.repo) return undefined;

  const basePath = normalizeRepoPath(base.subpath);
  const headPath = normalizeRepoPath(head.subpath);
  if (!basePath || basePath !== headPath) return undefined;

  return {
    corpus: {
      kind: 'github',
      repo: `${base.owner}/${base.repo}`,
      path: basePath,
      ...(base.branch ? { ref: base.branch } : {}),
    },
    baseRef:
      getString(options, 'base-ref') ||
      getString(options, 'base') ||
      base.branch ||
      '',
    headRef:
      getString(options, 'head-ref') ||
      getString(options, 'head') ||
      head.branch ||
      '',
    path: basePath,
  };
}

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
function emitSearchInputWarnings(o: {
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

function buildSugar(args: ParsedArgs): Resolved {
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

function validateShorthandOptions(
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
  if (contentView && !['exact', 'compact', 'symbols'].includes(contentView)) {
    return '--content-view must be exact, compact, or symbols.';
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

function parseNumericOptions(
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
    ['min-length', 'minLength', 1],
    ['max-entries', 'maxEntries', 1],
    ['entry-page', 'entryPageNumber', 1],
    ['scan-offset', 'scanOffset', 0],
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

function isParseError(value: unknown): value is { error: string } {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { error?: unknown }).error === 'string'
  );
}

function isSinglePositionalTarget(
  args: ParsedArgs,
  fromFlag: boolean
): boolean {
  if (fromFlag || args.args.length !== 1) return false;
  const first = args.args[0];
  const target = getString(args.options, 'target');
  if (getString(args.options, 'repo')) {
    return (
      Boolean(target && target !== 'code') || hasTargetIntent(args.options)
    );
  }
  if (
    (!target || target === 'pullRequests' || target === 'diff') &&
    parsePullRequestRef(first, getString(args.options, 'pr'))
  ) {
    return true;
  }
  if (!target && !hasTargetIntent(args.options) && isLocalFileTarget(first)) {
    return true;
  }
  if (
    !target &&
    !hasTargetIntent(args.options) &&
    singlePositionalCorpusTarget(first)
  ) {
    return true;
  }
  if (!target && !hasTargetIntent(args.options)) return false;
  if (target === 'packages' || target === 'repositories') return false;
  // A file-like positional with content intent (e.g. --content-view, line
  // range, --match-string) is the read TARGET even if it doesn't exist yet —
  // otherwise it falls through to a text search with no corpus and resolves to
  // cwd ".", yielding a misleading "Path is a directory" instead of a clean
  // "File not found".
  if (hasContentIntent(args.options) && looksLikeFilePath(first ?? '')) {
    return true;
  }
  return isCorpusLike(first) || target === 'content' || target === 'structure';
}

function hasTargetIntent(options: ParsedArgs['options']): boolean {
  return Boolean(
    hasPullRequestIntent(options) ||
    getString(options, 'op') ||
    getBool(options, 'symbols') ||
    getBool(options, 'tree') ||
    getBool(options, 'inspect', 'list', 'strings', 'decompress') ||
    getBool(options, 'detailed') ||
    getString(options, 'extract') ||
    getString(options, 'artifact-mode') ||
    getString(options, 'content-view') ||
    getString(options, 'match-string') ||
    getString(options, 'start-line') ||
    getString(options, 'end-line') ||
    getString(options, 'char-offset') ||
    getString(options, 'char-length') ||
    getBool(options, 'full-content') ||
    getBool(options, 'raw') ||
    getString(options, 'uri') ||
    getString(options, 'symbol') ||
    getString(options, 'workspace-root')
  );
}

function inferTarget(
  args: ParsedArgs,
  targetArg: string | undefined,
  hints: { hasSearchPredicate?: boolean } = {}
): string | undefined {
  const { options } = args;
  if (
    parsePullRequestRef(targetArg, getString(options, 'pr')) ||
    hasPullRequestIntent(options)
  ) {
    return 'pullRequests';
  }
  if (
    getString(options, 'op') ||
    getString(options, 'symbol') ||
    getBool(options, 'symbols')
  )
    return 'semantics';
  if (getBool(options, 'tree')) return 'structure';
  if (
    getBool(options, 'inspect', 'list', 'strings', 'decompress') ||
    getString(options, 'extract') ||
    getString(options, 'artifact-mode') ||
    getBool(options, 'detailed')
  ) {
    return 'artifacts';
  }
  if (
    hasContentIntent(options) ||
    (isLocalFileTarget(targetArg) && !hints.hasSearchPredicate)
  )
    return 'content';
  // A lone path/ref positional (local dir, owner/repo, owner/repo/file) browses
  // or reads rather than text-searches: dir/repo -> structure, file -> content.
  if (!hints.hasSearchPredicate) {
    const corpusTarget = singlePositionalCorpusTarget(targetArg);
    if (corpusTarget) return corpusTarget;
  }
  if (
    getString(options, 'search') === 'path' ||
    getString(options, 'entry') ||
    getString(options, 'ext') ||
    getString(options, 'name', 'filename') ||
    getString(options, 'path-pattern')
  ) {
    return 'files';
  }
  return undefined;
}

function hasContentIntent(options: ParsedArgs['options']): boolean {
  return Boolean(
    getBool(options, 'raw', 'full-content') ||
    getString(options, 'match-string') ||
    getString(options, 'start-line') ||
    getString(options, 'end-line') ||
    getString(options, 'char-offset') ||
    getString(options, 'char-length') ||
    ['exact', 'compact', 'symbols'].includes(getString(options, 'content-view'))
  );
}

function hasPullRequestIntent(options: ParsedArgs['options']): boolean {
  return Boolean(
    getString(options, 'pr') ||
    getString(options, 'state') ||
    getString(options, 'label') ||
    getString(options, 'base') ||
    getString(options, 'head') ||
    getString(options, 'draft') ||
    getBool(options, 'draft') ||
    getBool(options, 'comments', 'commits', 'deep')
  );
}

function isCorpusLike(value: string | undefined): boolean {
  if (!value) return false;
  const ref = resolveRef(value);
  if (isGithubRef(ref)) return true;
  try {
    return existsSync(ref.path);
  } catch {
    return false;
  }
}

function isLocalFileTarget(value: string | undefined): boolean {
  if (!value) return false;
  const ref = resolveRef(value);
  if (isGithubRef(ref)) return false;
  try {
    return existsSync(ref.path) && statSync(ref.path).isFile();
  } catch {
    return false;
  }
}

/** A path segment with a file extension (last segment has a `.ext`). */
function looksLikeFilePath(subpath: string): boolean {
  const base = subpath.split('/').pop() ?? '';
  return /\.[A-Za-z0-9]+$/.test(base);
}

/**
 * A lone positional that is a path/ref (not a search term) routes to a browse/
 * read target instead of a text search — the terse forms the quick commands
 * expose:
 *   existing local directory   -> structure
 *   owner/repo[/dir]           -> structure
 *   owner/repo/file.ext        -> content
 * Local files are handled by isLocalFileTarget -> content. Returns the OQL
 * target, or undefined when the value should be treated as a search term.
 */
function singlePositionalCorpusTarget(
  value: string | undefined
): string | undefined {
  if (!value) return undefined;
  const ref = resolveRef(value);
  if (isGithubRef(ref)) {
    return ref.subpath && looksLikeFilePath(ref.subpath)
      ? 'content'
      : 'structure';
  }
  try {
    if (existsSync(ref.path) && statSync(ref.path).isDirectory()) {
      return 'structure';
    }
  } catch {
    /* not an existing local directory */
  }
  return undefined;
}

function listOption(value: string): string[] {
  return value
    .split(',')
    .map(v => v.trim())
    .filter(Boolean);
}

function parseBooleanString(value: string | undefined): boolean | undefined {
  if (!value) return undefined;
  const normalized = value.trim().toLowerCase();
  if (['true', '1', 'yes', 'y'].includes(normalized)) return true;
  if (['false', '0', 'no', 'n'].includes(normalized)) return false;
  return undefined;
}

function normalizeEntryType(
  value: string | undefined
): 'file' | 'directory' | undefined {
  if (!value) return undefined;
  if (value === 'file' || value === 'f') return 'file';
  if (value === 'directory' || value === 'd') return 'directory';
  return undefined;
}

function resolveSemanticsOp(
  options: ParsedArgs['options']
): string | undefined {
  return (
    getString(options, 'op') ||
    (getBool(options, 'symbols') ? 'documentSymbols' : undefined)
  );
}

function resolveArtifactMode(
  options: ParsedArgs['options']
): string | undefined {
  if (getString(options, 'artifact-mode'))
    return getString(options, 'artifact-mode');
  if (getBool(options, 'inspect')) return 'inspect';
  if (getBool(options, 'list')) return 'list';
  if (getBool(options, 'strings')) return 'strings';
  if (getBool(options, 'decompress')) return 'decompress';
  if (getString(options, 'extract')) return 'extract';
  return undefined;
}

/** Resolve a target string to a corpus (local path vs GitHub ref). FS-aware. */
function resolveCorpus(
  target: string | undefined,
  oqlTarget?: string,
  repoOption?: string,
  ownerOption?: string,
  sourceOverride?: string,
  pathOverride?: string
): CliShorthandCorpus {
  if (sourceOverride === 'npm') return { kind: 'npm' };
  if (!target && oqlTarget === 'packages') return { kind: 'npm' };
  if (target === 'npm') return { kind: 'npm' };
  if (sourceOverride === 'local') {
    return {
      kind: 'local',
      path: pathOverride
        ? path.resolve(pathOverride)
        : target
          ? path.resolve(target)
          : '.',
    };
  }
  if (ownerOption && repoOption && !repoOption.includes('/')) {
    const repoPath = normalizeRepoPath(pathOverride ?? target);
    return {
      kind: 'github',
      repo: `${ownerOption}/${repoOption}`,
      ...(repoPath ? { path: repoPath } : {}),
    };
  }
  if (repoOption) {
    const repo = resolveRef(repoOption);
    if (isGithubRef(repo)) {
      const repoPath = normalizeRepoPath(repo.subpath, pathOverride ?? target);
      return {
        kind: 'github',
        repo: `${repo.owner}/${repo.repo}`,
        ...(repoPath ? { path: repoPath } : {}),
        ...(repo.branch ? { ref: repo.branch } : {}),
      };
    }
  }
  if (sourceOverride === 'github' && target) {
    const ref = resolveRef(target);
    if (isGithubRef(ref)) {
      const repoPath = normalizeRepoPath(pathOverride ?? ref.subpath);
      return {
        kind: 'github',
        repo: `${ref.owner}/${ref.repo}`,
        ...(repoPath ? { path: repoPath } : {}),
        ...(ref.branch ? { ref: ref.branch } : {}),
      };
    }
  }
  if (!target) {
    return {
      kind: 'local',
      path: pathOverride ? path.resolve(pathOverride) : '.',
    };
  }
  const ref = resolveRef(target);
  if (isGithubRef(ref)) {
    const repoPath = normalizeRepoPath(pathOverride ?? ref.subpath);
    return {
      kind: 'github',
      repo: `${ref.owner}/${ref.repo}`,
      ...(repoPath ? { path: repoPath } : {}),
      ...(ref.branch ? { ref: ref.branch } : {}),
    };
  }
  return {
    kind: 'local',
    path: pathOverride ? path.resolve(pathOverride) : ref.path,
  };
}

function normalizeRepoPath(...parts: readonly (string | undefined)[]): string {
  const joined = parts
    .map(part => part?.trim().replace(/^\/+|\/+$/g, ''))
    .filter(Boolean)
    .join('/');
  if (!joined) return '';
  const normalized = path.posix.normalize(joined);
  if (
    normalized === '..' ||
    normalized.startsWith('../') ||
    normalized.includes('/../')
  ) {
    throw new Error('Remote path cannot contain path traversal segments.');
  }
  return normalized === '.' ? '' : normalized;
}

function parsePullRequestRef(
  input: string | undefined,
  prOverride?: string | undefined
): { owner: string; repo: string; prNumber?: number } | undefined {
  if (!input) return undefined;
  const urlMatch = input.match(/github\.com\/([^/]+)\/([^/]+)\/pull\/(\d+)/);
  if (urlMatch) {
    return {
      owner: urlMatch[1]!,
      repo: urlMatch[2]!,
      prNumber: Number.parseInt(urlMatch[3]!, 10),
    };
  }
  const hashMatch = input.match(/^([^/]+)\/([^#/]+)#(\d+)$/);
  if (hashMatch) {
    return {
      owner: hashMatch[1]!,
      repo: hashMatch[2]!,
      prNumber: Number.parseInt(hashMatch[3]!, 10),
    };
  }
  const repoMatch = input.match(/^([^/]+)\/([^/]+)$/);
  if (!repoMatch) return undefined;
  if (!prOverride) return undefined;
  const prNumber =
    prOverride && /^\d+$/.test(prOverride)
      ? Number.parseInt(prOverride, 10)
      : undefined;
  return {
    owner: repoMatch[1]!,
    repo: repoMatch[2]!,
    ...(prNumber !== undefined ? { prNumber } : {}),
  };
}

function exitCodeFor(result: OqlRunResult): number {
  const envelopes = isBatchEnvelope(result)
    ? result.children.map(c => c.envelope)
    : [result];
  for (const env of envelopes) {
    if (env.diagnostics.some(d => d.code === 'rateLimited'))
      return EXIT.RATE_LIMIT;
    // Classify the error-bearing diagnostics by message text the same way the
    // direct-tool path does, so genuine not-found (3) / auth (4) / rate-limit
    // (7) failures are reachable through `search`. A diagnostic that carries no
    // such signal classifies as TOOL — for an `invalidQuery` that means a truly
    // malformed query, which stays USAGE (2).
    const invalidQuery = env.diagnostics.find(d => d.code === 'invalidQuery');
    if (invalidQuery) {
      const classified = classifyToolErrorText(invalidQuery.message);
      return classified === EXIT.TOOL ? EXIT.USAGE : classified;
    }
    if (env.evidence.kind === 'unsupported') return EXIT.TOOL;
  }
  return EXIT.OK;
}
