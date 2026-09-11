import {
  cpSync,
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readlinkSync,
  readdirSync,
  renameSync,
  rmSync,
  symlinkSync,
} from 'node:fs';
import { homedir, platform as osPlatform } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { getOctocodeHome } from '@octocodeai/config';

export type SkillPlatform =
  'pi' | 'cursor' | 'claude' | 'codex' | 'opencode' | 'copilot' | 'gemini';

export type SkillScope = 'global' | 'project';
export type SkillInstallMode = 'symlink' | 'copy' | 'auto';
export type SkillCanonicalStatus =
  'installed' | 'upgraded' | 'unchanged' | 'conflict' | 'failed';
export type SkillDestinationStatus =
  'linked' | 'copied' | 'unchanged' | 'conflict' | 'failed';

export interface SkillPlatformDescriptor {
  readonly platform: SkillPlatform;
  readonly aliases: readonly string[];
  readonly autoMode: 'symlink' | 'copy';
  readonly globalRelativePath: string;
  readonly projectRelativePath: string;
}

export const SKILL_PLATFORMS: readonly SkillPlatformDescriptor[] = [
  {
    platform: 'pi',
    aliases: [],
    autoMode: 'symlink',
    globalRelativePath: '.pi/agent/skills',
    projectRelativePath: '.pi/skills',
  },
  {
    platform: 'cursor',
    aliases: [],
    autoMode: 'symlink',
    globalRelativePath: '.cursor/skills',
    projectRelativePath: '.cursor/skills',
  },
  {
    platform: 'claude',
    aliases: ['claude-desktop'],
    autoMode: 'symlink',
    globalRelativePath: '.claude/skills',
    projectRelativePath: '.claude/skills',
  },
  {
    platform: 'codex',
    aliases: ['shared', 'common', 'agents', 'codex-native'],
    autoMode: 'symlink',
    globalRelativePath: '.agents/skills',
    projectRelativePath: '.agents/skills',
  },
  {
    platform: 'opencode',
    aliases: [],
    autoMode: 'symlink',
    globalRelativePath: '.config/opencode/skills',
    projectRelativePath: '.opencode/skills',
  },
  {
    platform: 'copilot',
    aliases: [],
    autoMode: 'symlink',
    globalRelativePath: '.copilot/skills',
    projectRelativePath: '.github/skills',
  },
  {
    platform: 'gemini',
    aliases: [],
    autoMode: 'symlink',
    globalRelativePath: '.gemini/skills',
    projectRelativePath: '.gemini/skills',
  },
] as const;

export const VALID_SKILL_PLATFORM_VALUES = [
  ...SKILL_PLATFORMS.flatMap(({ platform, aliases }) => [platform, ...aliases]),
  'all',
] as const;

const PLATFORM_BY_VALUE = new Map<string, SkillPlatform>(
  SKILL_PLATFORMS.flatMap(({ platform, aliases }) => [
    [platform, platform] as const,
    ...aliases.map(alias => [alias, platform] as const),
  ])
);

const PLATFORM_DESCRIPTOR_BY_NAME = new Map<
  SkillPlatform,
  SkillPlatformDescriptor
>(SKILL_PLATFORMS.map(descriptor => [descriptor.platform, descriptor]));

export function formatSkillPlatformHelp(): string {
  const canonical = [
    ...SKILL_PLATFORMS.map(({ platform }) => platform),
    'all',
  ].join(' | ');
  const aliases = SKILL_PLATFORMS.filter(
    ({ aliases: platformAliases }) => platformAliases.length > 0
  )
    .map(
      ({ platform, aliases: platformAliases }) =>
        `${platformAliases.join(', ')} -> ${platform}`
    )
    .join('; ');
  return aliases ? `${canonical} (aliases: ${aliases})` : canonical;
}

export function parseSkillPlatforms(raw: string): {
  platforms: SkillPlatform[];
  error?: string;
} {
  const values = raw
    .split(',')
    .map(value => value.trim().toLowerCase())
    .filter(Boolean);
  const platforms: SkillPlatform[] = [];
  for (const value of values) {
    if (value === 'all')
      return { platforms: SKILL_PLATFORMS.map(({ platform }) => platform) };
    const platform = PLATFORM_BY_VALUE.get(value);
    if (!platform) {
      return {
        platforms: [],
        error: `Unknown platform: "${value}". Valid: ${VALID_SKILL_PLATFORM_VALUES.join(', ')}`,
      };
    }
    if (!platforms.includes(platform)) platforms.push(platform);
  }
  return { platforms };
}

