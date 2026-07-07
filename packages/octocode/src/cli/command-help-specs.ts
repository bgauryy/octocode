import { findCommandSpec } from './commands/specs.js';
import type { CLICommandSpec } from './types.js';

const SKILL_SPEC_OPTION_PATCHES: Record<
  string,
  { description: string; hasValue?: boolean }
> = {
  add: {
    hasValue: true,
    description:
      'GitHub skill path URL or owner/repo/path shorthand; a library path installs every direct child skill folder',
  },
  platform: {
    hasValue: true,
    description:
      'Comma-separated install targets — common (default), cursor, claude, codex, opencode, pi, copilot, gemini, all',
  },
  mode: {
    hasValue: true,
    description:
      'Install mode: symlink (default, linked to ~/.octocode/skills), copy, or hybrid (copy for claude, symlink for others)',
  },
  update: {
    description:
      'Alias of --force for replacing existing destination folders or links; source cache refreshes every install',
  },
  verbose: {
    description: 'Show source URL in addition to source and destination paths',
  },
  json: {
    description:
      'Output non-redundant JSON: skills[], platforms, mode, and aggregate summary',
  },
};

const SKILL_SPEC_EXTRA_OPTIONS = [
  {
    name: 'install-all',
    description: 'Install every current official Octocode skill',
  },
  {
    name: 'all-skills',
    description: 'Alias of --install-all',
  },
];

function patchSkillCommandSpec(spec: CLICommandSpec): CLICommandSpec {
  const existingOptions = spec.options ?? [];
  const patchedOptions = existingOptions.map(option => {
    const patch = SKILL_SPEC_OPTION_PATCHES[option.name];
    return patch ? { ...option, ...patch } : option;
  });
  const optionNames = new Set(patchedOptions.map(option => option.name));
  const options = [
    ...patchedOptions,
    ...SKILL_SPEC_EXTRA_OPTIONS.filter(option => !optionNames.has(option.name)),
  ];

  return {
    ...spec,
    usage:
      'skill --list\nskill (--add <github-path> | --name <octocode-skill> | --install-all) [--platform common|cursor|claude|codex|opencode|pi|copilot|gemini|all] [--mode symlink|copy|hybrid] [--force|--update] [--dry-run] [--verbose] [--branch <ref>] [--json]',
    scheme: [
      ...(spec.scheme ?? []),
      '--install-all installs every current official Octocode skill; --all-skills is an alias.',
      'additional platforms: copilot and gemini are accepted by this CLI build.',
    ],
    whenToUse: [
      ...(spec.whenToUse ?? []),
      'Use --install-all to install every current official Octocode skill without a shell loop.',
    ],
    examples: [
      'skill --list',
      'skill --name octocode-research',
      'skill --name octocode-research --platform codex',
      'skill --name octocode-research --platform claude',
      'skill --name octocode-research --platform copilot,gemini',
      'skill --name octocode-research --platform all --dry-run',
      'skill --install-all --platform pi',
      'skill --add owner/repo/skills/code-review --platform cursor,codex',
      'skill --add owner/repo/skills --platform common',
      'skill --add https://github.com/owner/repo/blob/main/skills/code-review/SKILL.md --platform claude --json',
    ],
    options,
  };
}

export function findStaticCommandHelp(
  name: string
): CLICommandSpec | undefined {
  const spec = findCommandSpec(name);
  if (!spec) {
    return undefined;
  }

  return name === 'skill' ? patchSkillCommandSpec(spec) : spec;
}
