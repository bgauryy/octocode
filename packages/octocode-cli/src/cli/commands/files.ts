import type { CLICommand } from '../types.js';
import { getBool, getString } from '../options.js';
import { resolveRef, isGithubRef, isLocalRef, refLabel } from '../routing.js';
import { c, bold, dim } from '../../utils/colors.js';
import { EXIT } from '../exit-codes.js';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';
import {
  getDirectToolText,
  markDirectToolFailure,
  printDirectToolResult,
} from './direct-tool-output.js';

type SourceMode = 'auto' | 'local' | 'github';
type SearchMode = 'path' | 'content' | 'both';

type TextContent = {
  readonly type?: string;
  readonly text?: string;
};

type DirectToolResult = {
  readonly content?: readonly TextContent[];
  readonly structuredContent?: unknown;
  readonly isError?: boolean;
};

type PlannedCall = {
  readonly label: string;
  readonly toolName: string;
  readonly query: Record<string, unknown>;
};

const SOURCE_VALUES = new Set(['auto', 'local', 'github']);
const SEARCH_VALUES = new Set(['path', 'content', 'both']);
const LOCAL_FIND_SORT_VALUES = new Set(['modified', 'name', 'path', 'size']);
const LOCAL_SEARCH_SORT_VALUES = new Set([
  'path',
  'modified',
  'accessed',
  'created',
]);
const ENTRY_VALUES = new Set(['f', 'd']);
const LOCAL_SEARCH_MODES = new Set(['paginated', 'discovery', 'detailed']);

const OPTION_NAMES = new Set([
  'help',
  'json',
  'compact',
  'no-color',
  'source',
  'search',
  'ext',
  'path',
  'limit',
  'page',
  'page-size',
  'verbose',
  'owner',
  'repo',
  'filename',
  'name',
  'path-pattern',
  'regex',
  'entry',
  'min-depth',
  'max-depth',
  'empty',
  'modified-within',
  'modified-before',
  'accessed-within',
  'size-greater',
  'size-less',
  'permissions',
  'executable',
  'readable',
  'writable',
  'exclude-dir',
  'sort',
  'details',
  'show-modified',
  'mode',
  'fixed-string',
  'perl-regex',
  'case-insensitive',
  'case-sensitive',
  'whole-word',
  'invert-match',
  'include',
  'exclude',
  'hidden',
  'no-ignore',
  'files-only',
  'files-without-match',
  'context-lines',
  'match-length',
  'max-matches-per-file',
  'max-files',
  'match-page',
  'multiline',
  'multiline-dotall',
  'sort-reverse',
  'count-lines',
  'count-matches',
]);

const LOCAL_PATH_ONLY_OPTIONS = new Set([
  'name',
  'path-pattern',
  'regex',
  'entry',
  'min-depth',
  'max-depth',
  'empty',
  'modified-within',
  'modified-before',
  'accessed-within',
  'size-greater',
  'size-less',
  'permissions',
  'executable',
  'readable',
  'writable',
  'details',
  'show-modified',
]);

const LOCAL_CONTENT_ONLY_OPTIONS = new Set([
  'mode',
  'fixed-string',
  'perl-regex',
  'case-insensitive',
  'case-sensitive',
  'whole-word',
  'invert-match',
  'include',
  'exclude',
  'hidden',
  'no-ignore',
  'files-only',
  'files-without-match',
  'context-lines',
  'match-length',
  'max-matches-per-file',
  'max-files',
  'match-page',
  'multiline',
  'multiline-dotall',
  'sort-reverse',
  'count-lines',
  'count-matches',
]);

const GITHUB_ONLY_OPTIONS = new Set(['owner', 'repo', 'filename', 'verbose']);
const SHARED_LOCAL_OPTIONS = new Set(['exclude-dir', 'sort']);

function listOption(value: string): string[] | undefined {
  const list = value
    .split(',')
    .map(item => item.trim())
    .filter(Boolean);
  return list.length > 0 ? list : undefined;
}

