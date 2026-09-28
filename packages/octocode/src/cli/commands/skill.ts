import type { CLICommand, ParsedArgs } from '../types.js';
import { EXIT } from '../exit-codes.js';
import { getBool, getString } from '../options.js';
import { bold, dim } from '../../utils/colors.js';
import { runList } from './skills/commands/list.js';
import { runInstall, type InstallOptions } from './skills/commands/install.js';
import { runRemove } from './skills/commands/remove.js';
import { runInfo } from './skills/commands/info.js';
import { runCheck } from './skills/commands/check.js';
import type { InstallMode } from './skills/installer.js';
import { formatSkillPlatformHelp } from '@octocodeai/octocode-skill-installer';

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
  install <name>...       Install one or more bundled skills
  install --add <source> Add a local skill to the canonical home
  remove  <name>...       Remove a skill — home copy + platform links
  check  [<name>...]      Verify installs, platform links, and env readiness
  info   <name>           Show full SKILL.md content

${bold('Install options')}
  --all                   Install all bundled skills
  --platform <p>          Link into platform dir  ${dim(`(${formatSkillPlatformHelp()})`)}
  --global                Install links in the selected platform's global scope
  --project-dir <dir>     Install links in the selected platform's project scope
  --path <dir>            Install bundled skill directly to a custom destination
  --mode copy|symlink|auto  ${dim('[default: symlink · copy only when requested]')}
  --force                 Replace an existing installation that differs
  --upgrade               Refresh changed bundled content; preserve destination drift
  --dry-run               Preview without writing

${bold('Remove options')}
  --all                   Remove all installed skills
  --platform <p>          Remove only specified platform link(s)  ${dim('(home kept)')}
  --dry-run               Preview without deleting

${bold('Check options')}
  --platform <p>          Check specific platforms only
  --workspace             Also check <cwd>/.agents/skills
  --fix                   Refresh stale/broken installs in place (adds no new locations)
  --dry-run               With --fix: preview fixes without writing
  --no-env                Skip env param checks

${bold('Global flags')}
  --json                  Machine-readable JSON output
  --json-errors           Emit structured JSON errors on stdout
  --help                  Show this help

${bold('Examples')}
  octocode skill list --json
  octocode skill install --all --platform pi,cursor --global
  octocode skill install --add ./skills/my-skill --platform claude,cursor --global
  octocode skill install octocode-research --platform codex --project-dir .
  octocode skill remove octocode-research --platform pi
  octocode skill check --fix
  octocode skill info octocode-research
`);
}

function subcommand(args: ParsedArgs): string {
  const first = args.args[0];
  if (first && SUBCOMMANDS.has(first)) return first;
  return first ?? 'help';
}

function positionalAfterSubcommand(args: ParsedArgs): string[] {
  const first = args.args[0];
  return first && SUBCOMMANDS.has(first) ? args.args.slice(1) : args.args;
}

function platformOption(args: ParsedArgs): string | null {
  return getString(args.options, 'platform');
}

function installMode(args: ParsedArgs): InstallMode {
  const rawMode = getString(args.options, 'mode');
  return rawMode === 'copy' || rawMode === 'auto' ? rawMode : 'symlink';
}

function installNames(args: ParsedArgs): string[] {
  return positionalAfterSubcommand(args).filter(a => !a.startsWith('-'));
}

export const skillCommand: CLICommand = {
  name: 'skill',
  options: [
    { name: 'add', hasValue: true },
    { name: 'platform', hasValue: true },
    { name: 'all' },
    { name: 'mode', hasValue: true, default: 'symlink' },
    { name: 'force' },
    { name: 'upgrade' },
    { name: 'global' },
    { name: 'project-dir', hasValue: true },
    { name: 'workspace' },
    { name: 'path', hasValue: true },
    { name: 'dry-run' },
    { name: 'fix' },
    { name: 'no-env' },
    { name: 'json' },
  ],
  handler: (args: ParsedArgs) => {
    const json = getBool(args.options, 'json');
    const jsonErrors = getBool(args.options, 'json-errors');
    const fail = (message: string): void => {
      if (jsonErrors)
        console.log(
          JSON.stringify({
            kind: 'octocode.toolError',
            version: 1,
            error: message,
          })
        );
      else if (json)
        console.log(JSON.stringify({ success: false, error: message }));
      else console.error(message);
      process.exitCode = EXIT.USAGE;
    };
    const allowed = new Set([
      ...(skillCommand.options ?? []).map(option => option.name),
      'help',
      'json-errors',
      'no-color',
      'redact-emails',
    ]);
    const unknown = Object.keys(args.options).find(key => !allowed.has(key));
    if (unknown) return fail(`Unknown option: --${unknown}`);
    for (const option of skillCommand.options ?? []) {
      if (
        option.hasValue &&
        args.options[option.name] !== undefined &&
        typeof args.options[option.name] !== 'string'
      ) {
        return fail(`--${option.name} requires a value.`);
      }
    }
    if (
      args.options.mode !== undefined &&
      !['copy', 'symlink', 'auto'].includes(String(args.options.mode))
    ) {
      return fail('--mode expects copy|symlink|auto.');
    }
    if (getBool(args.options, 'help')) {
      printBundledSkillHelp();
      return;
    }
    const command = subcommand(args);

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
        runInfo(skillName, { json, jsonErrors });
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
          jsonErrors,
        });
        return;

      case 'install': {
        const addSource = getString(args.options, 'add');
        const addLocal = Boolean(addSource);
        const installAll = getBool(args.options, 'all');
        const rawPath = getString(args.options, 'path') || null;
        const opts: InstallOptions = {
          all: installAll,
          sourcePath: addLocal ? addSource || rawPath : null,
          platform: platformOption(args),
          workspace: getBool(args.options, 'workspace'),
          global: getBool(args.options, 'global'),
          projectDir: getString(args.options, 'project-dir') || null,
          customPath: addLocal ? null : rawPath,
          mode: installMode(args),
          force: getBool(args.options, 'force'),
          upgrade: getBool(args.options, 'upgrade'),
          dryRun: getBool(args.options, 'dry-run'),
          json,
          jsonErrors,
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
            jsonErrors,
          }
        );
        return;

      case 'help':
        printBundledSkillHelp();
        return;

      default:
        fail(
          `Unknown skill command: "${command}". Run octocode skill help for usage.`
        );
    }
  },
};
