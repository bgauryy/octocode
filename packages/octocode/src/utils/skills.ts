import { fileURLToPath } from 'node:url';
import {
  basename,
  dirname,
  isAbsolute,
  join,
  relative,
  resolve,
  sep,
} from 'node:path';
import {
  existsSync,
  readFileSync,
  writeFileSync,
  mkdirSync,
  rmSync,
  symlinkSync,
} from 'node:fs';
import {
  copyDirectory,
  dirExists,
  listSubdirectories,
  fileExists,
  readFileContent,
} from './fs.js';
import { HOME, isWindows, getAppDataPath } from './platform.js';
import { paths } from '@octocodeai/octocode-tools-core/paths';
import { trySafe } from './try-safe.js';
import { parseSkillFrontmatter } from './parsers/frontmatter.js';
import { z } from '@octocodeai/octocode-tools-core/zod';

const OCTOCODE_DIR = paths.home;
const CONFIG_FILE = paths.cliConfig;

const OctocodeConfigSchema = z
  .object({
    skillsDestDir: z.string().optional(),
  })
  .passthrough();

type OctocodeConfig = z.infer<typeof OctocodeConfigSchema>;

export type SkillInstallMode = 'copy' | 'symlink';
export type SkillInstallStrategy = SkillInstallMode | 'hybrid';
export const SKILL_INSTALL_TARGETS = [
  'claude-code',
  'claude-desktop',
  'cursor',
  'codex',
  'opencode',
  'pi',
  'copilot',
  'gemini',
  'agents',
] as const;
export type SkillInstallTarget = (typeof SKILL_INSTALL_TARGETS)[number];
export const USER_SKILL_PLATFORMS = [
  'common',
  'cursor',
  'claude',
  'codex',
  'opencode',
  'pi',
  'copilot',
  'gemini',
] as const;
export type UserSkillPlatform = (typeof USER_SKILL_PLATFORMS)[number];
export const DEFAULT_SKILL_INSTALL_TARGETS: readonly SkillInstallTarget[] = [
  'agents',
];
export const CLAUDE_SKILL_INSTALL_TARGETS: readonly SkillInstallTarget[] = [
  'claude-code',
  'claude-desktop',
];
export const USER_SKILL_PLATFORM_TARGETS: Record<
  UserSkillPlatform,
  readonly SkillInstallTarget[]
> = {
  common: ['agents'],
  cursor: ['cursor'],
  claude: CLAUDE_SKILL_INSTALL_TARGETS,
  codex: ['agents'],
  opencode: ['opencode'],
  pi: ['pi'],
  copilot: ['copilot'],
  gemini: ['gemini'],
};
export type SkillInstallResult = 'installed' | 'skipped' | 'failed';

const SKILL_TARGET_IDS: Record<string, SkillInstallTarget> = {
  'claude-code': 'claude-code',
  'claude-desktop': 'claude-desktop',
  claude: 'claude-code',
  cursor: 'cursor',
  codex: 'codex',
  opencode: 'opencode',
  pi: 'pi',
  'pi-agent': 'pi',
  copilot: 'copilot',
  'github-copilot': 'copilot',
  'vscode-copilot': 'copilot',
  gemini: 'gemini',
  'gemini-cli': 'gemini',
  agents: 'agents',
  agent: 'agents',
  common: 'agents',
};

function loadConfig(): OctocodeConfig {
  return trySafe(() => {
    if (existsSync(CONFIG_FILE)) {
      const content = readFileSync(CONFIG_FILE, 'utf-8');
      const parsed = OctocodeConfigSchema.safeParse(JSON.parse(content));
      return parsed.success ? parsed.data : {};
    }
    return {};
  }, {});
}

function saveConfig(config: OctocodeConfig): void {
  trySafe(() => {
    if (!existsSync(OCTOCODE_DIR)) {
      mkdirSync(OCTOCODE_DIR, { recursive: true, mode: 0o700 });
    }
    writeFileSync(CONFIG_FILE, JSON.stringify(config, null, 2), {
      encoding: 'utf-8',
      mode: 0o600,
    });
    return true;
  }, false);
}

