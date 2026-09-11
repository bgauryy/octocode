import {
  existsSync,
  lstatSync,
  readFileSync,
  readlinkSync,
  readdirSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import {
  getCanonicalSkillsDir,
  parseSkillPlatforms,
  resolveSkillDestination,
  type SkillPlatform,
  type SkillScope,
} from '@octocodeai/octocode-skill-installer';

export const AWARENESS_SKILL_NAME = 'octocode-awareness';

export interface SkillCommandOptions {
  skillsDir: string;
  cwd?: string;
  homeDir?: string;
  canonicalSkillsDir?: string;
}

export interface SkillCommandResult {
  exitCode: number;
  payload: Record<string, unknown>;
}

export type SkillCommandArguments = string[] | Record<string, unknown>;

export function failSkillCommand(
  error: string,
  details: Record<string, unknown> = {}
): SkillCommandResult {
  return { exitCode: 1, payload: { ok: false, error, ...details } };
}

export function skillOption(
  argv: SkillCommandArguments,
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

export function skillFlag(
  argv: SkillCommandArguments,
  name: string
): boolean {
  return Array.isArray(argv)
    ? argv.includes(name)
    : argv[name.slice(2).replaceAll('-', '_')] === true;
}

export function bundledSkillPath(options: SkillCommandOptions): string {
  return join(resolve(options.skillsDir), AWARENESS_SKILL_NAME);
}

export function canonicalSkillPath(options: SkillCommandOptions): string {
  return join(
    resolve(options.canonicalSkillsDir ?? getCanonicalSkillsDir()),
    AWARENESS_SKILL_NAME
  );
}

export function pathExists(path: string): boolean {
  try {
    lstatSync(path);
    return true;
  } catch {
    return false;
  }
}

function equalEntry(left: string, right: string): boolean {
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
        equalEntry(join(left, name), join(right, name))
    )
  );
}

export function skillTreesEqual(left: string, right: string): boolean {
  return equalEntry(resolve(left), resolve(right));
}

export function linkPointsTo(linkPath: string, targetPath: string): boolean {
  if (!pathExists(linkPath) || !lstatSync(linkPath).isSymbolicLink())
    return false;
  const rawTarget = readlinkSync(linkPath);
  const resolvedTarget = isAbsolute(rawTarget)
    ? resolve(rawTarget)
    : resolve(dirname(linkPath), rawTarget);
  return resolvedTarget === resolve(targetPath);
}

export interface ScopedSkillSelection {
  platforms: SkillPlatform[];
  scope: SkillScope;
  homeDir: string;
  projectDir?: string;
}

export function scopedSkillSelection(
  argv: SkillCommandArguments,
  options: SkillCommandOptions
): ScopedSkillSelection | SkillCommandResult {
  const platformValue = skillOption(argv, '--platform');
  if (!platformValue)
    return failSkillCommand('--platform is required for a scoped skill target');
  const parsed = parseSkillPlatforms(platformValue);
  if (parsed.error) return failSkillCommand(parsed.error);

  const global = skillFlag(argv, '--global');
  const projectDirValue = skillOption(argv, '--project-dir');
  if (global && projectDirValue)
    return failSkillCommand('use either --global or --project-dir, not both');
  if (!global && !projectDirValue)
    return failSkillCommand(
      'choose an explicit scope with --global or --project-dir <path>'
    );

  const cwd = options.cwd ?? process.cwd();
  const projectDir = projectDirValue
    ? resolve(cwd, projectDirValue)
    : undefined;
  if (
    projectDir &&
    (!existsSync(projectDir) || !lstatSync(projectDir).isDirectory())
  ) {
    return failSkillCommand(`project directory does not exist: ${projectDir}`);
  }

  return {
    platforms: parsed.platforms,
    scope: global ? 'global' : 'project',
    homeDir: resolve(options.homeDir ?? homedir()),
    ...(projectDir ? { projectDir } : {}),
  };
}

export function isSkillCommandFailure(
  value: ScopedSkillSelection | SkillCommandResult
): value is SkillCommandResult {
  return 'exitCode' in value;
}

export function selectedDestination(
  selection: ScopedSkillSelection,
  platform: SkillPlatform
): string {
  return join(
    resolveSkillDestination({
      platform,
      scope: selection.scope,
      homeDir: selection.homeDir,
      ...(selection.projectDir ? { projectDir: selection.projectDir } : {}),
    }),
    AWARENESS_SKILL_NAME
  );
}

export type CanonicalSkillStatus =
  | 'installed'
  | 'outdated'
  | 'missing'
  | 'invalid';

export type SkillDestinationCheckStatus =
  | 'linked'
  | 'copied'
  | 'missing'
  | 'broken'
  | 'drifted';

export function inspectCanonicalSkill(options: SkillCommandOptions): {
  source: string;
  canonical: string;
  status: CanonicalSkillStatus;
} {
  const source = bundledSkillPath(options);
  const canonical = canonicalSkillPath(options);
  if (!pathExists(canonical)) return { source, canonical, status: 'missing' };
  const canonicalStat = lstatSync(canonical);
  const skillFile = join(canonical, 'SKILL.md');
  if (
    canonicalStat.isSymbolicLink() ||
    !canonicalStat.isDirectory() ||
    !existsSync(skillFile) ||
    !lstatSync(skillFile).isFile()
  ) {
    return { source, canonical, status: 'invalid' };
  }
  return {
    source,
    canonical,
    status: skillTreesEqual(source, canonical) ? 'installed' : 'outdated',
  };
}

export function inspectSkillDestination(
  destination: string,
  canonical: string
): SkillDestinationCheckStatus {
  if (!pathExists(destination)) return 'missing';
  const stat = lstatSync(destination);
  if (stat.isSymbolicLink()) {
    if (!existsSync(destination)) return 'broken';
    return linkPointsTo(destination, canonical) ? 'linked' : 'drifted';
  }
  if (!stat.isDirectory() || !existsSync(join(destination, 'SKILL.md')))
    return 'drifted';
  return pathExists(canonical) && skillTreesEqual(destination, canonical)
    ? 'copied'
    : 'drifted';
}