export interface SkillDestinationOptions {
  platform: SkillPlatform;
  scope: SkillScope;
  homeDir?: string;
  projectDir?: string;
}

export function resolveSkillDestination(
  options: SkillDestinationOptions
): string {
  const home = resolve(options.homeDir ?? homedir());
  const root =
    options.scope === 'project' ? resolve(options.projectDir ?? '') : home;
  if (options.scope === 'project' && !options.projectDir) {
    throw new Error('projectDir is required for project skill installation');
  }

  const descriptor = PLATFORM_DESCRIPTOR_BY_NAME.get(options.platform);
  if (!descriptor)
    throw new Error(`Unknown skill platform: ${options.platform}`);
  return join(
    root,
    options.scope === 'global'
      ? descriptor.globalRelativePath
      : descriptor.projectRelativePath
  );
}

export function getCanonicalSkillsDir(
  env: NodeJS.ProcessEnv = process.env
): string {
  return join(getOctocodeHome(env), 'skills');
}

export interface BundledSkill {
  name: string;
  sourcePath: string;
}

export interface SkillInstallTarget {
  platform: SkillPlatform;
  scope: SkillScope;
  homeDir?: string;
  projectDir?: string;
  operatingSystem?: NodeJS.Platform;
}

export interface InstallBundledSkillsOptions {
  skills: readonly BundledSkill[];
  targets: readonly SkillInstallTarget[];
  canonicalSkillsDir?: string;
  mode?: SkillInstallMode;
  force?: boolean;
  upgrade?: boolean;
  dryRun?: boolean;
}

export interface SkillDestinationOutcome {
  platform: SkillPlatform;
  scope: SkillScope;
  destination: string;
  mode: 'symlink' | 'copy';
  status: SkillDestinationStatus;
  linkTarget?: string;
  error?: string;
}

export interface SkillInstallOutcome {
  name: string;
  source: string;
  canonical: string;
  canonicalStatus: SkillCanonicalStatus;
  canonicalError?: string;
  destinations: SkillDestinationOutcome[];
}

export interface SkillInstallSummary {
  installed: number;
  upgraded: number;
  linked: number;
  copied: number;
  unchanged: number;
  conflicts: number;
  failed: number;
}

export interface InstallBundledSkillsResult {
  ok: boolean;
  action: 'install' | 'upgrade' | 'dry-run';
  dryRun: boolean;
  force: boolean;
  upgrade: boolean;
  canonicalSkillsDir: string;
  skills: SkillInstallOutcome[];
  summary: SkillInstallSummary;
}

function pathExists(path: string): boolean {
  try {
    lstatSync(path);
    return true;
  } catch {
    return false;
  }
}

function sameTree(left: string, right: string): boolean {
  if (!pathExists(left) || !pathExists(right)) return false;
  const leftStat = lstatSync(left);
  const rightStat = lstatSync(right);
  if (leftStat.isSymbolicLink() || rightStat.isSymbolicLink()) {
    return (
      leftStat.isSymbolicLink() &&
      rightStat.isSymbolicLink() &&
      readlinkSync(left) === readlinkSync(right)
    );
  }
  if (leftStat.isFile() || rightStat.isFile()) {
    return (
      leftStat.isFile() &&
      rightStat.isFile() &&
      readFileSync(left).equals(readFileSync(right))
    );
  }
  if (!leftStat.isDirectory() || !rightStat.isDirectory()) return false;
  const leftEntries = readdirSync(left).sort();
  const rightEntries = readdirSync(right).sort();
  return (
    leftEntries.length === rightEntries.length &&
    leftEntries.every(
      (name, index) =>
        name === rightEntries[index] &&
        sameTree(join(left, name), join(right, name))
    )
  );
}

