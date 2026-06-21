import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import {
  resolveRef,
  isGithubRef,
  refLabel,
  cloneCommandFor,
} from '../routing.js';
import { c, bold, dim } from '../../utils/colors.js';
import { EXIT, classifyToolErrorText } from '../exit-codes.js';
import { printCliError } from '../cli-error.js';
import { formatGithubFailure } from '../github-error.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import { getConfigSync } from '@octocodeai/octocode-tools-core/config';
import {
  renderLocalResults,
  type LocalSearchResult,
} from './local-search-render.js';
import {
  formatMaterializationHints,
  materializeRemoteForCli,
  withMaterializationHints,
} from '../remote-local.js';

interface GithubPagination {
  totalMatches?: number;
  uniqueFileCount?: number;
  currentPage?: number;
  totalPages?: number;
}

interface GithubFile {
  owner?: string;
  repo?: string;
  path?: string;
  matchCount?: number;
  matches?: Array<{ value?: string; line?: number }>;
}

// `--mode` selects a TEXT search mode. Structural (AST) search is a separate
// axis, triggered by --pattern/--rule (routes to localSearchCode mode
// "structural"), not by a --mode value.
const GREP_MODES = new Set(['paginated', 'discovery', 'detailed']);

interface GithubCodeResult {
  // ghSearchCode nests hits under results[].data.files[] (same shape as the
  // local renderer), not results[].matches — render off that. With concise:true
  // each file is a flat "owner/repo:path" string instead of an object.
  results?: Array<{
    data?: {
      files?: Array<string | GithubFile>;
      pagination?: GithubPagination;
      nonExistentScope?: boolean;
    };
  }>;
  hints?: string[];
  emptyQueries?: Array<{
    id?: string;
    hints?: string[];
    nonExistentScope?: true;
  }>;
}

type GithubRenderRef = {
  readonly owner?: string;
  readonly repo?: string;
  readonly subpath?: string;
};

function listOption(value: string | undefined): string[] | undefined {
  const items = value
    ?.split(',')
    .map(item => item.trim())
    .filter(Boolean);
  return items && items.length > 0 ? items : undefined;
}

