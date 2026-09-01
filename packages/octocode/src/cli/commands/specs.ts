import type { CLICommandSpec, CLIOption } from '../types.js';

const flag = (
  name: string,
  description: string,
  hasValue = false,
  defaultValue?: string
): CLIOption => ({
  name,
  description,
  ...(hasValue ? { hasValue: true } : {}),
  ...(defaultValue === undefined ? {} : { default: defaultValue }),
});

const SPECS: readonly CLICommandSpec[] = [
  {
    name: 'cache',
    description: 'Materialize GitHub content for local-tool research',
    usage: 'cache <fetch|status|clear> [owner/repo] [path] [options]',
    scheme: [
      'fetch reads a file/tree or clone; status inspects caches; clear requires --clone, --tree, or --all.',
    ],
    whenToUse: ['Use before local analysis of a remote repository.'],
    examples: [
      'cache fetch facebook/react README.md --depth file',
      'cache status',
    ],
    options: [
      flag('depth', 'Fetch depth: file, tree, or clone', true),
      flag('branch', 'Branch, tag, or SHA', true),
      flag('force-refresh', 'Bypass cached clone state'),
      flag('clone', 'Clear clone cache'),
      flag('tree', 'Clear tree cache'),
      flag('all', 'Clear all temporary caches'),
      flag('json', 'Output JSON'),
    ],
  },
  {
    name: 'context',
    description: 'Print token-sized agent protocol and tool guidance',
    usage: 'context [--full|--minimal] [--json]',
    scheme: [
      'Outputs the agent protocol, output semantics, tool descriptions, and on-demand schema commands.',
    ],
    whenToUse: [
      'Use --minimal for the cheapest orientation; --full adds the shared prompt and descriptions.',
    ],
    examples: ['context --minimal', 'context --full', 'context --json'],
    options: [
      flag('full', 'Include the shared prompt and full descriptions'),
      flag('minimal', 'Print only protocol, commands, output mode, and names'),
      flag('json', 'Output JSON'),
    ],
  },
  {
    name: 'install',
    description: 'Install octocode-mcp for an IDE',
    usage:
      'install --ide <ide> [--method npx] [--force] [--check] [--rollback] [--backup-path <path>] [--json]',
    scheme: [
      'required option: --ide supported client id; --check validates without writing.',
    ],
    whenToUse: ['Configure or validate an MCP client installation.'],
    examples: ['install --ide cursor', 'install --ide claude-code --check'],
    options: [
      flag('ide', 'IDE to configure', true),
      flag('method', 'Installation method', true, 'npx'),
      flag('force', 'Overwrite existing configuration'),
      flag('check', 'Pre-flight only'),
      flag('rollback', 'Restore the latest backup'),
      flag('backup-path', 'Backup file to restore', true),
      flag('json', 'Output JSON'),
    ],
  },
  {
    name: 'auth',
    description: 'Manage GitHub authentication',
    usage:
      'auth [login|logout|refresh|status] [--hostname <host>] [--git-protocol <ssh|https>] [--force] [--yes] [--json]',
    scheme: ['The optional action defaults to interactive auth management.'],
    whenToUse: ['Use auth status --json for a narrow non-interactive check.'],
    examples: ['auth status --json', 'auth login'],
    options: [
      flag('hostname', 'GitHub Enterprise hostname', true),
      flag('git-protocol', 'Git protocol: ssh or https', true),
      flag('force', 'Re-authenticate'),
      flag('status', 'Auth-only status probe'),
      flag('yes', 'Skip confirmation'),
      flag('json', 'Output JSON'),
    ],
  },
  {
    name: 'login',
    description: 'Authenticate with GitHub',
    usage: 'login [options]',
    scheme: ['Runs GitHub OAuth and stores encrypted credentials.'],
    whenToUse: [
      'Humans can store credentials; agents should prefer token environment variables.',
    ],
    examples: ['login', 'login --force'],
    options: [
      flag('hostname', 'GitHub Enterprise hostname', true),
      flag('git-protocol', 'Git protocol: ssh or https', true),
      flag('force', 'Re-authenticate'),
      flag('json', 'Output JSON'),
    ],
  },
  {
    name: 'logout',
    description: 'Sign out from GitHub',
    usage: 'logout [options]',
    scheme: ['Removes stored Octocode credentials for the host.'],
    whenToUse: ['Clear credentials before switching accounts.'],
    examples: ['logout --yes'],
    options: [
      flag('hostname', 'GitHub Enterprise hostname', true),
      flag('yes', 'Skip confirmation'),
      flag('json', 'Output JSON'),
    ],
  },
  {
    name: 'status',
    description: 'Show Octocode health status',
    usage: 'status [options]',
    scheme: ['Checks auth, installation, cache health, and optional MCP sync.'],
    whenToUse: ['Diagnose setup before research.'],
    examples: ['status', 'status --sync'],
    options: [
      flag('hostname', 'GitHub Enterprise hostname', true),
      flag('sync', 'Include MCP sync analysis'),
      flag('json', 'Output JSON'),
    ],
  },
  {
    name: 'lsp-server',
    description: 'Manage language servers for semantic research',
    usage:
      'lsp-server <list|status|install|uninstall|clean> [name...] [options]',
    scheme: ['The positional subcommand selects inspection or provisioning.'],
    whenToUse: ['Diagnose or provision a missing semantic provider.'],
    examples: [
      'lsp-server status src/main.rs',
      'lsp-server install rust-analyzer',
    ],
    options: [
      flag('all', 'Select every downloadable server'),
      flag('force', 'Re-download managed state'),
      flag('yes', 'Allow non-interactive download'),
      flag('platform', 'Target platform id', true),
      flag('json', 'Output JSON'),
    ],
  },
  {
    name: 'skill',
    description: 'List, install, inspect, verify, or remove Agent Skills',
    usage: 'skill <list|install|remove|check|info> [options]',
    scheme: [
      'Use bundled names positionally or --add for a local/GitHub source.',
    ],
    whenToUse: ['Install or verify Octocode workflow skills.'],
    examples: ['skill list', 'skill check octocode-research --fix'],
    options: [
      flag('add', 'Local path or GitHub skill source', true),
      flag('platform', 'Comma-separated install targets', true),
      flag('all', 'Select every bundled skill'),
      flag('mode', 'Install mode', true),
      flag('keep', 'Preserve existing destinations'),
      flag('workspace', 'Also install into the workspace'),
      flag('path', 'Custom destination', true),
      flag('dry-run', 'Preview without writing'),
      flag('fix', 'Repair missing installs'),
      flag('no-env', 'Skip environment checks'),
      flag('json', 'Output JSON'),
    ],
  },
];

export const COMMAND_SPECS: readonly CLICommandSpec[] = SPECS;

export function findCommandSpec(name: string): CLICommandSpec | undefined {
  return COMMAND_SPECS.find(command => command.name === name);
}
