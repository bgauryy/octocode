import type { ParsedArgs } from '../types.js';
import { OPTIONS_WITH_VALUES } from '../parser.js';
import { EXIT } from '../exit-codes.js';
import { reportFailure } from './skills/commands/fail.js';
import { getBool, getString } from '../options.js';
import { runList } from './skills/commands/list.js';
import { runInstall, type InstallOptions } from './skills/commands/install.js';
import { runRemove } from './skills/commands/remove.js';
import { runInfo } from './skills/commands/info.js';
import { runCheck } from './skills/commands/check.js';
import {
  formatSkillPlatformHelp,
  type SkillInstallMode,
} from '@octocodeai/octocode-skill-installer';

const SUBCOMMANDS = new Set(['list', 'install', 'remove', 'check', 'info']);

function printBundledSkillHelp(): void {
  console.log(`Install, remove, or check bundled Octocode skills

Usage: octocode skill <COMMAND> [OPTIONS]

Commands:
  list                 Bundled skills with install status and platform links
  info <name>          A skill's SKILL.md and env readiness
  install <name>...    Install skills (--all for every bundled skill)
  install --add <dir>  Add a local skill to the canonical home
  remove <name>...     Remove skills: home copy and platform links (--all for every skill)
  check [<name>...]    Verify installs, platform links, and env readiness

Options:
      --platform <P>      Platforms, comma-separated: ${formatSkillPlatformHelp()}
      --global            install: link into each platform's global scope
      --project-dir <DIR> install: link into each platform's project scope
      --path <DIR>        install: copy straight to a custom destination
      --mode <MODE>       install: symlink (default), copy, or auto
      --force             install: replace a differing install; remove: also delete real directories
      --upgrade           install: refresh changed bundled content, keep destination edits
      --workspace         check: also check <cwd>/.agents/skills
      --fix               check: refresh stale or broken installs in place
      --no-env            check: skip env readiness
      --dry-run           install, remove, check --fix: preview without writing
      --json              Print JSON (errors too)
  -h, --help              Print help

Examples:
  octocode skill install --all --platform claude,cursor --global
  octocode skill install --add ./skills/my-skill --platform claude --global
  octocode skill check --fix`);
}

/** Flags each subcommand reads; any other skill flag is a usage error there. */
const SUBCOMMAND_FLAGS: Record<string, readonly string[]> = {
  list: ['json'],
  info: ['json'],
  check: ['platform', 'workspace', 'fix', 'dry-run', 'no-env', 'json'],
  install: [
    'add',
    'platform',
    'all',
    'mode',
    'force',
    'upgrade',
    'global',
    'project-dir',
    'path',
    'dry-run',
    'json',
  ],
  remove: ['all', 'platform', 'force', 'dry-run', 'json'],
};
/** Flags every subcommand accepts. */
const GLOBAL_FLAGS = ['help'];

function editDistance(left: string, right: string): number {
  let previous = Array.from({ length: right.length + 1 }, (_, i) => i);
  for (let i = 1; i <= left.length; i++) {
    const current = [i];
    for (let j = 1; j <= right.length; j++) {
      current[j] = Math.min(
        previous[j]! + 1,
        current[j - 1]! + 1,
        previous[j - 1]! + (left[i - 1] === right[j - 1] ? 0 : 1)
      );
    }
    previous = current;
  }
  return previous[right.length]!;
}

/**
 * The usage error for the first flag `command` does not read: a flag of
 * another subcommand names where it applies; an unknown flag names the
 * nearest flag `command` accepts.
 */
function unknownFlagError(
  command: string,
  options: ParsedArgs['options']
): string | undefined {
  const accepted = [...(SUBCOMMAND_FLAGS[command] ?? []), ...GLOBAL_FLAGS];
  const flag = Object.keys(options).find(key => !accepted.includes(key));
  if (flag === undefined) return undefined;
  const owners = Object.entries(SUBCOMMAND_FLAGS)
    .filter(([, flags]) => flags.includes(flag))
    .map(([name]) => name);
  if (owners.length > 0) {
    return `Unknown option for skill ${command}: --${flag} (it applies to skill ${owners.join(', ')})`;
  }
  const nearest = accepted
    .map(name => ({ name, distance: editDistance(flag, name) }))
    .filter(({ distance }) => distance <= 2)
    .sort((a, b) => a.distance - b.distance)[0];
  return `Unknown option: --${flag}${nearest ? ` (did you mean --${nearest.name}?)` : ''}`;
}