export function setCustomSkillsDestDir(path: string | null): void {
  const config = loadConfig();
  if (path) {
    config.skillsDestDir = path;
  } else {
    delete config.skillsDestDir;
  }
  saveConfig(config);
}

export function getCustomSkillsDestDir(): string | null {
  const config = loadConfig();
  return config.skillsDestDir || null;
}

export function getDefaultSkillsDestDir(): string {
  if (isWindows) {
    const appData = getAppDataPath();
    return join(appData, 'Claude', 'skills');
  }
  return join(HOME, '.claude', 'skills');
}

export function normalizeSkillTarget(
  target: string
): SkillInstallTarget | null {
  return SKILL_TARGET_IDS[target.trim().toLowerCase()] ?? null;
}

export function formatSkillInstallTargets(): string {
  return SKILL_INSTALL_TARGETS.join(', ');
}

export function getSkillsDirForTarget(
  target: SkillInstallTarget,
  defaultDestDir: string = getSkillsDestDir()
): string {
  if (target === 'claude-code') {
    return defaultDestDir;
  }

  if (isWindows) {
    const appData = getAppDataPath();
    switch (target) {
      case 'claude-desktop':
        return join(appData, 'Claude Desktop', 'skills');
      case 'cursor':
        return join(HOME, '.cursor', 'skills');
      case 'codex':
        return join(HOME, '.agents', 'skills');
      case 'opencode':
        return join(appData, 'opencode', 'skills');
      case 'pi':
        return join(HOME, '.pi', 'agent', 'skills');
      case 'copilot':
        return join(HOME, '.copilot', 'skills');
      case 'gemini':
        return join(HOME, '.gemini', 'skills');
      case 'agents':
        return join(HOME, '.agents', 'skills');
    }
  }

  switch (target) {
    case 'claude-desktop':
      return join(HOME, '.claude-desktop', 'skills');
    case 'cursor':
      return join(HOME, '.cursor', 'skills');
    case 'codex':
      return join(HOME, '.agents', 'skills');
    case 'opencode':
      return join(HOME, '.config', 'opencode', 'skills');
    case 'pi':
      return join(HOME, '.pi', 'agent', 'skills');
    case 'copilot':
      return join(HOME, '.copilot', 'skills');
    case 'gemini':
      return join(HOME, '.gemini', 'skills');
    case 'agents':
      return join(HOME, '.agents', 'skills');
  }
}

export function resolveModeForTarget(
  strategy: SkillInstallStrategy,
  target: SkillInstallTarget
): SkillInstallMode {
  if (strategy === 'hybrid') {
    return target === 'claude-code' || target === 'claude-desktop'
      ? 'copy'
      : 'symlink';
  }

  return strategy;
}

export function isPathInside(baseDir: string, targetPath: string): boolean {
  const normalizedBase = resolve(baseDir);
  const normalizedTarget = resolve(targetPath);
  const relativePath = relative(normalizedBase, normalizedTarget);

  return (
    relativePath !== '..' &&
    !relativePath.startsWith(`..${sep}`) &&
    !isAbsolute(relativePath)
  );
}

export function isSafeSkillName(skillName: string): boolean {
  const trimmed = skillName.trim();
  return (
    trimmed.length > 0 &&
    trimmed === skillName &&
    trimmed !== '.' &&
    trimmed !== '..' &&
    !trimmed.includes('\0') &&
    !trimmed.includes('/') &&
    !trimmed.includes('\\')
  );
}

export function resolveSkillDestination(
  destDir: string,
  skillName: string
): string | null {
  if (!isSafeSkillName(skillName)) {
    return null;
  }

  const destinationPath = resolve(destDir, skillName);
  return isPathInside(destDir, destinationPath) ? destinationPath : null;
}

interface SkillMetadata {
  name: string;
  description: string;
  folder: string;
}

