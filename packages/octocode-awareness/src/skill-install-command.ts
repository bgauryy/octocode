import { existsSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import {
  SKILL_PLATFORMS,
  VALID_SKILL_PLATFORM_VALUES,
  getCanonicalSkillsDir,
  installBundledSkills,
  parseSkillPlatforms,
  type SkillInstallMode,
} from '@octocodeai/octocode-skill-installer';

const SKILL_NAME = 'octocode-awareness';

export interface SkillInstallOptions {
  skillsDir: string;
  cwd?: string;
  homeDir?: string;
  canonicalSkillsDir?: string;
}

export interface SkillInstallResult {
  exitCode: number;
  payload: Record<string, unknown>;
}

function fail(
  error: string,
  details: Record<string, unknown> = {}
): SkillInstallResult {
  return { exitCode: 1, payload: { ok: false, error, ...details } };
}

function option(
  argv: string[] | Record<string, unknown>,
  name: string
): string | undefined {
  if (!Array.isArray(argv)) {
    const value = argv[name.slice(2).replaceAll('-', '_')];
    return value === undefined ? undefined : String(value);
  }
  const prefix = `${name}=`;
  const inline = argv.find(arg => arg.startsWith(prefix));
  if (inline) return inline.slice(prefix.length);
  const index = argv.indexOf(name);
  if (index < 0) return undefined;
  const value = argv[index + 1];
  return value && !value.startsWith('--') ? value : undefined;
}

function flag(argv: string[] | Record<string, unknown>, name: string): boolean {
  return Array.isArray(argv)
    ? argv.includes(name)
    : argv[name.slice(2).replaceAll('-', '_')] === true;
}

function installMode(
  argv: string[] | Record<string, unknown>
): SkillInstallMode | undefined {
  const value = option(argv, '--mode') ?? 'symlink';
  return value === 'symlink' || value === 'copy' || value === 'auto'
    ? value
    : undefined;
}

export function runSkillInstall(
  argv: string[] | Record<string, unknown>,
  options: SkillInstallOptions
): SkillInstallResult {
  const platformValue = option(argv, '--platform');
  if (!platformValue) {
    return fail(
      `--platform is required (${VALID_SKILL_PLATFORM_VALUES.join('|')})`
    );
  }
  const parsedPlatforms = parseSkillPlatforms(platformValue);
  if (parsedPlatforms.error) {
    return fail(parsedPlatforms.error, {
      supported_platforms: VALID_SKILL_PLATFORM_VALUES,
    });
  }

  const global = flag(argv, '--global');
  const projectDirValue = option(argv, '--project-dir');
  if (global && projectDirValue)
    return fail('use either --global or --project-dir, not both');
  if (!global && !projectDirValue)
    return fail(
      'choose an explicit scope with --global or --project-dir <path>'
    );

  const mode = installMode(argv);
  if (!mode) return fail('--mode must be symlink, copy, or auto');

  const cwd = options.cwd ?? process.cwd();
  const projectDir = projectDirValue
    ? resolve(cwd, projectDirValue)
    : undefined;
  if (projectDir) {
    const unsupported = parsedPlatforms.platforms.filter(
      platform =>
        !SKILL_PLATFORMS.find(candidate => candidate.platform === platform)
          ?.supportsProject
    );
    if (unsupported.length > 0) {
      return fail(
        `project skill installation is unsupported for: ${unsupported.join(', ')}`
      );
    }
  }
  if (
    projectDir &&
    (!existsSync(projectDir) || !statSync(projectDir).isDirectory())
  ) {
    return fail(`project directory does not exist: ${projectDir}`);
  }

  const source = join(resolve(options.skillsDir), SKILL_NAME);
  const scope = global ? ('global' as const) : ('project' as const);
  const homeDir = options.homeDir ?? homedir();
  const result = installBundledSkills({
    skills: [{ name: SKILL_NAME, sourcePath: source }],
    canonicalSkillsDir: options.canonicalSkillsDir ?? getCanonicalSkillsDir(),
    targets: parsedPlatforms.platforms.map(platform => ({
      platform,
      scope,
      homeDir,
      ...(projectDir ? { projectDir } : {}),
    })),
    mode,
    force: flag(argv, '--force'),
    dryRun: flag(argv, '--dry-run'),
  });
  return {
    exitCode: result.ok ? 0 : 1,
    payload: { ...result },
  };
}