function intOption(value: string): number | undefined {
  if (!value) return undefined;
  const parsed = Number.parseInt(value, 10);
  return Number.isInteger(parsed) && parsed >= 0 ? parsed : undefined;
}

function positiveIntOption(value: string): number | undefined {
  const parsed = intOption(value);
  return parsed && parsed > 0 ? parsed : undefined;
}

function globForQuery(query: string): string {
  return /[*?[\]]/.test(query) ? query : `*${query}*`;
}

function buildNameGlobs(query: string, extList: readonly string[]): string[] {
  const base = globForQuery(query);
  if (extList.length === 0) return [base];
  return extList.map(ext => {
    const cleanExt = ext.replace(/^\./, '');
    return `${base}.${cleanExt}`;
  });
}

function hasAnyOption(
  options: Record<string, string | boolean>,
  names: ReadonlySet<string>
): string | undefined {
  return Object.keys(options).find(name => names.has(name));
}

function error(message: string, jsonOutput: boolean): void {
  if (jsonOutput) {
    console.log(JSON.stringify({ success: false, error: message }));
  } else {
    console.error(`\n  ${c('red', 'x')} ${message}`);
    console.error(
      `\n  ${dim('Examples:')}\n` +
        `    octocode files auth src --source local --search path --ext ts\n` +
        `    octocode files executeDirectTool . --search content --ext ts\n` +
        `    octocode files auth bgauryy/octocode-mcp --source github --search both\n`
    );
  }
  process.exitCode = EXIT.USAGE;
}

function parseSourceMode(value: string): SourceMode | undefined {
  if (!value) return 'auto';
  return SOURCE_VALUES.has(value) ? (value as SourceMode) : undefined;
}

function parseSearchMode(value: string): SearchMode | undefined {
  if (!value) return 'path';
  return SEARCH_VALUES.has(value) ? (value as SearchMode) : undefined;
}

function resolveGithubTarget(args: {
  target: string;
  owner?: string;
  repo?: string;
  pathOverride?: string;
}): { owner: string; repo: string; path?: string } | undefined {
  const ref = args.target ? resolveRef(args.target) : undefined;
  if (args.owner && args.repo) {
    return {
      owner: args.owner,
      repo: args.repo,
      path: args.pathOverride || (ref && isGithubRef(ref) ? ref.subpath : ''),
    };
  }

  if (ref && isGithubRef(ref)) {
    return {
      owner: ref.owner,
      repo: ref.repo,
      path: args.pathOverride || ref.subpath,
    };
  }

  return undefined;
}

function resolveLocalPath(target: string, pathOverride?: string): string {
  const ref = resolveRef(pathOverride || target || '.');
  return isLocalRef(ref) ? ref.path : target || '.';
}

function validateOptions(
  options: Record<string, string | boolean>,
  source: SourceMode,
  search: SearchMode,
  targetIsGithub: boolean
): string | undefined {
  const unknown = Object.keys(options).find(
    option => !OPTION_NAMES.has(option)
  );
  if (unknown) return `Unknown files option --${unknown}.`;

  if (source === 'github' || (source === 'auto' && targetIsGithub)) {
    const localOnly =
      hasAnyOption(options, LOCAL_PATH_ONLY_OPTIONS) ||
      hasAnyOption(options, LOCAL_CONTENT_ONLY_OPTIONS) ||
      hasAnyOption(options, SHARED_LOCAL_OPTIONS);
    if (localOnly) return `--${localOnly} is local-only; use --source local.`;
  }

  if (source === 'local' || (!targetIsGithub && source === 'auto')) {
    const githubOnly = hasAnyOption(options, GITHUB_ONLY_OPTIONS);
    if (githubOnly)
      return `--${githubOnly} is GitHub-only; use --source github.`;
  }

  if (search === 'path') {
    const contentOnly = hasAnyOption(options, LOCAL_CONTENT_ONLY_OPTIONS);
    if (contentOnly)
      return `--${contentOnly} only applies to content search; use --search content or --search both.`;
  }

  if (search === 'content') {
    const pathOnly = hasAnyOption(options, LOCAL_PATH_ONLY_OPTIONS);
    if (pathOnly)
      return `--${pathOnly} only applies to path search; use --search path or --search both.`;
  }

  return undefined;
}

