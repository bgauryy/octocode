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
    description:
      'Install one GitHub Agent Skill folder, every skill in a GitHub skills library path, one named Octocode skill, or every official Octocode skill into supported local skill directories.',
    usage:
      'skill --list\nskill (--add <github-path> | --name <octocode-skill> | --install-all) [--platform common|cursor|claude|codex|opencode|pi|copilot|gemini|all] [--mode symlink|copy|hybrid] [--force|--update] [--dry-run] [--verbose] [--branch <ref>] [--json]',
    scheme: [
      'required source: pass exactly one of --add <github-path>, --name <octocode-skill>, or --install-all. --add accepts a GitHub skill folder URL or owner/repo/path shorthand; a library path such as owner/repo/skills installs every direct child skill folder with SKILL.md.',
      '--name must be a safe Octocode skill folder name such as octocode-research. Run --list to see the live named-skill catalog.',
      '--platform (alias --target, default: common) accepts comma-separated values: common, cursor, claude, codex, opencode, pi, copilot, gemini, or all. --all is shorthand for --platform all.',
      'platform paths: common -> ~/.agents/skills, cursor -> ~/.cursor/skills, claude -> ~/.claude/skills plus ~/.claude-desktop/skills, codex -> ~/.agents/skills, opencode -> ~/.config/opencode/skills, pi -> ~/.pi/agent/skills, copilot -> ~/.copilot/skills, gemini -> ~/.gemini/skills. Windows uses platform-appropriate home/AppData paths.',
      'install mode: symlink (default) refreshes ~/.octocode/skills/<skill> and links selected clients to that canonical source. copy duplicates the refreshed source into each destination. hybrid copies Claude targets and symlinks the rest.',
      '--force (alias --update) replaces existing destination folders or links. The canonical source in ~/.octocode/skills refreshes on every install attempt.',
      '--dry-run previews source and destination paths without fetching or writing anything.',
      'output: human output prints mode, platforms, canonical source path, every destination path, and a final summary. --json returns skills[] entries plus top-level platforms, mode, and aggregate summary.',
    ],
    whenToUse: [
      'Use --list to discover all available Octocode named skills before installing.',
      'Use --name for Octocode-maintained skills like octocode-research; use --add for arbitrary GitHub skill folders or GitHub skills libraries.',
      'Omit --platform to install to the default shared location (~/.agents/skills); use --platform all for every supported local skill destination.',
      'Use --install-all to install every current official Octocode skill without a shell loop.',
      'Use --dry-run before broad installs or forced overwrites.',
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
