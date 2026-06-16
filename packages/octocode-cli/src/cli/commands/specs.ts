import type { CLICommandSpec } from '../types.js';

export const COMMAND_SPECS: readonly CLICommandSpec[] = [
  {
    name: 'get',
    description:
      'Fetch and minify file content for local paths and GitHub references',
    usage:
      'octocode get <path|github-ref> [--mode none|standard|symbols] [--branch <ref>] [--match-string <s>] [--match-regex] [--match-case-sensitive] [--start-line <n>] [--end-line <n>] [--context-lines <n>] [--page-size <n>] [--page <n>] [--char-offset <n>] [--char-length <n>] [--full-content] [--content-type file|directory] [--force-refresh] [--json]',
    scheme: [
      'arg[0] target: required string; local file path OR owner/repo/path GitHub ref.',
      'options: --mode enum(none|standard|symbols, default standard), --branch string, --content-type enum(file|directory).',
      'slice options: --match-string string, --match-regex boolean, --match-case-sensitive boolean, --start-line int, --end-line int, --context-lines int.',
      'page options: --page-size int chars, --page int, --char-offset int, --char-length int, --full-content boolean.',
      'runtime: local target -> localGetFileContent; GitHub target -> ghGetFileContent.',
      'output: YAML content by default; --json returns the raw tool envelope.',
    ],
    whenToUse: [
      'Use after tree/files/search identifies a file or exact slice to read.',
      'Default mode is standard (strips comments/blanks, token-efficient). Use --mode symbols for a skeleton map; use --mode none only when comments or exact formatting are required.',
      'If output is paginated, continue only when the page hints say more content is needed; otherwise narrow with --match-string or line bounds.',
    ],
    examples: [
      'octocode get packages/octocode-cli/src/cli/index.ts',
      'octocode get bgauryy/octocode-mcp/package.json --mode none',
      'octocode get src/index.ts --match-string "runCLI" --mode none',
    ],
    options: [
      {
        name: 'mode',
        hasValue: true,
        description:
          'Minification mode: standard for readable code, symbols for outline, none for exact text',
      },
      {
        name: 'branch',
        hasValue: true,
        description: 'Branch or ref for GitHub paths',
      },
      {
        name: 'match-string',
        hasValue: true,
        description: 'Return only sections matching this string',
      },
      { name: 'match-regex', description: 'Treat match-string as a regex' },
      {
        name: 'match-case-sensitive',
        description: 'Match string case-sensitively',
      },
      {
        name: 'start-line',
        hasValue: true,
        description: 'First line to return, 1-based',
      },
      {
        name: 'end-line',
        hasValue: true,
        description: 'Last line to return, 1-based',
      },
      {
        name: 'context-lines',
        hasValue: true,
        description: 'Context lines around match-string slices',
      },
      {
        name: 'page-size',
        hasValue: true,
        description: 'Characters per page',
      },
      {
        name: 'page',
        hasValue: true,
        description: 'Page number when using page-size',
      },
      { name: 'char-offset', hasValue: true, description: 'Character offset' },
      { name: 'char-length', hasValue: true, description: 'Character length' },
      {
        name: 'full-content',
        description: 'Return the whole file instead of a page or match slice',
      },
      {
        name: 'content-type',
        hasValue: true,
        description: 'GitHub content type: file or directory',
      },
      { name: 'force-refresh', description: 'Bypass GitHub cache' },
      { name: 'json', description: 'Output as JSON' },
    ],
  },
  {
    name: 'tree',
    description:
      'View directory structure for local paths and GitHub repositories',
    usage:
      'octocode tree <path|github-ref> [--depth <n>] [--branch <ref>] [--json]',
    scheme: [
      'arg[0] target: required string; local directory path OR owner/repo[/subpath] GitHub ref.',
      'options: --depth positive int, --branch string for GitHub refs, --json boolean.',
      'runtime: local target -> localViewStructure; GitHub target -> ghViewRepoStructure.',
      'output: YAML tree by default; --json returns the raw tool envelope.',
    ],
    whenToUse: [
      'Use first when the repository or directory layout is unknown.',
      'Follow with files/search to locate specific paths, then get for source.',
    ],
    examples: [
      'octocode tree packages/octocode-cli/src --depth 2',
      'octocode tree bgauryy/octocode-mcp --depth 2',
    ],
    options: [
      { name: 'depth', hasValue: true, description: 'Directory depth' },
      {
        name: 'branch',
        hasValue: true,
        description: 'Branch or ref for GitHub paths',
      },
      { name: 'json', description: 'Output raw JSON structure' },
    ],
  },
  {
    name: 'search',
    description: 'Smart code search for local paths and GitHub repositories',
    usage:
      'octocode search <pattern> <path|github-ref> [--type <ext>] [--branch <ref>] [--limit <n>] [--page <n>] [--page-size <n>] [--json]',
    scheme: [
      'arg[0] pattern: required string; code text, regex-ish text, symbol name, error text, or import.',
      'arg[1] target: required string; local path OR owner/repo[/path] GitHub ref.',
      'options: --type extension/language string, --branch string, --limit int, --page int, --page-size int, --json boolean.',
      'runtime: local target -> localSearchCode; GitHub target -> ghSearchCode.',
      'output: YAML search hits by default; snippets are discovery, then use get for evidence.',
    ],
    whenToUse: [
      'Use when you know code text, a function name, an error string, or an import to find.',
      'Search results are discovery; follow with get --match-string or lsp when you need exact proof.',
      'This is the code-search smart command; there is no separate `code` command.',
    ],
    examples: [
      'octocode search "executeDirectTool" packages/octocode-cli/src --type ts',
      'octocode search "useState" facebook/react --type tsx --limit 5',
    ],
    options: [
      {
        name: 'type',
        hasValue: true,
        description: 'Filter by language or extension, for example ts, py, go',
      },
      {
        name: 'limit',
        hasValue: true,
        description: 'Max files to show in rendered output',
      },
      {
        name: 'page',
        hasValue: true,
        description: 'Result page to fetch from the underlying search tool',
      },
      {
        name: 'page-size',
        hasValue: true,
        description: 'Results per page passed to the underlying search tool',
      },
      {
        name: 'branch',
        hasValue: true,
        description: 'Branch or ref for GitHub paths',
      },
      { name: 'json', description: 'Output raw JSON results' },
    ],
  },
  {
    name: 'files',
    description:
      'Find file paths and content matches across local paths and GitHub repositories',
    usage:
      'octocode files <query> [path|owner/repo] [--owner <owner> --repo <repo>] [--source auto|local|github] [--search path|content|both] [--ext <list>] [--path <subpath>] [--limit <n>] [--page <n>] [--json]',
    scheme: [
      'arg[0] query: required string; filename/path fragment or content term.',
      'arg[1] target: optional local path OR owner/repo; may be replaced by --owner and --repo for GitHub.',
      'source options: --source enum(auto|local|github, default auto), --search enum(path|content|both, default path), --ext comma-list, --path subpath/root.',
      'GitHub filters: --owner string, --repo string, --filename string, --branch not supported here.',
      'local path filters: --name, --path-pattern, --regex, --entry enum(f|d), --min-depth int, --max-depth int, size/time/permission flags.',
      'local content filters: --include/--exclude globs, --mode enum(paginated|discovery|detailed), rg booleans, context/count/page controls.',
      'runtime: path search -> localFindFiles or ghSearchCode(match:path); content search -> localSearchCode or ghSearchCode(match:file).',
      'output: YAML file hits by default; --json returns raw combined tool results.',
    ],
    whenToUse: [
      'Use when you know a filename, path fragment, extension, or broad content term.',
      'Use --search path for filename/path discovery, content for text matches, and both when unsure.',
      'Follow path hits with get for exact source, or search when you need line-level code matches.',
    ],
    examples: [
      'octocode files "command-help" packages/octocode-cli/src --search both --ext ts',
      'octocode files "package.json" bgauryy/octocode-mcp --search path --source github',
      'octocode files "parser" . --source local --search path --ext ts',
    ],
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
      {
        name: 'filename',
        hasValue: true,
        description: 'GitHub filename filter',
      },
      { name: 'name', hasValue: true, description: 'Local name pattern(s)' },
      {
        name: 'path-pattern',
        hasValue: true,
        description: 'Local path pattern filter',
      },
      { name: 'regex', hasValue: true, description: 'Local find regex' },
      {
        name: 'entry',
        hasValue: true,
        description: 'Local entry type: f or d',
      },
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
      {
        name: 'whole-word',
        description: 'Match whole words in content search',
      },
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
  },
  {
    name: 'pr',
    description:
      'Search and view pull requests; list with filters or deep-dive one PR',
    usage:
      'octocode pr <owner/repo[#N] | PR-URL> [--pr <n>] [--state open|closed|merged] [--patches] [--comments] [--commits] [--deep] [--json]',
    scheme: [
      'arg[0] target: required owner/repo, owner/repo#number, or GitHub PR URL.',
      'selection: --pr int selects one PR; #N or PR URL also selects one PR.',
      'list filters: --query string, --state enum(open|closed|merged), --author string, --label string, --base string, --limit int, --page int, --page-size int.',
      'content flags: --patches, --comments, --commits, --deep booleans; --file path narrows patches; --match-string narrows returned content.',
      'runtime: ghSearchPRs; broad target lists PRs, selected PR fetches requested surfaces.',
      'output: YAML PR metadata/content by default; --json returns raw tool envelope.',
    ],
    whenToUse: [
      'Use to research change history, PR discussion, review comments, or diffs.',
      'List mode finds candidate PRs; PR number or URL deep-dives one PR.',
      'For large PRs, use --file or --match-string before paging broad patches/comments.',
    ],
    examples: [
      'octocode pr bgauryy/octocode-mcp --state open --limit 5',
      'octocode pr bgauryy/octocode-mcp#123 --patches --comments',
      'octocode pr https://github.com/bgauryy/octocode-mcp/pull/123 --deep',
    ],
    options: [
      { name: 'pr', hasValue: true, description: 'PR number to view' },
      {
        name: 'query',
        hasValue: true,
        description: 'Keyword search in list mode',
      },
      { name: 'state', hasValue: true, description: 'Filter by state' },
      { name: 'author', hasValue: true, description: 'Filter by PR author' },
      { name: 'label', hasValue: true, description: 'Filter by label' },
      { name: 'base', hasValue: true, description: 'Filter by base branch' },
      { name: 'limit', hasValue: true, description: 'Max PRs to show' },
      { name: 'patches', description: 'Include unified diffs' },
      { name: 'file', hasValue: true, description: 'Show diff for one file' },
      { name: 'comments', description: 'Include comments' },
      { name: 'commits', description: 'Include commits' },
      {
        name: 'deep',
        description: 'Include patches, comments, commits, and reviews',
      },
      {
        name: 'match-string',
        hasValue: true,
        description: 'Narrow PR content',
      },
      { name: 'page', hasValue: true, description: 'Page number' },
      { name: 'page-size', hasValue: true, description: 'Results per page' },
      { name: 'json', description: 'Output raw JSON' },
    ],
  },
  {
    name: 'repo',
    description: 'Search GitHub repositories with research-oriented filters',
    usage:
      'octocode repo <keywords...> [--topic <list>] [--language <lang>] [--owner <owner>] [--stars <range>] [--forks <range>] [--good-first-issues <range>] [--license <spdx>] [--created <range>] [--updated <range>] [--size <range>] [--match name,description,readme] [--sort stars|forks|help-wanted-issues|updated|best-match] [--archived true|false] [--visibility public|private] [--limit <n>] [--page <n>] [--verbose] [--json]',
    scheme: [
      'args keywords: optional string list; AND-combined repository search keywords.',
      'discovery filters: --owner string, --topic comma-list, --language string, --license SPDX, --visibility enum(public|private).',
      'range filters: --stars, --forks, --good-first-issues, --created, --updated, --size use GitHub search range syntax.',
      'match/sort: --match comma-list(name|description|readme), --sort enum(stars|forks|help-wanted-issues|updated|best-match), --archived boolean string.',
      'pagination/output: --limit int, --page int, --verbose boolean, --json boolean.',
      'runtime: ghSearchRepos.',
      'output: YAML repo list by default; verbose/json exposes richer repository fields.',
    ],
    whenToUse: [
      'Use before GitHub tree/files/search when you need to discover the right repository.',
      'Use --owner with no keywords to enumerate an organization.',
    ],
    examples: [
      'octocode repo "mcp server" --language TypeScript --stars ">100"',
      'octocode repo --owner bgauryy --limit 10',
      'octocode repo "code search" --topic mcp --sort stars',
    ],
    options: [
      { name: 'topic', hasValue: true, description: 'Comma-separated topics' },
      { name: 'language', hasValue: true, description: 'Language filter' },
      { name: 'owner', hasValue: true, description: 'Owner or organization' },
      { name: 'stars', hasValue: true, description: 'Stars range' },
      { name: 'forks', hasValue: true, description: 'Forks range' },
      {
        name: 'good-first-issues',
        hasValue: true,
        description: 'Good-first-issues range',
      },
      { name: 'license', hasValue: true, description: 'SPDX license key' },
      { name: 'created', hasValue: true, description: 'Created date range' },
      { name: 'updated', hasValue: true, description: 'Pushed date range' },
      { name: 'size', hasValue: true, description: 'Repository size range' },
      {
        name: 'match',
        hasValue: true,
        description: 'Comma-separated scopes: name,description,readme',
      },
      {
        name: 'sort',
        hasValue: true,
        description:
          'Sort: stars, forks, help-wanted-issues, updated, best-match',
      },
      {
        name: 'archived',
        hasValue: true,
        description: 'Include only archived repos when true',
      },
      {
        name: 'visibility',
        hasValue: true,
        description: 'Visibility: public or private',
      },
      { name: 'limit', hasValue: true, description: 'Max repositories' },
      { name: 'page', hasValue: true, description: 'Result page' },
      { name: 'verbose', description: 'Return structured repository objects' },
      { name: 'json', description: 'Output raw JSON results' },
    ],
  },
  {
    name: 'pkg',
    description: 'Research an npm package and its source repository',
    usage: 'octocode pkg <package> [--page <n>] [--json]',
    scheme: [
      'arg[0] package: required npm package name or keyword query.',
      'options: --page int, --json boolean.',
      'runtime: npmSearch.',
      'output: YAML package metadata; exact package includes repository handoff when available.',
    ],
    whenToUse: [
      'Use when the research starts from an npm package name.',
      'Follow source repository fields with repo/tree/files/get for implementation evidence.',
    ],
    examples: ['octocode pkg zod', 'octocode pkg "@modelcontextprotocol/sdk"'],
    options: [
      {
        name: 'page',
        hasValue: true,
        description: 'Result page for package keyword searches',
      },
      { name: 'json', description: 'Output raw JSON results' },
    ],
  },
  {
    name: 'lsp',
    description:
      'Run LSP semantic navigation for a local source file after you know the symbol and line',
    usage:
      'octocode lsp <file> --type <type> [--symbol <name>] [--line <n>] [--workspace-root <path>] [--page <n>] [--page-size <n>] [--context-lines <n>] [--depth <n>] [--format structured|compact] [--json]',
    scheme: [
      'arg[0] file: required local source file path.',
      'required option: --type enum(definition|references|callers|callees|callHierarchy|hover|documentSymbols|typeDefinition|implementation).',
      'symbol options: --symbol string and --line int are required except when --type documentSymbols.',
      'context options: --workspace-root path, --page int, --page-size int, --context-lines int, --depth int, --format enum(structured|compact).',
      'runtime: lspGetSemantics with uri=file path.',
      'output: YAML semantic locations/content by default; --json returns raw tool envelope.',
    ],
    whenToUse: [
      'Use after search or symbols gives a local file, exact symbol name, and line number.',
      'Use symbols for directory outlines; use lsp for references, definitions, hover, callers, callees, typeDefinition, or implementation.',
      'documentSymbols works without --symbol/--line, but symbols is the friendlier outline command.',
      'If the line is unknown, run search or symbols first; do not guess --line.',
    ],
    examples: [
      'octocode lsp src/index.ts --type documentSymbols',
      'octocode lsp src/index.ts --type references --symbol runCLI --line 42',
      'octocode lsp src/index.ts --type definition --symbol runCLI --line 42 --format compact',
    ],
    options: [
      {
        name: 'type',
        hasValue: true,
        description:
          'Semantic query: definition, references, callers, callees, callHierarchy, hover, documentSymbols, typeDefinition, implementation',
      },
      {
        name: 'symbol',
        hasValue: true,
        description: 'Symbol name; required unless type is documentSymbols',
      },
      {
        name: 'line',
        hasValue: true,
        description: 'Line hint; required unless type is documentSymbols',
      },
      {
        name: 'workspace-root',
        hasValue: true,
        description: 'Workspace root for the language server',
      },
      { name: 'page', hasValue: true, description: 'Result page' },
      { name: 'page-size', hasValue: true, description: 'Results per page' },
      {
        name: 'context-lines',
        hasValue: true,
        description: 'Context lines around returned locations',
      },
      { name: 'depth', hasValue: true, description: 'Call hierarchy depth' },
      {
        name: 'format',
        hasValue: true,
        description: 'LSP output format: structured or compact',
      },
      { name: 'json', description: 'Output raw JSON results' },
    ],
  },
  {
    name: 'symbols',
    description:
      'Show a semantic symbol outline for a local file or directory before deeper LSP navigation',
    usage:
      'octocode symbols <file|path> [--ext <list>] [--kind <kind>] [--limit <n>] [--depth <n>] [--page-size <n>] [--json]',
    scheme: [
      'arg[0] target: required local source file or directory path.',
      'directory options: --ext comma-list, --limit int files, --depth int directory depth.',
      'render options: --kind string symbol kind filter, --page-size int symbols per file, --json boolean.',
      'runtime: file -> lspGetSemantics(documentSymbols); directory -> localFindFiles then batched documentSymbols.',
      'output: compact YAML-like outline by default; --json returns file list and raw LSP results.',
    ],
    whenToUse: [
      'Use first on local code to map classes, functions, methods, and exported shapes.',
      'This is the compact shortcut for LSP documentSymbols plus directory file discovery.',
      'Follow with lsp --type references/definition/hover when a specific symbol and line matter.',
    ],
    examples: [
      'octocode symbols packages/octocode-cli/src --ext ts --limit 10',
      'octocode symbols packages/octocode-cli/src/cli/commands/lsp.ts --kind function',
    ],
    options: [
      {
        name: 'ext',
        hasValue: true,
        description: 'Comma-separated source extensions for directory mode',
      },
      {
        name: 'kind',
        hasValue: true,
        description: 'Filter rendered symbols by kind',
      },
      {
        name: 'limit',
        hasValue: true,
        description: 'Maximum files to inspect in directory mode',
      },
      {
        name: 'depth',
        hasValue: true,
        description: 'Directory discovery depth',
      },
      { name: 'page-size', hasValue: true, description: 'Symbols per file' },
      { name: 'json', description: 'Output raw JSON results' },
    ],
  },
  {
    name: 'context',
    description:
      'Print the agent protocol, MCP system prompt, tool descriptions, and schemas',
    usage: 'octocode context [--full]  OR  octocode --context [--full]',
    scheme: [
      'args: none.',
      'top-level shortcut: --context prints the same agent target as the context command.',
      'options: --full boolean includes every full JSON input schema inline; omit it for compact schema summaries.',
      'output: CLI research protocol, MCP system prompt, smart command routing, tool descriptions, and schemas.',
    ],
    whenToUse: [
      'Use before autonomous research to see the exact operating prompt and tool routing rules.',
      'Use --full when an agent needs all JSON schemas in one target output.',
    ],
    examples: [
      'octocode context',
      'octocode context --full',
      'octocode --context --full',
    ],
    options: [
      {
        name: 'full',
        description: 'Include every full JSON input schema inline',
      },
    ],
  },
  {
    name: 'install',
    description: 'Install octocode-mcp for an IDE',
    usage:
      'octocode install --ide <ide> [--method npx] [--force] [--check] [--rollback] [--backup-path <path>] [--json]',
    scheme: [
      'args: none.',
      'required option: --ide supported client id.',
      'options: --method enum(npx, default npx), --force boolean, --check boolean, --rollback boolean, --backup-path path, --json boolean.',
      'runtime: writes or validates MCP client configuration for the selected IDE.',
      'output: install/check/rollback status; --json returns structured result.',
    ],
    options: [
      { name: 'ide', hasValue: true, description: 'IDE to configure' },
      {
        name: 'method',
        hasValue: true,
        description: 'Installation method (npx)',
        default: 'npx',
      },
      { name: 'force', description: 'Overwrite existing configuration' },
      { name: 'check', description: 'Pre-flight only' },
      { name: 'rollback', description: 'Restore the most recent backup' },
      {
        name: 'backup-path',
        hasValue: true,
        description: 'Backup file to restore',
      },
      { name: 'json', description: 'Output result as JSON' },
    ],
  },
  {
    name: 'auth',
    description: 'Manage GitHub authentication',
    usage:
      'octocode auth [login|logout|status|token|refresh] [--hostname <host>] [--json]',
    scheme: [
      'arg[0] action: optional enum(login|logout|status|token|refresh); defaults to interactive/auth status flow.',
      'options: --hostname GitHub Enterprise host, --json boolean.',
      'runtime: delegates to auth storage, GitHub OAuth, token refresh, or status lookup.',
      'output: auth action result; --json returns structured status/result.',
    ],
    options: [
      {
        name: 'hostname',
        hasValue: true,
        description: 'GitHub Enterprise hostname',
      },
      { name: 'json', description: 'Output as JSON' },
    ],
  },
  {
    name: 'login',
    description: 'Authenticate with GitHub',
    usage:
      'octocode login [--hostname <host>] [--git-protocol <ssh|https>] [--force] [--json]',
    scheme: [
      'args: none.',
      'options: --hostname GitHub Enterprise host, --git-protocol enum(ssh|https), --force boolean, --json boolean.',
      'runtime: GitHub OAuth login and encrypted credential storage.',
      'output: login status; --json returns structured result.',
    ],
    options: [
      {
        name: 'hostname',
        hasValue: true,
        description: 'GitHub Enterprise hostname',
      },
      {
        name: 'git-protocol',
        hasValue: true,
        description: 'Git protocol: ssh or https',
      },
      {
        name: 'force',
        description: 'Re-authenticate even if already logged in',
      },
      { name: 'json', description: 'Output result as JSON' },
    ],
  },
  {
    name: 'logout',
    description: 'Sign out from GitHub',
    usage: 'octocode logout [--hostname <host>] [--yes] [--json]',
    scheme: [
      'args: none.',
      'options: --hostname GitHub Enterprise host, --yes boolean skips confirmation, --json boolean.',
      'runtime: removes encrypted Octocode credentials for the host.',
      'output: logout status; --json returns structured result.',
    ],
    options: [
      {
        name: 'hostname',
        hasValue: true,
        description: 'GitHub Enterprise hostname',
      },
      { name: 'yes', description: 'Skip confirmation prompt' },
      { name: 'json', description: 'Output result as JSON' },
    ],
  },
  {
    name: 'skills',
    description:
      'Search, install, and manage Octocode skills across AI clients',
    usage:
      'octocode skills [search|read|install|remove|list|sync] [--skill <name>] [--targets <list>] [--mode <copy|symlink>] [--json]',
    scheme: [
      'arg[0] action: optional enum(search|read|install|remove|list|sync).',
      'skill selectors: --skill name, --local path, --target single target, --targets comma-list.',
      'install options: --mode enum(copy|symlink), --force boolean, --dry-run boolean.',
      'search/read options: --limit int, --full boolean, --direct boolean, --install boolean.',
      'runtime: skills marketplace plus local skill target installation/removal/sync.',
      'output: skill search/list/read/install status; --json returns structured result.',
    ],
    options: [
      { name: 'force', description: 'Overwrite existing skills' },
      { name: 'skill', hasValue: true, description: 'Skill folder name' },
      {
        name: 'local',
        hasValue: true,
        description: 'Path to a local skill folder',
      },
      {
        name: 'targets',
        hasValue: true,
        description: 'Comma-separated targets',
      },
      {
        name: 'mode',
        hasValue: true,
        description: 'Install mode: copy or symlink',
      },
      { name: 'limit', hasValue: true, description: 'Max search results' },
      { name: 'full', description: 'Show full SKILL.md' },
      { name: 'direct', description: 'Search skills.sh directly' },
      {
        name: 'target',
        hasValue: true,
        description: 'Filter list to one target',
      },
      { name: 'install', description: 'Install the top search result' },
      { name: 'dry-run', description: 'Show plan without writing' },
      { name: 'json', description: 'Output as JSON' },
    ],
  },
  {
    name: 'token',
    description: 'Print the GitHub token',
    usage:
      'octocode token [--type <auto|octocode|gh>] [--hostname <host>] [--source] [--validate] [--reveal] [--json]',
    scheme: [
      'args: none.',
      'options: --type enum(auto|octocode|gh, default auto), --hostname GitHub Enterprise host, --source boolean, --validate boolean, --reveal boolean, --json boolean.',
      'runtime: resolves token from env -> Octocode encrypted storage -> gh CLI according to --type.',
      'output: redacted token by default; --reveal prints the full token; --json returns structured result.',
    ],
    options: [
      {
        name: 'type',
        hasValue: true,
        description: 'Token source: auto, octocode, gh',
      },
      {
        name: 'hostname',
        hasValue: true,
        description: 'GitHub Enterprise hostname',
      },
      { name: 'source', description: 'Show token source and user info' },
      { name: 'validate', description: 'Verify the token with GitHub API' },
      { name: 'reveal', description: 'Print the full token on screen' },
      { name: 'json', description: 'Output as JSON' },
    ],
  },
  {
    name: 'status',
    description: 'Show Octocode health status',
    usage: 'octocode status [--hostname <host>] [--sync] [--json]',
    scheme: [
      'args: none.',
      'options: --hostname GitHub Enterprise host, --sync boolean includes MCP sync analysis, --json boolean.',
      'runtime: checks auth, installation/cache health, and optional MCP sync state.',
      'output: health summary; --json returns structured status.',
    ],
    options: [
      {
        name: 'hostname',
        hasValue: true,
        description: 'GitHub Enterprise hostname',
      },
      { name: 'sync', description: 'Include MCP sync analysis' },
      { name: 'json', description: 'Output as JSON' },
    ],
  },
];

export function findCommandSpec(name: string): CLICommandSpec | undefined {
  return COMMAND_SPECS.find(command => command.name === name);
}
