import { existsSync, mkdirSync, rmSync, cpSync, symlinkSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

import { getOctocodeHome } from '@octocodeai/config';
import { executeDirectTool } from '@octocodeai/octocode-tools-core/direct';

import { EXIT } from '../exit-codes.js';
import { c, bold, dim } from '../../utils/colors.js';

// ─── Skill source constants ──────────────────────────────────────────────────

export const NAMED_SKILL_OWNER = 'bgauryy';
export const NAMED_SKILL_REPO = 'octocode';
export const NAMED_SKILL_BASE_PATH = 'skills';

// ─── Types ───────────────────────────────────────────────────────────────────

export type InstallMode = 'symlink' | 'copy' | 'hybrid';
export type PlatformEntry = { platform: string; dir: string };

export type SkillInstallEntry = {
  name: string;
  canonicalDir: string;
  destinations: PlatformEntry[];
  mode: InstallMode;
  force: boolean;
  dryRun: boolean;
  verbose: boolean;
};

export type DestinationResult = {
  dir: string;
  platform: string;
  action: 'install' | 'skip' | 'overwrite';
  success: boolean;
  error?: string;
};

export type SkillResult = {
  name: string;
  canonicalDir: string;
  sourceFetched: boolean;
  mode: InstallMode;
  destinations: DestinationResult[];
};

export type GitHubRef = {
  owner: string;
  repo: string;
  path: string;
  branch?: string;
};

// ─── Platform path resolution ─────────────────────────────────────────────────

export function platformDirs(
  platform: string,
  skillName: string
): PlatformEntry[] {
  const home = homedir();

  const ALL_PLATFORM_DIRS: Record<string, string[]> = {
    common: [join(home, '.agents', 'skills', skillName)],
    cursor: [join(home, '.cursor', 'skills', skillName)],
    claude: [
      join(home, '.claude', 'skills', skillName),
      join(home, '.claude-desktop', 'skills', skillName),
    ],
    codex: [join(home, '.agents', 'skills', skillName)],
    opencode: [join(home, '.config', 'opencode', 'skills', skillName)],
    pi: [join(home, '.pi', 'agent', 'skills', skillName)],
    copilot: [join(home, '.copilot', 'skills', skillName)],
    gemini: [join(home, '.gemini', 'skills', skillName)],
  };

  const requested =
    platform === 'all'
      ? Object.keys(ALL_PLATFORM_DIRS)
      : platform
          .split(',')
          .map(p => p.trim())
          .filter(Boolean);

  const results: PlatformEntry[] = [];
  const seen = new Set<string>();

  for (const plat of requested) {
    const dirs = ALL_PLATFORM_DIRS[plat] ?? [
      join(home, `.${plat}`, 'skills', skillName),
    ];
    for (const dir of dirs) {
      const normalized = resolve(dir);
      if (!seen.has(normalized)) {
        seen.add(normalized);
        results.push({ platform: plat, dir });
      }
    }
  }

  return results;
}

// ─── Install helpers ──────────────────────────────────────────────────────────

export function isCopyMode(plat: string, mode: InstallMode): boolean {
  if (mode === 'copy') return true;
  if (mode === 'hybrid') return plat === 'claude';
  return false; // symlink
}

export function clonedLocalPath(
  structuredContent: unknown
): string | undefined {
  const sc = structuredContent as {
    results?: Array<{ data?: { localPath?: string } }>;
  };
  return sc?.results?.[0]?.data?.localPath;
}

export function canonicalSkillsRoot(): string {
  return join(getOctocodeHome(), 'skills');
}

export function refreshCanonical(
  localPath: string,
  canonicalDir: string,
  force: boolean
): void {
  if (existsSync(canonicalDir)) {
    if (!force) return; // already present, don't overwrite without --force
    rmSync(canonicalDir, { recursive: true, force: true });
  }
  mkdirSync(join(canonicalDir, '..'), { recursive: true });
  cpSync(localPath, canonicalDir, { recursive: true });
}

export function installDestination(
  canonicalDir: string,
  entry: PlatformEntry,
  mode: InstallMode,
  force: boolean,
  dryRun: boolean
): DestinationResult {
  const { dir, platform } = entry;
  const copy = isCopyMode(platform, mode);

  const action: DestinationResult['action'] = existsSync(dir)
    ? force
      ? 'overwrite'
      : 'skip'
    : 'install';

  if (dryRun || action === 'skip') {
    return { dir, platform, action, success: true };
  }

  try {
    if (action === 'overwrite') {
      rmSync(dir, { recursive: true, force: true });
    }
    mkdirSync(join(dir, '..'), { recursive: true });

    if (copy) {
      cpSync(canonicalDir, dir, { recursive: true });
    } else {
      symlinkSync(canonicalDir, dir, 'junction');
    }

    return { dir, platform, action, success: true };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { dir, platform, action, success: false, error: message };
  }
}

// ─── List ─────────────────────────────────────────────────────────────────────

export async function runList(jsonOutput: boolean): Promise<number> {
  try {
    const result = await executeDirectTool('ghViewRepoStructure', {
      queries: [
        {
          owner: NAMED_SKILL_OWNER,
          repo: NAMED_SKILL_REPO,
          path: NAMED_SKILL_BASE_PATH,
          maxDepth: 1,
          directoriesOnly: true,
          reasoning: 'List available Octocode named skills',
        },
      ],
    });

    const sc = result.structuredContent as {
      results?: Array<{
        data?: { folders?: string[]; structure?: Array<{ name: string }> };
      }>;
    };
    const data = sc?.results?.[0]?.data;
    const skills: string[] = data?.folders ?? [];

    if (jsonOutput) {
      console.log(JSON.stringify({ skills }, null, 2));
    } else {
      if (skills.length === 0) {
        console.log(`\n  ${dim('No skills found.')}\n`);
      } else {
        console.log(`\n  ${bold('Available Octocode Skills')}\n`);
        for (const skill of skills) {
          console.log(`    ${c('cyan', skill)}`);
        }
        console.log(
          `\n  ${dim('Install with:')} skill --name <name> [--platform cursor|claude|all]\n`
        );
      }
    }

    return EXIT.OK;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    if (jsonOutput) {
      console.log(JSON.stringify({ success: false, error: message }));
    } else {
      console.error(
        `\n  ${c('red', '✗')} Failed to fetch skill list: ${message}\n`
      );
    }
    return EXIT.GENERAL;
  }
}

// ─── Install ──────────────────────────────────────────────────────────────────

export async function fetchAndInstallSkill(
  entry: SkillInstallEntry,
  sourceOwner: string,
  sourceRepo: string,
  sourcePath: string,
  sourceBranch?: string
): Promise<SkillResult> {
  const {
    name,
    canonicalDir,
    destinations,
    mode,
    force,
    dryRun,
    verbose: _verbose,
  } = entry;

  const result: SkillResult = {
    name,
    canonicalDir,
    sourceFetched: false,
    mode,
    destinations: [],
  };

  // Fetch from GitHub (unless dry-run and canonical already present)
  if (!dryRun || !existsSync(canonicalDir)) {
    try {
      const cloneResult = await executeDirectTool('ghCloneRepo', {
        queries: [
          {
            owner: sourceOwner,
            repo: sourceRepo,
            branch: sourceBranch || undefined,
            sparsePath: sourcePath,
            forceRefresh: force || undefined,
            mainResearchGoal: `Install skill ${name}`,
            researchGoal: `Fetch skill ${name} from ${sourceOwner}/${sourceRepo}/${sourcePath}`,
            reasoning: 'skill install command',
          },
        ],
      });

      if (cloneResult.isError) {
        const errText =
          (cloneResult.content as Array<{ text?: string }> | undefined)?.[0]
            ?.text ?? 'Clone failed';
        result.destinations = destinations.map(d => ({
          dir: d.dir,
          platform: d.platform,
          action: 'install' as const,
          success: false,
          error: errText,
        }));
        return result;
      }

      const localPath = clonedLocalPath(cloneResult.structuredContent);
      if (!localPath) {
        result.destinations = destinations.map(d => ({
          dir: d.dir,
          platform: d.platform,
          action: 'install' as const,
          success: false,
          error: 'Clone succeeded but no localPath returned',
        }));
        return result;
      }

      if (!dryRun) {
        refreshCanonical(localPath, canonicalDir, force);
      }
      result.sourceFetched = true;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      result.destinations = destinations.map(d => ({
        dir: d.dir,
        platform: d.platform,
        action: 'install' as const,
        success: false,
        error: message,
      }));
      return result;
    }
  } else {
    result.sourceFetched = false; // dry-run, existing canonical
  }

  // Install destinations
  result.destinations = destinations.map(d =>
    installDestination(canonicalDir, d, mode, force, dryRun)
  );

  return result;
}

// ─── Parse GitHub path ────────────────────────────────────────────────────────

export function parseAddPath(
  raw: string,
  branchOverride?: string
): GitHubRef | null {
  // Strip https://github.com/ prefix if present
  let s = raw.trim();
  const ghPrefix = 'https://github.com/';
  if (s.startsWith(ghPrefix)) s = s.slice(ghPrefix.length);

  // owner/repo/path[@branch] or owner/repo@branch/path (both accepted)
  const atIdx = s.indexOf('@');
  let branch: string | undefined = branchOverride;
  if (atIdx !== -1 && !branchOverride) {
    branch = s.slice(atIdx + 1).split('/')[0];
    s = s.slice(0, atIdx) + s.slice(atIdx + 1 + branch.length);
  }

  // Remove leading /blob/main/ or /tree/main/
  s = s.replace(/\/(blob|tree)\/[^/]+\//g, '/');

  const parts = s.replace(/^\/+|\/+$/g, '').split('/');
  if (parts.length < 3) return null;

  const [owner, repo, ...rest] = parts;
  return { owner, repo, path: rest.join('/'), branch };
}

export function skillNameFromPath(path: string): string {
  return path.split('/').pop() ?? path;
}

// ─── Print results ────────────────────────────────────────────────────────────

export function printSkillResult(
  r: SkillResult,
  verbose: boolean,
  jsonOutput: boolean
): void {
  if (jsonOutput) return; // aggregated below

  const hasErrors = r.destinations.some(d => !d.success);

  console.log(
    `\n  ${hasErrors ? c('red', '✗') : c('green', '✓')} ${bold(r.name)}`
  );
  if (verbose) {
    console.log(`    ${dim('canonical')} ${r.canonicalDir}`);
  }
  for (const d of r.destinations) {
    const actionLabel =
      d.action === 'skip'
        ? dim('skip')
        : d.action === 'overwrite'
          ? c('yellow', 'overwrite')
          : c('green', 'install');
    const status = d.success ? actionLabel : c('red', 'error');
    console.log(`    ${status}  ${d.platform}  ${d.dir}`);
    if (d.error) {
      console.log(`      ${c('red', d.error)}`);
    }
  }
}
