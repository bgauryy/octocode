import type { CLICommandSpec, CLIOption } from '../types.js';

const flag = (
  name: string,
  description: string,
  hasValue = false
): CLIOption => ({
  name,
  description,
  ...(hasValue ? { hasValue: true } : {}),
});

export const COMMAND_SPECS: readonly CLICommandSpec[] = [
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
      flag('mode', 'Install mode: symlink, copy, or auto', true),
      flag('force', 'Replace differing destinations'),
      flag(
        'upgrade',
        'Refresh bundled content while preserving destination drift'
      ),
      flag('global', 'Install platform links in user scope'),
      flag('project-dir', 'Install platform links in project scope', true),
      flag('workspace', 'Also check the workspace (check only)'),
      flag('path', 'Custom destination', true),
      flag('dry-run', 'Preview without writing'),
      flag('fix', 'Repair missing installs'),
      flag('no-env', 'Skip environment checks'),
      flag('json', 'Output JSON'),
    ],
  },
];

export function findCommandSpec(name: string): CLICommandSpec | undefined {
  return COMMAND_SPECS.find(command => command.name === name);
}
