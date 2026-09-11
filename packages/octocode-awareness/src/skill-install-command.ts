import { existsSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import {
  VALID_SKILL_PLATFORM_VALUES,
  getCanonicalSkillsDir,
  installBundledSkills,
  parseSkillPlatforms,
  type SkillInstallMode,
} from '@octocodeai/octocode-skill-installer';
import {
  AWARENESS_SKILL_NAME,
  failSkillCommand,
  skillFlag,
  skillOption,
  type SkillCommandArguments,
  type SkillCommandOptions,
  type SkillCommandResult,
} from './skill-command-utils.js';

export type SkillInstallOptions = SkillCommandOptions;
export type SkillInstallResult = SkillCommandResult;

function installMode(
  argv: SkillCommandArguments
): SkillInstallMode | undefined {
  const value = skillOption(argv, '--mode') ?? 'symlink';
  return value === 'symlink' || value === 'copy' || value === 'auto'
    ? value
    : undefined;
}

export function runSkillInstall(
  argv: SkillCommandArguments,
  options: SkillInstallOptions
): SkillInstallResult {
  const platformValue = skillOption(argv, '--platform');
  if (!platformValue) {
    return failSkillCommand(
      `--platform is required (${VALID_SKILL_PLATFORM_VALUES.join('|')})`
    );
  }
  const parsedPlatforms = parseSkillPlatforms(platformValue);
  if (parsedPlatforms.error) {
    return failSkillCommand(parsedPlatforms.error, {
      supported_platforms: VALID_SKILL_PLATFORM_VALUES,
    });
  }

  const global = skillFlag(argv, '--global');
  const projectDirValue = skillOption(argv, '--project-dir');
  if (global && projectDirValue)
    return failSkillCommand('use either --global or --project-dir, not both');
  if (!global && !projectDirValue)
    return failSkillCommand(
      'choose an explicit scope with --global or --project-dir <path>'
    );

  const mode = installMode(argv);
  if (!mode)
    return failSkillCommand('--mode must be symlink, copy, or auto');

  const cwd = options.cwd ?? process.cwd();
  const projectDir = projectDirValue
    ? resolve(cwd, projectDirValue)
    : undefined;
  if (
    projectDir &&
    (!existsSync(projectDir) || !statSync(projectDir).isDirectory())
  ) {
    return failSkillCommand(`project directory does not exist: ${projectDir}`);
  }

  const source = join(resolve(options.skillsDir), AWARENESS_SKILL_NAME);
  const scope = global ? ('global' as const) : ('project' as const);
  const homeDir = options.homeDir ?? homedir();
  const result = installBundledSkills({
    skills: [{ name: AWARENESS_SKILL_NAME, sourcePath: source }],
    canonicalSkillsDir: options.canonicalSkillsDir ?? getCanonicalSkillsDir(),
    targets: parsedPlatforms.platforms.map(platform => ({
      platform,
      scope,
      homeDir,
      ...(projectDir ? { projectDir } : {}),
    })),
    mode,
    force: skillFlag(argv, '--force'),
    upgrade: skillFlag(argv, '--upgrade'),
    dryRun: skillFlag(argv, '--dry-run'),
  });
  return {
    exitCode: result.ok ? 0 : 1,
    payload: { ...result },
  };
}