function buildLocalFindQuery(
  query: string,
  path: string,
  options: Record<string, string | boolean>
): Record<string, unknown> {
  const extList = listOption(getString(options, 'ext')) ?? [];
  const explicitNames = listOption(getString(options, 'name'));
  const limit = positiveIntOption(getString(options, 'limit'));
  const pageSize =
    positiveIntOption(getString(options, 'page-size')) ?? limit ?? undefined;

  return {
    path,
    names:
      explicitNames ??
      (getString(options, 'regex') || getString(options, 'path-pattern')
        ? undefined
        : buildNameGlobs(query, extList)),
    pathPattern: getString(options, 'path-pattern') || undefined,
    regex: getString(options, 'regex') || undefined,
    entryType: getString(options, 'entry') || 'f',
    minDepth: intOption(getString(options, 'min-depth')),
    maxDepth: intOption(getString(options, 'max-depth')),
    empty: getBool(options, 'empty') || undefined,
    modifiedWithin: getString(options, 'modified-within') || undefined,
    modifiedBefore: getString(options, 'modified-before') || undefined,
    accessedWithin: getString(options, 'accessed-within') || undefined,
    sizeGreater: getString(options, 'size-greater') || undefined,
    sizeLess: getString(options, 'size-less') || undefined,
    permissions: getString(options, 'permissions') || undefined,
    executable: getBool(options, 'executable') || undefined,
    readable: getBool(options, 'readable') || undefined,
    writable: getBool(options, 'writable') || undefined,
    excludeDir: listOption(getString(options, 'exclude-dir')),
    sortBy: getString(options, 'sort') || undefined,
    details: getBool(options, 'details') || undefined,
    showFileLastModified: getBool(options, 'show-modified') || undefined,
    limit,
    itemsPerPage: pageSize,
    page: positiveIntOption(getString(options, 'page')),
    mainResearchGoal: `Find files matching ${query}`,
    researchGoal: `Find file paths in ${path}`,
    reasoning: 'CLI files command path search',
  };
}

function buildLocalSearchQuery(
  query: string,
  path: string,
  options: Record<string, string | boolean>
): Record<string, unknown> {
  const extList = listOption(getString(options, 'ext')) ?? [];
  const include = listOption(getString(options, 'include')) ?? [];
  const extIncludes =
    extList.length > 1 ? extList.map(ext => `*.${ext.replace(/^\./, '')}`) : [];
  const limit = positiveIntOption(getString(options, 'limit'));
  const pageSize =
    positiveIntOption(getString(options, 'page-size')) ?? limit ?? undefined;

  return {
    keywords: query,
    path,
    mode: getString(options, 'mode') || undefined,
    fixedString: getBool(options, 'fixed-string') || undefined,
    perlRegex: getBool(options, 'perl-regex') || undefined,
    caseInsensitive: getBool(options, 'case-insensitive') || undefined,
    caseSensitive: getBool(options, 'case-sensitive') || undefined,
    wholeWord: getBool(options, 'whole-word') || undefined,
    invertMatch: getBool(options, 'invert-match') || undefined,
    include: [...include, ...extIncludes].length
      ? [...include, ...extIncludes]
      : undefined,
    exclude: listOption(getString(options, 'exclude')),
    excludeDir: listOption(getString(options, 'exclude-dir')),
    noIgnore: getBool(options, 'no-ignore') || undefined,
    hidden: getBool(options, 'hidden') || undefined,
    filesOnly: getBool(options, 'files-only') || undefined,
    filesWithoutMatch: getBool(options, 'files-without-match') || undefined,
    contextLines: intOption(getString(options, 'context-lines')),
    matchContentLength: positiveIntOption(getString(options, 'match-length')),
    maxMatchesPerFile: positiveIntOption(
      getString(options, 'max-matches-per-file')
    ),
    maxFiles: positiveIntOption(getString(options, 'max-files')),
    multiline: getBool(options, 'multiline') || undefined,
    multilineDotall: getBool(options, 'multiline-dotall') || undefined,
    sort: getString(options, 'sort') || undefined,
    sortReverse: getBool(options, 'sort-reverse') || undefined,
    langType: extList.length === 1 ? extList[0] : undefined,
    countLinesPerFile: getBool(options, 'count-lines') || undefined,
    countMatchesPerFile: getBool(options, 'count-matches') || undefined,
    matchPage: positiveIntOption(getString(options, 'match-page')),
    itemsPerPage: pageSize,
    page: positiveIntOption(getString(options, 'page')),
    mainResearchGoal: `Search file contents for ${query}`,
    researchGoal: `Find content matches in ${path}`,
    reasoning: 'CLI files command content search',
  };
}

