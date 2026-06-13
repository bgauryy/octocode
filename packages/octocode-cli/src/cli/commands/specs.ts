import type { CLICommandSpec } from '../types.js';

export const COMMAND_SPECS: readonly CLICommandSpec[] = [
  {
    name: 'get',
    description:
      'Fetch and minify file content for local paths and GitHub references',
    usage:
      'octocode get <path|github-ref> [--mode none|standard|symbols] [--type <ext>] [--branch <ref>] [--match-string <s>] [--start-line <n>] [--end-line <n>] [--page-size <n>] [--page <n>] [--stats] [--json]',
    options: [
      {
        name: 'mode',
        hasValue: true,
        description: 'Minification mode: standard (default), symbols, none',
      },
      {
        name: 'type',
        hasValue: true,
        description: 'Language hint; overrides auto-detection',
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
      {
        name: 'start-line',
        hasValue: true,
        description: 'First line to return, 1-based (GitHub only)',
      },
      {
        name: 'end-line',
        hasValue: true,
        description: 'Last line to return, 1-based (GitHub only)',
      },
      {
        name: 'page-size',
        hasValue: true,
        description: 'Characters per page for GitHub file reads',
      },
      {
        name: 'page',
        hasValue: true,
        description: 'GitHub file page number when pagination is available',
      },
      { name: 'stats', description: 'Print size-reduction statistics' },
      { name: 'json', description: 'Output as JSON' },
    ],
  },
  {
    name: 'tree',
    description:
      'View directory structure for local paths and GitHub repositories',
    usage:
      'octocode tree <path|github-ref> [--depth <n>] [--branch <ref>] [--json]',
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
    description: 'Search code in local paths and GitHub repositories',
    usage:
      'octocode search <pattern> <path|github-ref> [--type <ext>] [--limit <n>] [--page <n>] [--page-size <n>] [--json]',
    options: [
      {
        name: 'type',
        hasValue: true,
        description: 'Filter by language or extension',
      },
      { name: 'limit', hasValue: true, description: 'Max files to show' },
      { name: 'page', hasValue: true, description: 'Result page to fetch' },
      { name: 'page-size', hasValue: true, description: 'Results per page' },
      {
        name: 'branch',
        hasValue: true,
        description: 'Branch or ref for GitHub paths',
      },
      { name: 'json', description: 'Output raw JSON results' },
    ],
  },
  {
    name: 'pr',
    description:
      'Search and view pull requests; list with filters or deep-dive one PR',
    usage:
      'octocode pr <owner/repo[#N] | PR-URL> [--pr <n>] [--state open|closed|merged] [--patches] [--comments] [--commits] [--deep] [--json]',
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
    name: 'pkg',
    description: 'Research an npm package and its source repository',
    usage: 'octocode pkg <package> [--page <n>] [--json]',
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
    description: 'Run LSP semantic research for a local source file',
    usage:
      'octocode lsp <file> --type <type> [--symbol <name>] [--line <n>] [--page <n>] [--page-size <n>] [--json]',
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
        description: 'LSP format: structured or compact',
      },
      { name: 'json', description: 'Output raw JSON results' },
    ],
  },
  {
    name: 'symbols',
    description: 'Show a semantic symbol outline for a local file or directory',
    usage:
      'octocode symbols <file|path> [--ext <list>] [--kind <kind>] [--limit <n>] [--depth <n>] [--json]',
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
    name: 'install',
    description: 'Install octocode-mcp for an IDE',
    usage: 'octocode install --ide <ide> [--method npx] [--force] [--json]',
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
    usage: 'octocode auth [login|logout|status|token|refresh] [--json]',
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
      'octocode token [--type <auto|octocode|gh>] [--hostname <host>] [--source] [--validate] [--json]',
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