function replaceDirectory(source: string, destination: string): void {
  const parent = dirname(destination);
  mkdirSync(parent, { recursive: true });
  const stagingRoot = mkdtempSync(join(parent, '.octocode-skill-install-'));
  const staged = join(stagingRoot, 'skill');
  const backup = join(
    parent,
    `.octocode-skill-backup-${process.pid}-${Date.now()}`
  );
  let backedUp = false;
  try {
    cpSync(source, staged, { recursive: true, verbatimSymlinks: true });
    if (pathExists(destination)) {
      renameSync(destination, backup);
      backedUp = true;
    }
    renameSync(staged, destination);
    if (backedUp) rmSync(backup, { recursive: true, force: true });
  } catch (error) {
    if (backedUp && !pathExists(destination) && pathExists(backup))
      renameSync(backup, destination);
    throw error;
  } finally {
    rmSync(stagingRoot, { recursive: true, force: true });
  }
}

function replaceLink(
  target: string,
  destination: string,
  operatingSystem: NodeJS.Platform
): void {
  const parent = dirname(destination);
  mkdirSync(parent, { recursive: true });
  const stagingRoot = mkdtempSync(join(parent, '.octocode-skill-link-'));
  const staged = join(stagingRoot, 'skill');
  const backup = join(
    parent,
    `.octocode-skill-backup-${process.pid}-${Date.now()}`
  );
  let backedUp = false;
  try {
    symlinkSync(
      target,
      staged,
      operatingSystem === 'win32' ? 'junction' : 'dir'
    );
    if (pathExists(destination)) {
      renameSync(destination, backup);
      backedUp = true;
    }
    renameSync(staged, destination);
    if (backedUp) rmSync(backup, { recursive: true, force: true });
  } catch (error) {
    if (backedUp && !pathExists(destination) && pathExists(backup))
      renameSync(backup, destination);
    throw error;
  } finally {
    rmSync(stagingRoot, { recursive: true, force: true });
  }
}

function pointsTo(linkPath: string, targetPath: string): boolean {
  if (!pathExists(linkPath) || !lstatSync(linkPath).isSymbolicLink())
    return false;
  const rawTarget = readlinkSync(linkPath);
  const resolvedTarget = isAbsolute(rawTarget)
    ? resolve(rawTarget)
    : resolve(dirname(linkPath), rawTarget);
  return resolvedTarget === resolve(targetPath);
}

function effectiveMode(
  mode: SkillInstallMode,
  platform: SkillPlatform
): 'symlink' | 'copy' {
  if (mode !== 'auto') return mode;
  return SKILL_PLATFORMS.find(candidate => candidate.platform === platform)!
    .autoMode;
}

function validateSkill(skill: BundledSkill): string | undefined {
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(skill.name))
    return `Invalid skill name: "${skill.name}"`;
  const skillFile = join(resolve(skill.sourcePath), 'SKILL.md');
  if (!existsSync(skillFile)) return `Bundled skill is missing: ${skillFile}`;
  const stat = lstatSync(skillFile);
  if (!stat.isFile() || stat.isSymbolicLink())
    return `Bundled SKILL.md must be a regular file: ${skillFile}`;
  return undefined;
}

function increment(
  summary: SkillInstallSummary,
  status: SkillCanonicalStatus | SkillDestinationStatus
): void {
  if (status === 'installed') summary.installed += 1;
  else if (status === 'upgraded') summary.upgraded += 1;
  else if (status === 'linked') summary.linked += 1;
  else if (status === 'copied') summary.copied += 1;
  else if (status === 'unchanged') summary.unchanged += 1;
  else if (status === 'conflict') summary.conflicts += 1;
  else summary.failed += 1;
}