function getSkillsSourceCandidates(): string[] {
  const currentFile = fileURLToPath(import.meta.url);
  const currentDir = dirname(currentFile);

  return [
    resolve(currentDir, '..', 'skills'),
    resolve(currentDir, '..', '..', 'skills'),
    resolve(currentDir, '..', '..', '..', '..', 'skills'),
    resolve(currentDir, '..', '..', '..', 'skills'),
  ];
}

function findSkillsSourcePath(options: { fallback: boolean }): string {
  const candidates = getSkillsSourceCandidates();

  for (const candidate of candidates) {
    if (dirExists(candidate)) {
      return candidate;
    }
  }

  if (options.fallback) {
    return candidates[0];
  }

  throw new Error('Skills directory not found');
}

export function getSkillsSourcePath(): string {
  return findSkillsSourcePath({ fallback: false });
}

export function copySkills(destDir: string): boolean {
  const skillsSource = getSkillsSourcePath();
  return copyDirectory(skillsSource, destDir);
}

export function copySkill(skillName: string, destDir: string): boolean {
  const skillsSource = getSkillsSourcePath();
  const skillPath = resolveSkillDestination(skillsSource, skillName);
  const destPath = resolveSkillDestination(destDir, skillName);

  if (!skillPath || !destPath) {
    return false;
  }

  return (
    installSkillToDestination({
      sourcePath: skillPath,
      destinationPath: destPath,
      mode: 'copy',
      force: true,
    }) === 'installed'
  );
}

export function getAvailableSkills(): string[] {
  const skillsSource = getSkillsSourcePath();
  return listSubdirectories(skillsSource).filter(
    name => name.startsWith('octocode-') && isSafeSkillName(name)
  );
}

export function getSkillsSourceDir(): string {
  return findSkillsSourcePath({ fallback: true });
}

export function getSkillsDestDir(): string {
  const customPath = getCustomSkillsDestDir();
  if (customPath) {
    return customPath;
  }
  return getDefaultSkillsDestDir();
}

export function getSkillMetadata(skillPath: string): SkillMetadata | null {
  const skillMdPath = join(skillPath, 'SKILL.md');

  if (!fileExists(skillMdPath)) {
    return null;
  }

  const content = readFileContent(skillMdPath);
  if (!content) {
    return null;
  }

  const parsed = parseSkillFrontmatter(content);
  if (!parsed?.name || !parsed.description) {
    return null;
  }

  return {
    name: parsed.name,
    description: parsed.description,
    folder: basename(skillPath),
  };
}

export function getAllSkillsMetadata(): SkillMetadata[] {
  const skillsSource = getSkillsSourcePath();
  const skillDirs = listSubdirectories(skillsSource).filter(
    name => name.startsWith('octocode-') && isSafeSkillName(name)
  );

  const skills: SkillMetadata[] = [];

  for (const skillDir of skillDirs) {
    const skillPath = join(skillsSource, skillDir);
    const metadata = getSkillMetadata(skillPath);
    if (metadata) {
      skills.push(metadata);
    }
  }

  return skills;
}

export function installSkillToDestination({
  sourcePath,
  destinationPath,
  mode,
  force,
}: {
  sourcePath: string;
  destinationPath: string;
  mode: SkillInstallMode;
  force: boolean;
}): SkillInstallResult {
  try {
    if (!dirExists(sourcePath)) {
      return 'failed';
    }

    if (existsSync(destinationPath)) {
      if (!force) {
        return 'skipped';
      }
      rmSync(destinationPath, { recursive: true, force: true });
    }

    const parentDir = dirname(destinationPath);
    if (!dirExists(parentDir)) {
      mkdirSync(parentDir, { recursive: true, mode: 0o700 });
    }

    if (mode === 'symlink') {
      const symlinkType: 'dir' | 'junction' = isWindows ? 'junction' : 'dir';
      symlinkSync(sourcePath, destinationPath, symlinkType);
      return 'installed';
    }

    return copyDirectory(sourcePath, destinationPath) ? 'installed' : 'failed';
  } catch {
    return 'failed';
  }
}
