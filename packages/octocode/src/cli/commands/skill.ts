import type { CLICommand, ParsedArgs } from '../types.js';
import { EXIT } from '../exit-codes.js';
import { getBool, getString } from '../options.js';
import { c, bold, dim } from '../../utils/colors.js';
import { runList } from './skills/commands/list.js';
import { runInstall, type InstallOptions } from './skills/commands/install.js';
import { runRemove } from './skills/commands/remove.js';
import { runInfo } from './skills/commands/info.js';
import { runCheck } from './skills/commands/check.js';
import type { InstallMode } from './skills/installer.js';

const SUBCOMMANDS = new Set([
  'list',
  'install',
  'remove',
  'check',
  'info',
  'help',
]);

function printBundledSkillHelp(): void {
  console.log(`
${bold('octocode skill')} — bundled Octocode skills

${bold('Usage')}
  octocode skill <command> [options]

${bold('Commands')}
  list                    List bundled skills with install/env status
  install <name>...       Install one or more bundled skills  ${dim('(override by default)')}
  --add <source>          Add to canonical home + link into ~/.agents/skills
  remove  <name>...       Remove a skill — home copy + platform links
  check  [<name>...]      Verify installs, platform links, and env readiness
  info   <name>           Show full SKILL.md content

${bold('Install options')}
  --all                   Install all bundled skills
  --platform <p>          Link into platform dir  ${dim('(comma-sep: pi | cursor | claude | claude-desktop | codex | codex-native | opencode | copilot | gemini | common | all)')}
  --workspace, --repo     Also link into <cwd>/.agents/skills/
  --path <dir>            Install bundled skill directly to a custom destination
  --mode copy|symlink|hybrid  ${dim('[default: symlink · hybrid = copy for claude]')}
  --keep                  Preserve existing  ${dim('[default: override]')}
  --dry-run               Preview without writing

${bold('Remove options')}
  --all                   Remove all installed skills
  --platform <p>          Remove only specified platform link(s)  ${dim('(home kept)')}
  --dry-run               Preview without deleting

${bold('Check options')}
  --platform <p>          Check specific platforms only
  --workspace             Also check <cwd>/.agents/skills
  --fix                   Re-install missing/broken locations automatically
  --no-env                Skip env param checks

${bold('Global flags')}
  --json                  Machine-readable JSON output
  --help                  Show this help

${bold('Examples')}
  octocode skill list --json
  octocode skill install --all --platform pi,cursor
  octocode skill --add ./skills/my-skill --platform claude,cursor,codex-native
  octocode skill install octocode-research --workspace --keep
  octocode skill remove octocode-research --platform pi
  octocode skill check --fix
  octocode skill info octocode-research
`);
}

function subcommand(args: ParsedArgs): string {
  const first = args.args[0];
  if (first && SUBCOMMANDS.has(first)) return first;
  if (getBool(args.options, 'list')) return 'list';
  if (getString(args.options, 'add') || getBool(args.options, 'add'))
    return 'install';
  if (
    getString(args.options, 'name') ||
    getBool(args.options, 'install-all') ||
    getBool(args.options, 'all-skills')
  ) {
    return 'install';
  }
  return first ?? 'help';
}

function positionalAfterSubcommand(args: ParsedArgs): string[] {
  const first = args.args[0];
  return first && SUBCOMMANDS.has(first) ? args.args.slice(1) : args.args;
}

function platformOption(args: ParsedArgs): string | null {
  return (
    getString(args.options, 'platform') ?? getString(args.options, 'target')
  );
}

function installMode(args: ParsedArgs): InstallMode {
  const rawMode = getString(args.options, 'mode');
  return rawMode === 'copy' || rawMode === 'hybrid' ? rawMode : 'symlink';
}

function installNames(args: ParsedArgs): string[] {
  const names = positionalAfterSubcommand(args).filter(a => !a.startsWith('-'));
  const named = getString(args.options, 'name');
  return named ? [...names, named] : names;
}

export const skillCommand: CLICommand = {
  name: 'skill',
  options: [
    // Legacy/core-documented flags kept so existing `octocode skill --name ...` still routes.
    { name: 'add', hasValue: true },
    { name: 'name', hasValue: true },
    { name: 'list' },
    { name: 'platform', hasValue: true },
    { name: 'target', hasValue: true },
    { name: 'all' },
    { name: 'mode', hasValue: true, default: 'symlink' },
    { name: 'force' },
    { name: 'update' },
    { name: 'dry-run' },
    { name: 'verbose' },
    { name: 'branch', hasValue: true },
    { name: 'json' },
    { name: 'install-all' },
    { name: 'all-skills' },
    // Bundled-skill subcommand flags.
    { name: 'keep' },
    { name: 'workspace' },
    { name: 'repo' },
    { name: 'path', hasValue: true },
    { name: 'fix' },
    { name: 'no-env' },
  ],
  handler: (args: ParsedArgs) => {
    const json = getBool(args.options, 'json');
    const command = subcommand(args);

    switch (command) {
      case 'list':
        runList({ json });
        return;

      case 'info': {
        const skillName = positionalAfterSubcommand(args)[0];
        if (!skillName) {
          const msg = 'Usage: octocode skill info <skill-name>';
          if (json) console.log(JSON.stringify({ success: false, error: msg }));
          else console.error(`\n  ${c('red', '✗')} ${msg}\n`);
          process.exitCode = EXIT.USAGE;
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
          workspace:
            getBool(args.options, 'workspace') || getBool(args.options, 'repo'),
          fix: getBool(args.options, 'fix'),
          noEnv: getBool(args.options, 'no-env'),
          json,
        });
        return;

      case 'install': {
        const addSource = getString(args.options, 'add');
        const addLocal = Boolean(addSource) || getBool(args.options, 'add');
        const installAll =
          getBool(args.options, 'all') ||
          getBool(args.options, 'install-all') ||
          getBool(args.options, 'all-skills');
        const rawPath = getString(args.options, 'path');
        if (addLocal && !addSource && !rawPath) {
          const error = '--add requires <source>.';
          if (json) console.log(JSON.stringify({ success: false, error }));
          else console.error(`\n  ${c('red', '✗')} ${error}\n`);
          process.exitCode = EXIT.USAGE;
          return;
        }
        const opts: InstallOptions = {
          all: installAll,
          sourcePath: addLocal ? addSource || rawPath : null,
          platform: platformOption(args),
          workspace:
            getBool(args.options, 'workspace') || getBool(args.options, 'repo'),
          customPath: addLocal ? null : rawPath,
          mode: installMode(args),
          // Old remote installer used --force; bundled installer overwrites by default.
          keep:
            getBool(args.options, 'keep') &&
            !getBool(args.options, 'force') &&
            !getBool(args.options, 'update'),
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
            json,
          }
        );
        return;

      case 'help':
        printBundledSkillHelp();
        return;

      default:
        if (json) {
          console.log(
            JSON.stringify({
              success: false,
              error: `Unknown skill command: "${command}"`,
            })
          );
        } else {
          console.error(
            `\n  ${c('red', '✗')} Unknown skill command: "${command}"`
          );
          console.error(
            `  Run ${c('cyan', 'octocode skill help')} for usage.\n`
          );
        }
        process.exitCode = EXIT.NOT_FOUND;
    }
  },
};
