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
} from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

const SKILL_NAME = 'octocode-awareness';

const PLATFORM_DIRS = {
  shared: { global: '.agents/skills', project: '.agents/skills' },
  codex: { global: '.agents/skills', project: '.agents/skills' },
  'codex-native': { global: '.codex/skills' },
  claude: { global: '.claude/skills', project: '.claude/skills' },
  'claude-desktop': { global: '.claude-desktop/skills' },
  cursor: { global: '.cursor/skills', project: '.cursor/skills' },
  opencode: { global: '.config/opencode/skills', project: '.opencode/skills' },
  pi: { global: '.pi/agent/skills', project: '.pi/skills' },
  copilot: { global: '.copilot/skills', project: '.github/skills' },
  gemini: { global: '.gemini/skills', project: '.gemini/skills' },
} as const;

type Platform = keyof typeof PLATFORM_DIRS;
type Scope = 'global' | 'project';

export interface SkillInstallOptions {
  skillsDir: string;
  cwd?: string;
  homeDir?: string;
}

export interface SkillInstallResult {
  exitCode: number;
  payload: Record<string, unknown>;
}

function fail(error: string, details: Record<string, unknown> = {}): SkillInstallResult {
  return { exitCode: 1, payload: { ok: false, error, ...details } };
}

function option(argv: string[], name: string): string | undefined {
  const prefix = `${name}=`;
  const inline = argv.find((arg) => arg.startsWith(prefix));
  if (inline) return inline.slice(prefix.length);
  const index = argv.indexOf(name);
  if (index < 0) return undefined;
  const value = argv[index + 1];
  return value && !value.startsWith('--') ? value : undefined;
}

function flag(argv: string[], name: string): boolean {
  return argv.includes(name);
}

function sameTree(left: string, right: string): boolean {
  if (!existsSync(left) || !existsSync(right)) return false;
  const leftStat = lstatSync(left);
  const rightStat = lstatSync(right);
  if (leftStat.isSymbolicLink() || rightStat.isSymbolicLink()) {
    return leftStat.isSymbolicLink() && rightStat.isSymbolicLink()
      && readlinkSync(left) === readlinkSync(right);
  }
  if (leftStat.isFile() || rightStat.isFile()) {
    return leftStat.isFile() && rightStat.isFile()
      && readFileSync(left).equals(readFileSync(right));
  }
  if (!leftStat.isDirectory() || !rightStat.isDirectory()) return false;
  const leftEntries = readdirSync(left).sort();
  const rightEntries = readdirSync(right).sort();
  return leftEntries.length === rightEntries.length
    && leftEntries.every((name, index) => name === rightEntries[index]
      && sameTree(join(left, name), join(right, name)));
}

function replaceDirectory(source: string, destination: string): void {
  const parent = dirname(destination);
  mkdirSync(parent, { recursive: true });
  const stagingRoot = mkdtempSync(join(parent, '.octocode-awareness-install-'));
  const staged = join(stagingRoot, SKILL_NAME);
  const backup = join(parent, `.octocode-awareness-backup-${process.pid}-${Date.now()}`);
  let backedUp = false;
  try {
    cpSync(source, staged, { recursive: true });
    if (existsSync(destination)) {
      renameSync(destination, backup);
      backedUp = true;
    }
    renameSync(staged, destination);
    if (backedUp) rmSync(backup, { recursive: true, force: true });
  } catch (error) {
    if (backedUp && !existsSync(destination) && existsSync(backup)) renameSync(backup, destination);
    throw error;
  } finally {
    rmSync(stagingRoot, { recursive: true, force: true });
  }
}

export function runSkillInstall(argv: string[], options: SkillInstallOptions): SkillInstallResult {
  const platformValue = option(argv, '--platform');
  if (!platformValue) {
    return fail(`--platform is required (${Object.keys(PLATFORM_DIRS).join('|')})`);
  }
  if (!(platformValue in PLATFORM_DIRS)) {
    return fail(`unsupported platform "${platformValue}"`, { supported_platforms: Object.keys(PLATFORM_DIRS) });
  }
  const platform = platformValue as Platform;
  const global = flag(argv, '--global');
  const projectDirValue = option(argv, '--project-dir');
  if (global && projectDirValue) return fail('use either --global or --project-dir, not both');
  if (!global && !projectDirValue) return fail('choose an explicit scope with --global or --project-dir <path>');

  const scope: Scope = global ? 'global' : 'project';
  const relativeDir = PLATFORM_DIRS[platform][scope as keyof (typeof PLATFORM_DIRS)[Platform]];
  if (!relativeDir) return fail(`${platform} does not support ${scope} skill installation`);

  const root = global
    ? (options.homeDir ?? homedir())
    : resolve(options.cwd ?? process.cwd(), projectDirValue!);
  if (!global && !existsSync(root)) return fail(`project directory does not exist: ${root}`);

  const source = join(resolve(options.skillsDir), SKILL_NAME);
  if (!existsSync(join(source, 'SKILL.md'))) {
    return fail(`bundled skill is missing: ${join(source, 'SKILL.md')}`);
  }
  const destination = join(root, relativeDir, SKILL_NAME);
  const identical = sameTree(source, destination);
  const exists = existsSync(destination);
  const dryRun = flag(argv, '--dry-run');
  const force = flag(argv, '--force');

  if (dryRun) {
    return {
      exitCode: 0,
      payload: {
        ok: true,
        action: 'dry-run',
        skill: SKILL_NAME,
        platform,
        scope,
        source,
        destination,
        changed: !identical,
        conflict: exists && !identical,
        requires_force: exists && !identical,
        next: exists && !identical
          ? 'Review the destination, then rerun with --force to replace it.'
          : `Rerun without --dry-run to install ${SKILL_NAME}.`,
      },
    };
  }

  if (identical) {
    return {
      exitCode: 0,
      payload: { ok: true, action: 'install', skill: SKILL_NAME, platform, scope, source, destination, changed: false },
    };
  }
  if (exists && !force) {
    return fail(`destination already exists and differs; inspect it, then rerun with --force to replace it`, {
      skill: SKILL_NAME,
      platform,
      scope,
      source,
      destination,
    });
  }

  try {
    replaceDirectory(source, destination);
    return {
      exitCode: 0,
      payload: {
        ok: true,
        action: 'install',
        skill: SKILL_NAME,
        platform,
        scope,
        source,
        destination,
        changed: true,
        next: 'Restart or reload the agent host, then use npx @octocodeai/octocode-awareness attend --compact.',
      },
    };
  } catch (error) {
    return fail(`cannot install ${SKILL_NAME}: ${error instanceof Error ? error.message : String(error)}`, {
      platform,
      scope,
      source,
      destination,
    });
  }
}