export function installBundledSkills(
  options: InstallBundledSkillsOptions
): InstallBundledSkillsResult {
  const dryRun = options.dryRun ?? false;
  const force = options.force ?? false;
  const upgrade = options.upgrade ?? false;
  const mode = options.mode ?? 'symlink';
  const canonicalSkillsDir = resolve(
    options.canonicalSkillsDir ?? getCanonicalSkillsDir()
  );
  const summary: SkillInstallSummary = {
    installed: 0,
    upgraded: 0,
    linked: 0,
    copied: 0,
    unchanged: 0,
    conflicts: 0,
    failed: 0,
  };
  const outcomes: SkillInstallOutcome[] = [];

  for (const skill of options.skills) {
    const source = resolve(skill.sourcePath);
    const canonical = join(canonicalSkillsDir, skill.name);
    const validationError = validateSkill({ ...skill, sourcePath: source });
    const canonicalExists = pathExists(canonical);
    const canonicalDiffers =
      canonicalExists && !validationError && !sameTree(source, canonical);
    const managedCopyDestinations = new Set<string>();
    if (upgrade && canonicalDiffers) {
      for (const target of options.targets) {
        if (effectiveMode(mode, target.platform) !== 'copy') continue;
        try {
          const destination = join(resolveSkillDestination(target), skill.name);
          if (sameTree(canonical, destination))
            managedCopyDestinations.add(destination);
        } catch {
          // Target validation is reported by the normal destination pass below.
        }
      }
    }
    let canonicalStatus: SkillCanonicalStatus;
    let canonicalError: string | undefined;

    if (validationError) {
      canonicalStatus = 'failed';
      canonicalError = validationError;
    } else if (sameTree(source, canonical)) {
      canonicalStatus = 'unchanged';
    } else if (canonicalExists && !force && !upgrade) {
      canonicalStatus = 'conflict';
    } else if (dryRun) {
      canonicalStatus = canonicalExists ? 'upgraded' : 'installed';
    } else {
      try {
        replaceDirectory(source, canonical);
        canonicalStatus = canonicalExists ? 'upgraded' : 'installed';
      } catch (error) {
        canonicalStatus = 'failed';
        canonicalError = error instanceof Error ? error.message : String(error);
      }
    }
    increment(summary, canonicalStatus);

    const destinations: SkillDestinationOutcome[] = [];
    const seenDestinations = new Set<string>();
    if (
      canonicalStatus === 'installed' ||
      canonicalStatus === 'upgraded' ||
      canonicalStatus === 'unchanged'
    ) {
      for (const target of options.targets) {
        let destination: string;
        try {
          destination = join(resolveSkillDestination(target), skill.name);
        } catch (error) {
          const failed: SkillDestinationOutcome = {
            platform: target.platform,
            scope: target.scope,
            destination: '',
            mode: effectiveMode(mode, target.platform),
            status: 'failed',
            error: error instanceof Error ? error.message : String(error),
          };
          destinations.push(failed);
          increment(summary, failed.status);
          continue;
        }
        if (seenDestinations.has(destination)) continue;
        seenDestinations.add(destination);

        const targetMode = effectiveMode(mode, target.platform);
        let status: SkillDestinationStatus;
        let error: string | undefined;
        const exists = pathExists(destination);
        const managedCopyUpgrade =
          upgrade &&
          canonicalStatus === 'upgraded' &&
          targetMode === 'copy' &&
          managedCopyDestinations.has(destination);
        const copyAlreadyMatchesIncoming =
          upgrade &&
          canonicalStatus === 'upgraded' &&
          targetMode === 'copy' &&
          sameTree(source, destination);
        const unchanged =
          targetMode === 'symlink'
            ? pointsTo(destination, canonical)
            : copyAlreadyMatchesIncoming ||
              (!managedCopyUpgrade &&
                sameTree(
                  dryRun && !pathExists(canonical) ? source : canonical,
                  destination
                ));

        if (unchanged) {
          status = 'unchanged';
        } else if (exists && !force && !managedCopyUpgrade) {
          status = 'conflict';
        } else if (dryRun) {
          status = targetMode === 'symlink' ? 'linked' : 'copied';
        } else {
          try {
            if (targetMode === 'symlink') {
              replaceLink(
                canonical,
                destination,
                target.operatingSystem ?? osPlatform()
              );
              status = 'linked';
            } else {
              replaceDirectory(canonical, destination);
              status = 'copied';
            }
          } catch (installError) {
            status = 'failed';
            error =
              installError instanceof Error
                ? installError.message
                : String(installError);
          }
        }

        const outcome: SkillDestinationOutcome = {
          platform: target.platform,
          scope: target.scope,
          destination,
          mode: targetMode,
          status,
          ...(targetMode === 'symlink' ? { linkTarget: canonical } : {}),
          ...(error ? { error } : {}),
        };
        destinations.push(outcome);
        increment(summary, status);
      }
    }

    outcomes.push({
      name: skill.name,
      source,
      canonical,
      canonicalStatus,
      ...(canonicalError ? { canonicalError } : {}),
      destinations,
    });
  }

  return {
    ok: summary.conflicts === 0 && summary.failed === 0,
    action: dryRun ? 'dry-run' : upgrade ? 'upgrade' : 'install',
    dryRun,
    force,
    upgrade,
    canonicalSkillsDir,
    skills: outcomes,
    summary,
  };
}
