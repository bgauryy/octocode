import { findCommandSpec } from './commands/specs.js';
import type { CLICommandSpec } from './types.js';

const SKILL_SPEC_OPTION_PATCHES: Record<
  string,
  { description: string; hasValue?: boolean }
> = {
  add: {
    hasValue: true,
    description:
      'GitHub skill path URL or owner/repo/path shorthand; combine with --path for a local skill folder or local skills library',
  },
  name: {
    hasValue: true,
    description:
      'Official Octocode skill name; checks bundled package skills first, then falls back to GitHub',
  },
  path: {
    hasValue: true,
    description:
      'Local skill folder, SKILL.md path, or skills library path; use with --add when the agent already knows the bundled skill location',
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
    name: 'path',
    hasValue: true,
    description: SKILL_SPEC_OPTION_PATCHES.path.description,
  },
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
  const scheme = (spec.scheme ?? []).map(line => {
    if (line.startsWith('required source:')) {
      return 'required source: pass exactly one of --add <github-folder>, --add --path <local-skill-or-skills-dir>, --name <octocode-skill>, or --install-all. --name checks bundled skills first, then falls back to GitHub.';
    }
    if (line.startsWith('accepted --add forms')) {
      return 'accepted --add forms include GitHub URLs/shorthands, or --add --path pointing at a local skill folder, SKILL.md, or direct-child skills library.';
    }
    if (line.startsWith('--name must')) {
      return '--name must be a safe official skill name such as octocode-awareness or octocode-research. Run with --list to see available Octocode skill names.';
    }
    if (line.startsWith('--platform')) {
      return '--platform (alias --target, default: common) accepts comma-separated values: common (-> ~/.agents/skills), cursor (-> ~/.cursor/skills), claude (-> Claude Code + Claude Desktop skill folders), codex (-> ~/.agents/skills), opencode (-> ~/.config/opencode/skills), pi (-> ~/.pi/agent/skills), copilot, gemini, or all. --all is shorthand for --platform all.';
    }
    if (line.startsWith('platform paths:')) {
      return 'platform paths: common/codex -> ~/.agents/skills, cursor -> ~/.cursor/skills, opencode -> ~/.config/opencode/skills, pi -> ~/.pi/agent/skills, claude -> Claude Code plus Claude Desktop skill folders. Windows uses platform-appropriate AppData paths.';
    }
    if (line.startsWith('install mode:')) {
      return 'install mode: symlink (default) links each target to the Octocode-managed source cache; copy embeds a standalone copy; hybrid uses copy for claude targets and symlink for everything else.';
    }
    if (line.startsWith('runtime:')) {
      return "runtime: validates the source, refreshes Octocode's skill-sources cache, then installs to the selected platform destinations. Bundled/local sources avoid GitHub.";
    }
    if (line.startsWith('output:')) {
      return 'output: human summary by default; --json returns success, skills[], platforms, mode, and installed/skipped/failed counts.';
    }
    return line;
  });

  return {
    ...spec,
    usage:
      'skill --list\nskill (--add <github-path> | --add --path <local-skill-or-skills-dir> | --name <octocode-skill> | --install-all) [--platform common|cursor|claude|codex|opencode|pi|copilot|gemini|all] [--mode symlink|copy|hybrid] [--force|--update] [--dry-run] [--verbose] [--branch <ref>] [--json]',
    scheme: [
      ...scheme,
      '--install-all installs every current official Octocode skill; --all-skills is an alias.',
      '--add --path <local-skill-or-skills-dir> installs from a local skill folder, SKILL.md, or direct-child skills library without resolving GitHub.',
      'additional platforms: copilot and gemini are accepted by this CLI build.',
    ],
    whenToUse: [
      ...(spec.whenToUse ?? []).map(line =>
        line.includes('octocode-engineer')
          ? 'Use --name for official Octocode skills like octocode-awareness or octocode-research; use --add for GitHub skill folders; use --add --path when the agent already knows a local bundled skill location.'
          : line
      ),
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
      'skill --add --path /path/to/skills/octocode-awareness --platform common',
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