function buildGithubQueries(
  query: string,
  target: { owner: string; repo: string; path?: string },
  options: Record<string, string | boolean>,
  match: 'path' | 'file'
): Record<string, unknown>[] {
  const extList = listOption(getString(options, 'ext')) ?? [undefined];
  return extList.map(extension => ({
    keywordsToSearch: [query],
    owner: target.owner,
    repo: target.repo,
    extension,
    filename: getString(options, 'filename') || undefined,
    path: getString(options, 'path') || target.path || undefined,
    match,
    limit: positiveIntOption(getString(options, 'limit')),
    page: positiveIntOption(getString(options, 'page')),
    verbose: getBool(options, 'verbose') || undefined,
    mainResearchGoal: `Search ${target.owner}/${target.repo} files for ${query}`,
    researchGoal:
      match === 'path'
        ? `Find matching file paths in ${target.owner}/${target.repo}`
        : `Find content matches in ${target.owner}/${target.repo}`,
    reasoning: `CLI files command GitHub ${match} search`,
  }));
}

function validateValues(
  options: Record<string, string | boolean>,
  source: SourceMode,
  search: SearchMode
): string | undefined {
  const sort = getString(options, 'sort');
  const entry = getString(options, 'entry');
  const localMode = getString(options, 'mode');
  const extList = listOption(getString(options, 'ext')) ?? [];

  if (entry && !ENTRY_VALUES.has(entry)) return '--entry must be f or d.';
  if (localMode && !LOCAL_SEARCH_MODES.has(localMode)) {
    return '--mode must be paginated, discovery, or detailed.';
  }
  if (
    sort &&
    source !== 'github' &&
    search === 'both' &&
    !['path', 'modified'].includes(sort)
  ) {
    return '--sort with --search both must be path or modified.';
  }
  if (
    sort &&
    search !== 'both' &&
    source !== 'github' &&
    !(
      (search === 'path' && LOCAL_FIND_SORT_VALUES.has(sort)) ||
      (search === 'content' && LOCAL_SEARCH_SORT_VALUES.has(sort))
    )
  ) {
    return search === 'path'
      ? '--sort for path search must be modified, name, path, or size.'
      : '--sort for content search must be path, modified, accessed, or created.';
  }
  if (source === 'github' && extList.length > 5) {
    return 'GitHub search supports at most 5 extensions per files command.';
  }
  if (getBool(options, 'fixed-string') && getBool(options, 'perl-regex')) {
    return '--fixed-string and --perl-regex are mutually exclusive.';
  }
  if (
    getBool(options, 'files-only') &&
    getBool(options, 'files-without-match')
  ) {
    return '--files-only and --files-without-match are mutually exclusive.';
  }
  if (getBool(options, 'count-lines') && getBool(options, 'count-matches')) {
    return '--count-lines and --count-matches are mutually exclusive.';
  }
  return undefined;
}

async function runCall(call: PlannedCall): Promise<DirectToolResult> {
  return executeDirectTool(call.toolName, {
    queries: [call.query],
  }) as Promise<DirectToolResult>;
}

async function runGithubCall(
  call: Omit<PlannedCall, 'query'> & { queries: Record<string, unknown>[] }
): Promise<DirectToolResult> {
  return executeDirectTool(call.toolName, {
    queries: call.queries,
  }) as Promise<DirectToolResult>;
}

