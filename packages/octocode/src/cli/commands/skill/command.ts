import {
  getSkillTargetDestinations,
  parseUserSkillPlatformList,
} from '../../../features/skills.js';
import type { CLICommand, ParsedArgs } from '../../types.js';
import { runDryRun } from './dry-run.js';
import { printUsageError } from './format.js';
import { getCanonicalSkillSourceRoot } from './bundled-source.js';
import { runInstall } from './install.js';
import { runListCommand } from './list-command.js';
import { resolveCommandRequests } from './resolve-command-requests.js';
import { DEFAULT_INSTALL_MODE } from './types.js';
import type { SkillInstallStrategy } from '../../../utils/skills.js';

export const skillCommand: CLICommand = {
  name: 'skill',
  options: [
    { name: 'add', hasValue: true },
    { name: 'path', hasValue: true },
    { name: 'name', hasValue: true },
    { name: 'platform', hasValue: true },
    { name: 'target', hasValue: true },
    { name: 'branch', hasValue: true },
    { name: 'mode', hasValue: true, default: DEFAULT_INSTALL_MODE },
    { name: 'force' },
    { name: 'update' },
    { name: 'dry-run' },
    { name: 'list' },
    { name: 'all' },
    { name: 'install-all' },
    { name: 'all-skills' },
    { name: 'verbose' },
    { name: 'json' },
  ],
  handler: async (args: ParsedArgs) => {
    const jsonOutput = Boolean(args.options['json']);
    const dryRun = Boolean(args.options['dry-run']);
    const verbose = Boolean(args.options['verbose']);
    const listFlag = Boolean(args.options['list']);

    // ── --list: show available Octocode named skills ──────────────────────────
    if (listFlag) {
      await runListCommand(jsonOutput);
      return;
    }

    // ── Parse source ──────────────────────────────────────────────────────────
    const rawAdd = args.options['add'];
    const rawPath = args.options['path'];
    const rawName = args.options['name'];
    const namedSkill =
      typeof rawName === 'string' && rawName.trim().length > 0
        ? rawName.trim()
        : undefined;
    const githubFolder =
      typeof rawAdd === 'string' && rawAdd.trim().length > 0
        ? rawAdd.trim()
        : rawAdd === true
          ? undefined
          : args.args[0];
    const localSkillPath =
      typeof rawPath === 'string' && rawPath.trim().length > 0
        ? rawPath.trim()
        : undefined;
    const installAll = Boolean(
      args.options['install-all'] || args.options['all-skills']
    );

    if (!githubFolder && !localSkillPath && !namedSkill && !installAll) {
      printUsageError(
        'Missing GitHub skill path, local --path, Octocode skill name, or --install-all  (try --list to browse)',
        jsonOutput
      );
      return;
    }

    const sourceChoices = [
      Boolean(githubFolder),
      Boolean(localSkillPath),
      Boolean(namedSkill),
      installAll,
    ].filter(Boolean).length;
    if (sourceChoices > 1) {
      printUsageError(
        'Use only one of --add <github-path>, --add --path <local-skill-or-skills-dir>, --name <octocode-skill>, or --install-all',
        jsonOutput
      );
      return;
    }

    // ── Parse mode (copy | symlink | hybrid) ──────────────────────────────────
    const rawMode = args.options['mode'];
    const mode =
      typeof rawMode === 'string' && rawMode.trim().length > 0
        ? rawMode.trim().toLowerCase()
        : DEFAULT_INSTALL_MODE;
    if (mode !== 'copy' && mode !== 'symlink' && mode !== 'hybrid') {
      printUsageError(
        'Invalid --mode value. Use copy, symlink, or hybrid.',
        jsonOutput
      );
      return;
    }

    // ── Parse platform (--all flag is shorthand for --platform all) ───────────
    const allFlag = Boolean(args.options['all']);
    const rawPlatform = allFlag
      ? 'all'
      : typeof args.options['platform'] === 'string' &&
          args.options['platform'].trim().length > 0
        ? args.options['platform']
        : typeof args.options['target'] === 'string' &&
            args.options['target'].trim().length > 0
          ? args.options['target']
          : 'common';

    const parsedPlatforms = parseUserSkillPlatformList(rawPlatform);
    if (parsedPlatforms.error) {
      printUsageError(
        `${parsedPlatforms.error}. Valid platforms: common, cursor, claude, codex, opencode, pi, copilot, gemini, all`,
        jsonOutput
      );
      return;
    }
    const platforms = parsedPlatforms.platforms;
    const targets = parsedPlatforms.targets;

    // ── Resolve source skill(s) ───────────────────────────────────────────────
    const branchOverride =
      typeof args.options['branch'] === 'string' &&
      args.options['branch'].trim().length > 0
        ? args.options['branch'].trim()
        : undefined;

    const requests = await resolveCommandRequests({
      installAll,
      namedSkill,
      localSkillPath,
      githubFolder,
      branchOverride,
      jsonOutput,
    });
    if (!requests) return;

    // ── Resolve destinations ──────────────────────────────────────────────────
    const destinations = getSkillTargetDestinations(targets, undefined);
    const sourceRoot = getCanonicalSkillSourceRoot();
    const force =
      Boolean(args.options['force']) || Boolean(args.options['update']);

    // ── --dry-run: preview without installing ─────────────────────────────────
    if (dryRun) {
      runDryRun({
        requests,
        destinations,
        force,
        jsonOutput,
        mode,
        platforms,
        verbose,
      });
      return;
    }

    await runInstall({
      requests,
      destinations,
      sourceRoot,
      force,
      mode: mode as SkillInstallStrategy,
      platforms,
      jsonOutput,
      verbose,
    });
  },
};