function subcommand(args: ParsedArgs): string | undefined {
  return args.args[0];
}

function positionalAfterSubcommand(args: ParsedArgs): string[] {
  const first = args.args[0];
  return first && SUBCOMMANDS.has(first) ? args.args.slice(1) : args.args;
}

function platformOption(args: ParsedArgs): string | null {
  return getString(args.options, 'platform');
}

function installMode(args: ParsedArgs): SkillInstallMode {
  const rawMode = getString(args.options, 'mode');
  return rawMode === 'copy' || rawMode === 'auto' ? rawMode : 'symlink';
}

function installNames(args: ParsedArgs): string[] {
  return positionalAfterSubcommand(args).filter(a => !a.startsWith('-'));
}

export const skillCommand = {
  name: 'skill',
  handler: (args: ParsedArgs) => {
    const json = getBool(args.options, 'json');
    const fail = (message: string): void => {
      reportFailure(message, json);
      process.exitCode = EXIT.USAGE;
    };
    const command = subcommand(args);
    if (getBool(args.options, 'help') || command === undefined) {
      printBundledSkillHelp();
      if (command === undefined && !getBool(args.options, 'help'))
        process.exitCode = EXIT.USAGE;
      return;
    }
    if (!SUBCOMMANDS.has(command)) {
      return fail(
        `Unknown skill command: "${command}". Commands: ${[...SUBCOMMANDS].join(', ')}.`
      );
    }
    const flagError = unknownFlagError(command, args.options);
    if (flagError) return fail(flagError);
    const missingValue = Object.keys(args.options).find(
      key =>
        OPTIONS_WITH_VALUES.has(key) && typeof args.options[key] !== 'string'
    );
    if (missingValue) return fail(`--${missingValue} requires a value.`);
    if (
      args.options.mode !== undefined &&
      !['copy', 'symlink', 'auto'].includes(String(args.options.mode))
    ) {
      return fail('--mode expects copy|symlink|auto.');
    }

    switch (command) {
      case 'list':
        runList({ json });
        return;

      case 'info': {
        const skillName = positionalAfterSubcommand(args)[0];
        if (!skillName) {
          const msg = 'Usage: octocode skill info <skill-name>';
          fail(msg);
          return;
        }
        runInfo(skillName, { json });
        return;
      }

      case 'check':
        runCheck({
          names: positionalAfterSubcommand(args).filter(
            a => !a.startsWith('-')
          ),
          platform: platformOption(args),
          workspace: getBool(args.options, 'workspace'),
          fix: getBool(args.options, 'fix'),
          dryRun: getBool(args.options, 'dry-run'),
          noEnv: getBool(args.options, 'no-env'),
          json,
        });
        return;

      case 'install': {
        const addSource = getString(args.options, 'add');
        const addLocal = Boolean(addSource);
        const installAll = getBool(args.options, 'all');
        const rawPath = getString(args.options, 'path') || null;
        const opts: InstallOptions = {
          all: installAll,
          sourcePath: addLocal ? addSource : null,
          platform: platformOption(args),
          global: getBool(args.options, 'global'),
          projectDir: getString(args.options, 'project-dir') || null,
          customPath: addLocal ? null : rawPath,
          mode: installMode(args),
          force: getBool(args.options, 'force'),
          upgrade: getBool(args.options, 'upgrade'),
          dryRun: getBool(args.options, 'dry-run'),
          json,
        };
        runInstall(installNames(args), opts);
        return;
      }

      case 'remove':
        runRemove(
          positionalAfterSubcommand(args).filter(a => !a.startsWith('-')),
          {
            all: getBool(args.options, 'all'),
            platform: platformOption(args),
            dryRun: getBool(args.options, 'dry-run'),
            force: getBool(args.options, 'force'),
            json,
          }
        );
        return;
    }
  },
};