function printComposite(
  outputs: Array<{ label: string; toolName: string; result: DirectToolResult }>,
  jsonOutput: boolean
): void {
  if (jsonOutput) {
    console.log(
      JSON.stringify(
        {
          results: outputs.map(output => ({
            search: output.label,
            tool: output.toolName,
            isError: Boolean(output.result.isError),
            structuredContent: output.result.structuredContent,
          })),
        },
        null,
        2
      )
    );
    return;
  }

  for (const output of outputs) {
    console.log();
    console.log(`  ${bold(output.label)}`);
    console.log(getDirectToolText(output.result));
  }
  console.log();
}

function isEmptySearchResult(result: DirectToolResult): boolean {
  if (result.isError) return false;

  const structured = result.structuredContent as
    | {
        readonly results?: readonly unknown[];
        readonly emptyQueries?: readonly unknown[];
        readonly status?: string;
      }
    | undefined;

  if (structured?.status === 'empty') return true;
  if (structured?.emptyQueries && structured.emptyQueries.length > 0) {
    return true;
  }
  if (structured?.results && structured.results.length === 0) return true;

  const text = getDirectToolText(result);
  return /^results:\s*\[\]/m.test(text) || /\bstatus:\s*empty\b/.test(text);
}

function printGithubPathFallbackHint(
  outputs: Array<{ label: string; result: DirectToolResult }>,
  target: { owner: string; repo: string } | undefined
): void {
  if (!target) return;

  const pathSearchEmpty = outputs.some(
    output =>
      output.label === 'GitHub path matches' &&
      isEmptySearchResult(output.result)
  );
  if (!pathSearchEmpty) return;

  const repoRef = `${target.owner}/${target.repo}`;
  console.log('smartHints:');
  console.log(
    `- GitHub path search can miss unindexed repos; use octocode tree ${repoRef} --depth 2 to browse paths.`
  );
  console.log(
    `- If the path is known, use octocode get ${repoRef}/<path> --mode standard.`
  );
  console.log();
}