function structuralShellExpansionError(shape: string): string | undefined {
  // Strip valid ast-grep metavariables before sniffing for shell-expansion
  // artifacts. Otherwise the `\$\s` check below matches the trailing `$` of an
  // unnamed `$$$` ellipsis when whitespace follows it (e.g. `{ $$$ }`), falsely
  // rejecting a correctly single-quoted pattern. `$1` (a shell positional),
  // `$(...)` (command substitution), and a lone dangling `$ ` survive the strip
  // and are still flagged.
  const withoutMetavars = shape
    .replace(/\$\$\$[A-Za-z_]\w*/g, '') // $$$NAME — named multi
    .replace(/\$\$\$/g, '') // $$$ — anonymous multi
    .replace(/\$[A-Za-z_]\w*/g, ''); // $NAME — named single
  if (/\$\d|\$\(|\$\s/.test(withoutMetavars)) {
    return "Structural pattern looks shell-expanded. Wrap metavariables in single quotes, e.g. --pattern 'useEffect($$$ARGS)'.";
  }
  if (!shape.includes('$') && /[A-Za-z_][\w.]*\([^)]*\b\d{5,}\b/.test(shape)) {
    return "Structural pattern contains a large bare number where a metavariable list often belongs. If you meant $$$ARGS, wrap the pattern in single quotes: --pattern 'fn($$$ARGS)'.";
  }
  return undefined;
}

const LOCAL_TYPE_GLOBS: Record<string, string[]> = {
  javascript: ['*.js', '*.jsx', '*.mjs', '*.cjs'],
  typescript: ['*.ts', '*.tsx', '*.mts', '*.cts'],
  python: ['*.py', '*.pyw'],
  rust: ['*.rs'],
  ruby: ['*.rb'],
  shell: ['*.sh', '*.bash', '*.zsh'],
  bash: ['*.sh', '*.bash'],
  markdown: ['*.md', '*.mdx'],
  yaml: ['*.yml', '*.yaml'],
};

function localIncludeGlobs(
  include: string[] | undefined,
  typeFilter: string | undefined
): string[] | undefined {
  const filters = [...(include ?? [])];
  const ext = typeFilter?.trim().replace(/^\./, '');
  if (ext) {
    const mapped = LOCAL_TYPE_GLOBS[ext.toLowerCase()];
    filters.push(...(mapped ?? [ext.includes('*') ? ext : `*.${ext}`]));
  }
  return filters.length > 0 ? filters : undefined;
}

function repoTargetKind(target: string): 'file' | 'tree' | 'repo' {
  if (!target || target === '.') return 'repo';
  return target.split('/').pop()?.includes('.') ? 'file' : 'tree';
}

interface LocalSearchOpts {
  mode?: string;
  include?: string[];
  exclude?: string[];
  contextLines?: number;
  maxMatchesPerFile?: number;
  page?: number;
  pageSize?: number;
  fixedString?: boolean;
  perlRegex?: boolean;
  caseInsensitive?: boolean;
  caseSensitive?: boolean;
  wholeWord?: boolean;
  invertMatch?: boolean;
  hidden?: boolean;
  noIgnore?: boolean;
  filesOnly?: boolean;
  filesWithoutMatch?: boolean;
  countLinesPerFile?: boolean;
  countMatchesPerFile?: boolean;
  multiline?: boolean;
  multilineDotall?: boolean;
  matchContentLength?: number;
  maxFiles?: number;
  matchPage?: number;
  onlyMatching?: boolean;
  unique?: boolean;
  countUnique?: boolean;
  matchWindow?: number;
}

async function searchLocal(
  keywords: string,
  dirPath: string,
  opts: LocalSearchOpts = {}
): Promise<LocalSearchResult> {
  const result = await executeDirectTool('localSearchCode', {
    queries: [
      {
        keywords,
        path: dirPath,
        mode: opts.mode as 'paginated' | 'discovery' | 'detailed' | undefined,
        include: opts.include,
        exclude: opts.exclude,
        contextLines: opts.contextLines,
        maxMatchesPerFile: opts.maxMatchesPerFile,
        fixedString: opts.fixedString,
        perlRegex: opts.perlRegex,
        caseInsensitive: opts.caseInsensitive,
        caseSensitive: opts.caseSensitive,
        wholeWord: opts.wholeWord,
        invertMatch: opts.invertMatch,
        hidden: opts.hidden,
        noIgnore: opts.noIgnore,
        filesOnly: opts.filesOnly,
        filesWithoutMatch: opts.filesWithoutMatch,
        countLinesPerFile: opts.countLinesPerFile,
        countMatchesPerFile: opts.countMatchesPerFile,
        multiline: opts.multiline,
        multilineDotall: opts.multilineDotall,
        matchContentLength: opts.matchContentLength,
        maxFiles: opts.maxFiles,
        matchPage: opts.matchPage,
        onlyMatching: opts.onlyMatching,
        unique: opts.unique,
        countUnique: opts.countUnique,
        matchWindow: opts.matchWindow,
        page: opts.page,
        itemsPerPage: opts.pageSize,
        mainResearchGoal: 'Search local codebase',
        researchGoal: `Find "${keywords}" in ${dirPath}`,
        reasoning: 'CLI grep command',
      },
    ],
  });

  if (result.isError) {
    const errText =
      result.content[0]?.type === 'text' ? result.content[0].text : '';
    throw new Error(`Search error: ${errText}`);
  }

  return result.structuredContent as LocalSearchResult;
}

// Structural (AST) search: grep --pattern/--rule routes here, to localSearchCode
// mode:"structural". Local-only because it parses files with the native
// Octocode engine. Shares the same executor and renderer as text grep; only the
// query shape differs.
async function searchLocalStructural(
  dirPath: string,
  opts: {
    pattern?: string;
    rule?: string;
    include?: string[];
    contextLines?: number;
    maxMatchesPerFile?: number;
    page?: number;
    pageSize?: number;
  }
): Promise<LocalSearchResult> {
  const shape = opts.pattern ?? opts.rule ?? '';
  const result = await executeDirectTool('localSearchCode', {
    queries: [
      {
        path: dirPath,
        mode: 'structural' as const,
        ...(opts.pattern ? { pattern: opts.pattern } : { rule: opts.rule }),
        include: opts.include,
        contextLines: opts.contextLines,
        maxMatchesPerFile: opts.maxMatchesPerFile,
        page: opts.page,
        itemsPerPage: opts.pageSize,
        mainResearchGoal: 'Search local codebase by AST shape',
        researchGoal: `Find AST shape "${shape}" in ${dirPath}`,
        reasoning: 'CLI grep --pattern/--rule (structural)',
      },
    ],
  });

  if (result.isError) {
    const errText =
      result.content[0]?.type === 'text' ? result.content[0].text : '';
    throw new Error(`Search error: ${errText}`);
  }

  return result.structuredContent as LocalSearchResult;
}

async function searchGithub(
  pattern: string,
  owner: string,
  repo: string,
  typeFilter?: string,
  subpath?: string,
  page?: number,
  pageSize?: number,
  concise?: boolean
): Promise<GithubCodeResult> {
  const result = await executeDirectTool('ghSearchCode', {
    queries: [
      {
        keywords: [pattern],
        owner,
        repo,
        extension: typeFilter,
        path: subpath || undefined,
        page,
        limit: pageSize,
        concise: concise || undefined,
        mainResearchGoal: 'Search GitHub codebase',
        researchGoal: `Find "${pattern}" in ${owner}/${repo}`,
        reasoning: 'CLI grep command',
      },
    ],
  });

  if (result.isError) {
    const errText =
      result.content[0]?.type === 'text' ? result.content[0].text : '';
    const sub = subpath ? `/${subpath}` : '';
    throw new Error(
      formatGithubFailure(errText, {
        target: `${owner}/${repo}${sub}`,
        genericLabel: 'GitHub search error',
      })
    );
  }

  return result.structuredContent as GithubCodeResult;
}

function renderGithubResults(
  sc: GithubCodeResult,
  limit: number,
  ref: GithubRenderRef = {}
): string {
  const data = sc?.results?.[0]?.data;
  const files = data?.files ?? [];
  const pagination = data?.pagination;
  const totalFiles = pagination?.uniqueFileCount ?? files.length;
  const hasNonExistentScope =
    data?.nonExistentScope === true ||
    (sc.emptyQueries ?? []).some(query => query.nonExistentScope === true);
  const lines: string[] = [];
  let shown = 0;

  for (const file of files) {
    if (shown >= limit) break;
    // concise:true → each entry is a flat "owner/repo:path" string.
    if (typeof file === 'string') {
      lines.push(`  ${c('cyan', file)}`);
      shown++;
      continue;
    }
    const repoLabel = `${file.owner ?? ref.owner ?? ''}/${file.repo ?? ref.repo ?? ''}`;
    lines.push(`  ${c('cyan', bold(file.path ?? ''))}  ${dim(repoLabel)}`);
    for (const m of (file.matches ?? []).slice(0, 5)) {
      const snippet = (m.value ?? '').trim().replace(/\n/g, ' ');
      if (snippet) lines.push(`    ${c('yellow', '›')} ${snippet}`);
    }
    shown++;
  }

  if (totalFiles > shown)
    lines.push(`\n  ${dim(`… ${totalFiles - shown} more files`)}`);
  if (pagination?.totalPages && pagination.totalPages > 1) {
    lines.push(
      `\n  ${dim(`Page ${pagination.currentPage ?? 1}/${pagination.totalPages} — use --page <n> to navigate`)}`
    );
  }

  if (lines.length === 0) {
    lines.push(`  ${dim('No matches found.')}`);

    const toolHints = sc?.hints ?? [];
    const indexingHint = toolHints.find(h =>
      /not be indexed|may not be indexed|zero here isn't proof/i.test(h)
    );
    if (indexingHint) {
      lines.push('');
      lines.push(`  ${c('yellow', '→')} ${indexingHint}`);
    }

    const repoRef =
      ref.owner && ref.repo ? `${ref.owner}/${ref.repo}` : '<owner>/<repo>';
    const cloneRef =
      ref.owner && ref.repo
        ? `${ref.owner}/${ref.repo}${ref.subpath ? `/${ref.subpath}` : ''}`
        : '<owner>/<repo>[/path]';
    lines.push('');
    if (hasNonExistentScope) {
      lines.push(
        `  ${c('yellow', '→')} GitHub reported this scope is not searchable. Verify owner/repo spelling, access, or path.`
      );
    } else {
      lines.push(
        `  ${c('yellow', '→')} GitHub code search may not index this repo. Alternatives:`
      );
    }
    lines.push(
      `     ${bold('ls ' + repoRef)}  — browse structure, then target a file`
    );
    lines.push(
      `     ${bold('cat ' + repoRef + '/<file> --match-string <pattern> --mode symbols')}  — search inside a known file (efficient)`
    );
    if (isCloneHintEnabled()) {
      lines.push(
        `     ${bold('clone ' + cloneRef)}  — pull it local, then rerun grep`
      );
    }
  }

  return lines.join('\n');
}

function isCloneHintEnabled(): boolean {
  try {
    const config = getConfigSync();
    return config.local.enabled && config.local.enableClone;
  } catch {
    return false;
  }
}

export const grepCommand: CLICommand = {
  name: 'grep',
  options: [
    { name: 'pattern', hasValue: true },
    { name: 'rule', hasValue: true },
    { name: 'type', hasValue: true },
    { name: 'mode', hasValue: true },
    { name: 'concise' },
    { name: 'include', hasValue: true },
    { name: 'exclude', hasValue: true },
    { name: 'context-lines', hasValue: true },
    { name: 'context', hasValue: true },
    { name: 'fixed' },
    { name: 'fixed-string' },
    { name: 'perl-regex' },
    { name: 'case-insensitive' },
    { name: 'case-sensitive' },
    { name: 'whole-word' },
    { name: 'invert-match' },
    { name: 'hidden' },
    { name: 'no-ignore' },
    { name: 'files-only' },
    { name: 'files-without-match' },
    { name: 'count-lines' },
    { name: 'count-matches' },
    { name: 'only-matching' },
    { name: 'unique' },
    { name: 'count' },
    { name: 'match-window', hasValue: true },
    { name: 'multiline' },
    { name: 'multiline-dotall' },
    { name: 'match-length', hasValue: true },
    { name: 'max-files', hasValue: true },
    { name: 'match-page', hasValue: true },
    { name: 'max-matches', hasValue: true },
    { name: 'limit', hasValue: true },
    { name: 'page', hasValue: true },
    { name: 'page-size', hasValue: true },
    { name: 'branch', hasValue: true },
    { name: 'repo', hasValue: true },
    { name: 'force-refresh' },
    { name: 'json' },
  ],
  handler: async args => {
    const { options } = args;
    const repoOption = getString(options, 'repo');

    // ── Structural (AST) branch ──────────────────────────────────────────────
    // --pattern / --rule switch grep to Octocode structural search (local-only).
    // arg[0] is the path here (there are no text keywords). Same executor and
    // renderer as text grep; only the query shape differs.
    const patternOpt = getString(options, 'pattern') || undefined;
    const ruleOpt = getString(options, 'rule') || undefined;
    if (patternOpt || ruleOpt) {
      const jsonOutput = getBool(options, 'json');
      // Foot-gun: a YAML rule passed in single quotes keeps `\n` LITERAL, so the
      // engine sees one line and the rule fails to parse. Detect and guide.
      if (ruleOpt && ruleOpt.includes('\\n') && !ruleOpt.includes('\n')) {
        const err =
          '--rule contains a literal "\\n" (single quotes do not expand escapes). Use $\'rule:\\n  ...\' or a real multiline string.';
        if (jsonOutput)
          console.log(JSON.stringify({ success: false, error: err }));
        else printCliError(err);
        process.exitCode = EXIT.USAGE;
        return;
      }
      if (patternOpt && ruleOpt) {
        const err = 'Provide either --pattern or --rule, not both.';
        if (jsonOutput)
          console.log(JSON.stringify({ success: false, error: err }));
        else printCliError(err);
        process.exitCode = EXIT.USAGE;
        return;
      }

      // Foot-gun guard: in structural mode arg[0] IS the path and there are no
      // text keywords. `grep <keywords> <path> --pattern …` would otherwise use
      // the keywords as the path and fail with a cryptic "no such file".
      if (!repoOption && args.args.length > 1) {
        const err =
          'Structural search (--pattern/--rule) takes a single local PATH as its only positional — it has no text keywords. ' +
          `You passed ${args.args.length} (${args.args.join(' ')}). Try: grep ${args.args[args.args.length - 1]} --pattern "${patternOpt ?? ruleOpt}"`;
        if (jsonOutput)
          console.log(JSON.stringify({ success: false, error: err }));
        else printCliError(err);
        process.exitCode = EXIT.USAGE;
        return;
      }

      const structuralTarget = args.args[0] || '.';
      const ref = repoOption ? undefined : resolveRef(structuralTarget);
      if (ref && isGithubRef(ref)) {
        const err =
          'Structural search (--pattern/--rule) is local-only because Octocode parses files on disk. ' +
          `Clone first: \`${cloneCommandFor(ref)}\`, then rerun grep on the local clone path. Or drop the flag for text search.`;
        if (jsonOutput)
          console.log(JSON.stringify({ success: false, error: err }));
        else printCliError(err);
        process.exitCode = EXIT.USAGE;
        return;
      }

      const rawLimitS = getString(options, 'limit');
      const limitS = rawLimitS ? parseInt(rawLimitS, 10) : 10;
      const ctxS = getString(options, 'context-lines', 'context');
      const maxS = getString(options, 'max-matches');
      const pageS = getString(options, 'page');
      const pageSizeS = getString(options, 'page-size');
      const includeS = getString(options, 'include');
      const shape = patternOpt ?? ruleOpt ?? '';
      const shellExpansionError = structuralShellExpansionError(shape);
      if (shellExpansionError) {
        if (jsonOutput)
          console.log(
            JSON.stringify({ success: false, error: shellExpansionError })
          );
        else printCliError(shellExpansionError);
        process.exitCode = EXIT.USAGE;
        return;
      }

      if (!jsonOutput) {
        process.stderr.write(
          `  ${dim(`Searching AST "${shape}" in ${repoOption ? `${repoOption}/${structuralTarget}` : refLabel(ref!)} ...`)}\n`
        );
      }

      try {
        const materialized = repoOption
          ? await materializeRemoteForCli({
              repoRef: repoOption,
              path: structuralTarget,
              branch: getString(options, 'branch') || undefined,
              forceRefresh: getBool(options, 'force-refresh') || undefined,
              kind: repoTargetKind(structuralTarget),
            })
          : undefined;
        const sc = await searchLocalStructural(
          materialized?.localPath ?? ref!.path,
          {
            pattern: patternOpt,
            rule: ruleOpt,
            include: localIncludeGlobs(
              listOption(includeS),
              getString(options, 'type') || undefined
            ),
            contextLines: ctxS ? parseInt(ctxS, 10) : undefined,
            maxMatchesPerFile: maxS ? parseInt(maxS, 10) : undefined,
            page: pageS ? parseInt(pageS, 10) : undefined,
            pageSize: pageSizeS ? parseInt(pageSizeS, 10) : limitS,
          }
        );
        if (jsonOutput) {
          const output = materialized
            ? withMaterializationHints({ structuredContent: sc }, materialized)
                .structuredContent
            : sc;
          console.log(JSON.stringify(output, null, 2));
          return;
        }
        const hints = materialized
          ? `\n${formatMaterializationHints(materialized)}`
          : '';
        console.log(
          '\n' +
            renderLocalResults(
              sc,
              limitS,
              ctxS ? parseInt(ctxS, 10) : undefined
            ) +
            hints +
            '\n'
        );
      } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : String(e);
        if (jsonOutput)
          console.log(JSON.stringify({ success: false, error: msg }));
        else printCliError(msg);
        process.exitCode = classifyToolErrorText(msg);
      }
      return;
    }

    const pattern = args.args[0] ?? '';
    const target = args.args[1] ?? '.';
    const typeFilter = getString(options, 'type');
    const branchOverride = getString(options, 'branch');
    const rawLimit = getString(options, 'limit');
    const limit = rawLimit ? parseInt(rawLimit, 10) : 10;
    const rawPage = getString(options, 'page');
    const rawPageSize = getString(options, 'page-size');
    const rawContextLines = getString(options, 'context-lines', 'context');
    const rawMaxMatches = getString(options, 'max-matches');
    const rawMatchLength = getString(options, 'match-length');
    const rawMaxFiles = getString(options, 'max-files');
    const rawMatchPage = getString(options, 'match-page');
    const rawMatchWindow = getString(options, 'match-window');
    const onlyMatching = getBool(options, 'only-matching') || undefined;
    const unique = getBool(options, 'unique') || undefined;
    const countUnique = getBool(options, 'count') || undefined;
    const page = rawPage ? parseInt(rawPage, 10) : undefined;
    const pageSize = rawPageSize ? parseInt(rawPageSize, 10) : limit;
    const contextLines = rawContextLines
      ? parseInt(rawContextLines, 10)
      : undefined;
    const maxMatchesPerFile = rawMaxMatches
      ? parseInt(rawMaxMatches, 10)
      : undefined;
    const matchContentLength = rawMatchLength
      ? parseInt(rawMatchLength, 10)
      : undefined;
    const maxFiles = rawMaxFiles ? parseInt(rawMaxFiles, 10) : undefined;
    const matchPage = rawMatchPage ? parseInt(rawMatchPage, 10) : undefined;
    const matchWindow = rawMatchWindow
      ? parseInt(rawMatchWindow, 10)
      : undefined;
    const concise = getBool(options, 'concise');
    const fixedString = getBool(options, 'fixed', 'fixed-string') || undefined;
    // Local has no `concise`; discovery mode is the paths-only equivalent.
    const modeOpt =
      getString(options, 'mode') || (concise ? 'discovery' : undefined);
    const includeOpt = getString(options, 'include');
    const excludeOpt = getString(options, 'exclude');
    const include = listOption(includeOpt);
    const exclude = listOption(excludeOpt);
    const jsonOutput = getBool(options, 'json');

    if (!pattern) {
      const err = 'Provide a search query (<keywords>).';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: err }));
      } else {
        printCliError(err);
        console.error(
          `\n  ${dim('Examples:')}\n` +
            `    grep "useState" src/\n` +
            `    grep "executeDirectTool" bgauryy/octocode-mcp\n` +
            `    grep "TODO" . --type ts\n` +
            `    ${dim('# for AST shape queries (local), pass --pattern:')}\n` +
            `    grep src --pattern "eval($X)"\n`
        );
      }
      process.exitCode = EXIT.USAGE;
      return;
    }

    const modeArg = getString(options, 'mode');
    if (modeArg && !GREP_MODES.has(modeArg)) {
      const err =
        'Invalid --mode. Use paginated, discovery, or detailed. For AST shape queries, pass --pattern or --rule.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: err }));
      } else {
        printCliError(err);
      }
      process.exitCode = EXIT.USAGE;
      return;
    }

    if ((unique || countUnique) && !onlyMatching) {
      const err = '--unique and --count require --only-matching.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: err }));
      } else {
        printCliError(err);
      }
      process.exitCode = EXIT.USAGE;
      return;
    }

    const ref = repoOption
      ? undefined
      : resolveRef(target, branchOverride || undefined);
    const label = repoOption ? `${repoOption}/${target}` : refLabel(ref!);

    if (ref && isGithubRef(ref) && (unique || countUnique)) {
      const err =
        '--unique and --count are local-only because they depend on local ripgrep match spans.';
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: err }));
      } else {
        printCliError(err);
      }
      process.exitCode = EXIT.USAGE;
      return;
    }

    if (!jsonOutput) {
      process.stderr.write(
        `  ${dim(`Searching "${pattern}" in ${label} ...`)}\n`
      );
    }

    try {
      if (repoOption) {
        const materialized = await materializeRemoteForCli({
          repoRef: repoOption,
          path: target === '.' ? undefined : target,
          branch: branchOverride || undefined,
          forceRefresh: getBool(options, 'force-refresh') || undefined,
          kind: repoTargetKind(target),
        });
        const sc = await searchLocal(pattern, materialized.localPath, {
          mode: modeOpt,
          include: localIncludeGlobs(include, typeFilter || undefined),
          exclude,
          contextLines,
          maxMatchesPerFile,
          fixedString,
          perlRegex: getBool(options, 'perl-regex') || undefined,
          caseInsensitive: getBool(options, 'case-insensitive') || undefined,
          caseSensitive: getBool(options, 'case-sensitive') || undefined,
          wholeWord: getBool(options, 'whole-word') || undefined,
          invertMatch: getBool(options, 'invert-match') || undefined,
          hidden: getBool(options, 'hidden') || undefined,
          noIgnore: getBool(options, 'no-ignore') || undefined,
          filesOnly: getBool(options, 'files-only') || undefined,
          filesWithoutMatch:
            getBool(options, 'files-without-match') || undefined,
          countLinesPerFile: getBool(options, 'count-lines') || undefined,
          countMatchesPerFile: getBool(options, 'count-matches') || undefined,
          multiline: getBool(options, 'multiline') || undefined,
          multilineDotall: getBool(options, 'multiline-dotall') || undefined,
          matchContentLength,
          maxFiles: maxFiles ?? limit,
          matchPage,
          onlyMatching,
          unique,
          countUnique,
          matchWindow,
          page,
          pageSize,
        });
        if (jsonOutput) {
          console.log(
            JSON.stringify(
              withMaterializationHints({ structuredContent: sc }, materialized)
                .structuredContent,
              null,
              2
            )
          );
          return;
        }
        console.log(
          '\n' +
            renderLocalResults(sc, limit, contextLines, {
              valuesOnly: Boolean(unique || countUnique),
            }) +
            '\n' +
            formatMaterializationHints(materialized) +
            '\n'
        );
      } else if (isGithubRef(ref!)) {
        const sc = await searchGithub(
          pattern,
          ref!.owner,
          ref!.repo,
          typeFilter || undefined,
          ref!.subpath || undefined,
          page,
          pageSize,
          concise
        );
        if (jsonOutput) {
          console.log(JSON.stringify(sc, null, 2));
          return;
        }
        console.log(
          '\n' +
            renderGithubResults(sc, limit, {
              owner: ref!.owner,
              repo: ref!.repo,
              subpath: ref!.subpath,
            }) +
            '\n'
        );
      } else {
        const sc = await searchLocal(pattern, ref!.path, {
          mode: modeOpt,
          include: localIncludeGlobs(include, typeFilter || undefined),
          exclude,
          contextLines,
          maxMatchesPerFile,
          fixedString,
          // `|| undefined` so an absent flag is omitted, never sent as `false`.
          // A literal `filesOnly: false` would OVERRIDE mode:"discovery" and
          // bring back full snippets — same idiom find.ts already uses.
          perlRegex: getBool(options, 'perl-regex') || undefined,
          caseInsensitive: getBool(options, 'case-insensitive') || undefined,
          caseSensitive: getBool(options, 'case-sensitive') || undefined,
          wholeWord: getBool(options, 'whole-word') || undefined,
          invertMatch: getBool(options, 'invert-match') || undefined,
          hidden: getBool(options, 'hidden') || undefined,
          noIgnore: getBool(options, 'no-ignore') || undefined,
          filesOnly: getBool(options, 'files-only') || undefined,
          filesWithoutMatch:
            getBool(options, 'files-without-match') || undefined,
          countLinesPerFile: getBool(options, 'count-lines') || undefined,
          countMatchesPerFile: getBool(options, 'count-matches') || undefined,
          multiline: getBool(options, 'multiline') || undefined,
          multilineDotall: getBool(options, 'multiline-dotall') || undefined,
          matchContentLength,
          maxFiles: maxFiles ?? limit,
          matchPage,
          onlyMatching,
          unique,
          countUnique,
          matchWindow,
          page,
          pageSize,
        });
        if (jsonOutput) {
          console.log(JSON.stringify(sc, null, 2));
          return;
        }
        console.log(
          '\n' +
            renderLocalResults(sc, limit, contextLines, {
              valuesOnly: Boolean(unique || countUnique),
            }) +
            '\n'
        );
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      if (jsonOutput) {
        console.log(JSON.stringify({ success: false, error: msg }));
      } else {
        printCliError(msg);
      }
      process.exitCode = classifyToolErrorText(msg);
    }
  },
};