export const filesCommand: CLICommand = {
  name: 'files',
  description:
    'Find files and content matches across local paths and GitHub repositories',
  usage:
    'octocode files <query> [path|owner/repo] [--owner <owner> --repo <repo>] [--source auto|local|github] [--search path|content|both] [--ext <list>] [--path <subpath>] [--limit <n>] [--page <n>] [--json]',
  options: [
    {
      name: 'source',
      hasValue: true,
      description:
        'Source selector: auto routes by target, local forces local tools, github forces GitHub search',
    },
    {
      name: 'search',
      hasValue: true,
      description:
        'Search mode: path finds filenames/paths, content finds text matches, both runs both modes',
    },
    {
      name: 'ext',
      hasValue: true,
      description:
        'Comma-separated extensions without dots; GitHub expands them into bulk queries',
    },
    {
      name: 'path',
      hasValue: true,
      description: 'Local search root override or GitHub repo subpath',
    },
    {
      name: 'limit',
      hasValue: true,
      description: 'Maximum results per underlying tool call',
    },
    {
      name: 'page',
      hasValue: true,
      description: 'Result page for paginated local or GitHub results',
    },
    {
      name: 'page-size',
      hasValue: true,
      description: 'Results per page, passed to local tools',
    },
    { name: 'owner', hasValue: true, description: 'GitHub owner' },
    { name: 'repo', hasValue: true, description: 'GitHub repository' },
    { name: 'filename', hasValue: true, description: 'GitHub filename filter' },
    { name: 'name', hasValue: true, description: 'Local find name pattern(s)' },
    {
      name: 'path-pattern',
      hasValue: true,
      description: 'Local path pattern filter',
    },
    { name: 'regex', hasValue: true, description: 'Local find regex' },
    { name: 'entry', hasValue: true, description: 'Local entry type: f or d' },
    { name: 'min-depth', hasValue: true, description: 'Local minimum depth' },
    { name: 'max-depth', hasValue: true, description: 'Local maximum depth' },
    { name: 'empty', description: 'Find empty local files/directories' },
    {
      name: 'modified-within',
      hasValue: true,
      description: 'Local modified-within filter',
    },
    {
      name: 'modified-before',
      hasValue: true,
      description: 'Local modified-before filter',
    },
    {
      name: 'accessed-within',
      hasValue: true,
      description: 'Local accessed-within filter',
    },
    {
      name: 'size-greater',
      hasValue: true,
      description: 'Local size greater-than filter',
    },
    {
      name: 'size-less',
      hasValue: true,
      description: 'Local size less-than filter',
    },
    {
      name: 'permissions',
      hasValue: true,
      description: 'Local permissions filter',
    },
    { name: 'executable', description: 'Find executable local files' },
    { name: 'readable', description: 'Find readable local files' },
    { name: 'writable', description: 'Find writable local files' },
    {
      name: 'exclude-dir',
      hasValue: true,
      description: 'Local directories to exclude',
    },
    {
      name: 'sort',
      hasValue: true,
      description:
        'Local sort field: path/modified for both; name/size also for path; accessed/created also for content',
    },
    {
      name: 'include',
      hasValue: true,
      description: 'Local content include globs',
    },
    {
      name: 'exclude',
      hasValue: true,
      description: 'Local content exclude globs',
    },
    {
      name: 'mode',
      hasValue: true,
      description: 'Local content mode: paginated, discovery, detailed',
    },
    { name: 'fixed-string', description: 'Use fixed-string content search' },
    { name: 'perl-regex', description: 'Use Perl-compatible regex search' },
    {
      name: 'case-insensitive',
      description: 'Case-insensitive content search',
    },
    { name: 'case-sensitive', description: 'Case-sensitive content search' },
    { name: 'whole-word', description: 'Match whole words in content search' },
    { name: 'invert-match', description: 'Invert local content matches' },
    { name: 'hidden', description: 'Search hidden local files' },
    {
      name: 'no-ignore',
      description: 'Ignore ignore files during local search',
    },
    { name: 'files-only', description: 'Return matching file paths only' },
    {
      name: 'files-without-match',
      description: 'Return files without a content match',
    },
    {
      name: 'context-lines',
      hasValue: true,
      description: 'Context lines around content matches',
    },
    {
      name: 'match-length',
      hasValue: true,
      description: 'Maximum match text length',
    },
    {
      name: 'max-matches-per-file',
      hasValue: true,
      description: 'Maximum matches per file',
    },
    {
      name: 'max-files',
      hasValue: true,
      description: 'Maximum local content files',
    },
    {
      name: 'match-page',
      hasValue: true,
      description: 'Page within matches for a file',
    },
    { name: 'multiline', description: 'Enable multiline local search' },
    {
      name: 'multiline-dotall',
      description: 'Make dot match newlines in multiline search',
    },
    { name: 'sort-reverse', description: 'Reverse local content sort' },
    { name: 'count-lines', description: 'Count matching lines per file' },
    { name: 'count-matches', description: 'Count matches per file' },
    { name: 'details', description: 'Show local file metadata' },
    {
      name: 'show-modified',
      description: 'Show local file modification timestamps',
    },
    { name: 'verbose', description: 'Verbose GitHub search results' },
    { name: 'json', description: 'Output raw JSON results' },
  ],
  handler: async args => {
    const jsonOutput = getBool(args.options, 'json');
    const query = args.args[0] ?? '';
    const targetArg = args.args[1] ?? '';
    const source = parseSourceMode(getString(args.options, 'source'));
    const search = parseSearchMode(getString(args.options, 'search'));

    if (!query) {
      error('Provide a file query.', jsonOutput);
      return;
    }
    if (!source) {
      error('Invalid --source. Use auto, local, or github.', jsonOutput);
      return;
    }
    if (!search) {
      error('Invalid --search. Use path, content, or both.', jsonOutput);
      return;
    }

    const resolvedTarget = targetArg ? resolveRef(targetArg) : undefined;
    const targetIsGithub = Boolean(
      resolvedTarget && isGithubRef(resolvedTarget)
    );
    const hasGithubPair = Boolean(
      getString(args.options, 'owner') && getString(args.options, 'repo')
    );
    const optionError = validateOptions(
      args.options,
      source,
      search,
      targetIsGithub || hasGithubPair
    );
    if (optionError) {
      error(optionError, jsonOutput);
      return;
    }

    const valueError = validateValues(args.options, source, search);
    if (valueError) {
      error(valueError, jsonOutput);
      return;
    }

    if (source === 'local' && (targetIsGithub || hasGithubPair)) {
      error(
        'Cannot use --source local with a GitHub reference target.',
        jsonOutput
      );
      return;
    }

    const effectiveSource: Exclude<SourceMode, 'auto'> =
      source === 'auto'
        ? targetIsGithub || hasGithubPair
          ? 'github'
          : 'local'
        : source;

    const outputs: Array<{
      label: string;
      toolName: string;
      result: DirectToolResult;
    }> = [];

    let githubTargetForHints: { owner: string; repo: string } | undefined;

    try {
      if (effectiveSource === 'github') {
        const target = resolveGithubTarget({
          target: targetArg,
          owner: getString(args.options, 'owner') || undefined,
          repo: getString(args.options, 'repo') || undefined,
          pathOverride: getString(args.options, 'path') || undefined,
        });
        if (!target) {
          error(
            'GitHub files search needs <owner/repo> or --owner <owner> --repo <repo>.',
            jsonOutput
          );
          return;
        }
        githubTargetForHints = { owner: target.owner, repo: target.repo };

        if (!jsonOutput) {
          process.stderr.write(
            `  ${dim(`Searching files in ${target.owner}/${target.repo} ...`)}\n`
          );
        }

        const searches: Array<{ label: string; match: 'path' | 'file' }> =
          search === 'both'
            ? [
                { label: 'GitHub path matches', match: 'path' },
                { label: 'GitHub content matches', match: 'file' },
              ]
            : [
                {
                  label:
                    search === 'path'
                      ? 'GitHub path matches'
                      : 'GitHub content matches',
                  match: search === 'path' ? 'path' : 'file',
                },
              ];

        for (const planned of searches) {
          const result = await runGithubCall({
            label: planned.label,
            toolName: 'ghSearchCode',
            queries: buildGithubQueries(
              query,
              target,
              args.options,
              planned.match
            ),
          });
          outputs.push({
            label: planned.label,
            toolName: 'ghSearchCode',
            result,
          });
        }
      } else {
        const localPath = resolveLocalPath(
          targetArg || '.',
          getString(args.options, 'path') || undefined
        );
        if (!jsonOutput) {
          process.stderr.write(
            `  ${dim(`Searching files in ${refLabel({ kind: 'local', path: localPath })} ...`)}\n`
          );
        }

        if (search === 'path' || search === 'both') {
          const call = {
            label: 'Local path matches',
            toolName: 'localFindFiles',
            query: buildLocalFindQuery(query, localPath, args.options),
          };
          outputs.push({ ...call, result: await runCall(call) });
        }

        if (search === 'content' || search === 'both') {
          const call = {
            label: 'Local content matches',
            toolName: 'localSearchCode',
            query: buildLocalSearchQuery(query, localPath, args.options),
          };
          outputs.push({ ...call, result: await runCall(call) });
        }
      }

      if (outputs.length === 1) {
        printDirectToolResult(outputs[0]!.result, jsonOutput);
      } else {
        printComposite(outputs, jsonOutput);
      }
      if (!jsonOutput) {
        printGithubPathFallbackHint(outputs, githubTargetForHints);
      }

      for (const output of outputs) {
        markDirectToolFailure(output.result);
      }
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      if (jsonOutput) {
        console.log(
          JSON.stringify({
            success: false,
            error: `Octocode tool runtime failed: ${message}`,
          })
        );
      } else {
        console.error(
          `\n  ${c('red', 'x')} Octocode tool runtime failed: ${message}\n`
        );
      }
      process.exitCode = EXIT.TOOL;
    }
  },
};
