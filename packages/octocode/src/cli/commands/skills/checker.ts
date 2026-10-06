/**
 * Check installation status of skills across all known locations.
 *
 * Checks:
 *   - Canonical home: ~/.octocode/skills/<name>/
 *   - All platform dirs: ~/.pi/agent/skills/<name>, ~/.cursor/skills/<name>, …
 *   - Workspace: <cwd>/.agents/skills/<name>
 *
 * For each path reports: installed (real dir) | linked (valid symlink) |
 *                        broken (dangling symlink) | missing
 */

import fs from 'node:fs';
import path from 'node:path';
import { contentFreshness, type Freshness } from './freshness.js';
import { getSkillsHome } from './home.js';
import { ALL_PLATFORMS, getPlatformSkillsDir } from './platforms.js';
import { getSkill } from './registry.js';
import type { SkillPlatform } from '@octocodeai/octocode-skill-installer';

// ─── Types ────────────────────────────────────────────────────────────────────

export type LocationStatus = 'installed' | 'linked' | 'broken' | 'missing';

export interface CheckedLocation {
  label: string;
  path: string;
  status: LocationStatus;
  /** Resolved symlink target (symlinks only) */
  linkTarget?: string;
  /**
   * Content comparison against the bundled skill source (present locations
   * of bundled skills only; omitted when it cannot be determined).
   */
  content?: Freshness;
}

export interface SkillCheckResult {
  skillName: string;
  home: CheckedLocation;
  platforms: CheckedLocation[];
  workspace: CheckedLocation;
}

// ─── All platforms to scan by default ────────────────────────────────────────

export const SCAN_PLATFORMS: SkillPlatform[] = [...ALL_PLATFORMS];

// ─── Internals ────────────────────────────────────────────────────────────────

function probe(label: string, p: string): CheckedLocation {
  try {
    if (!fs.existsSync(p)) {
      // existsSync follows symlinks — if false, either missing or broken link
      // Check for broken symlink specifically
      try {
        fs.lstatSync(p); // lstat doesn't follow links
        // lstat succeeded but existsSync failed → dangling symlink
        const target = fs.readlinkSync(p);
        return {
          label,
          path: p,
          status: 'broken',
          linkTarget: path.resolve(path.dirname(p), target),
        };
      } catch {
        return { label, path: p, status: 'missing' };
      }
    }

    const lstat = fs.lstatSync(p);
    if (lstat.isSymbolicLink()) {
      const target = fs.readlinkSync(p);
      const resolved = path.isAbsolute(target)
        ? target
        : path.resolve(path.dirname(p), target);
      return { label, path: p, status: 'linked', linkTarget: resolved };
    }

    return { label, path: p, status: 'installed' };
  } catch {
    return { label, path: p, status: 'missing' };
  }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/** Check every known installation location for one skill. */
export function checkSkill(
  skillName: string,
  platforms: SkillPlatform[] = SCAN_PLATFORMS
): SkillCheckResult {
  const homePath = path.join(getSkillsHome(), skillName);
  const wsPath = path.join(process.cwd(), '.agents', 'skills', skillName);

  const platformChecks: CheckedLocation[] = [];
  const seen = new Set<string>();

  for (const platform of platforms) {
    const dir = getPlatformSkillsDir(platform);
    const p = path.join(dir, skillName);
    if (seen.has(p)) continue; // collapse duplicates (codex = agents = common)
    seen.add(p);
    platformChecks.push(probe(platform, p));
  }

  const result: SkillCheckResult = {
    skillName,
    home: probe('home', homePath),
    platforms: platformChecks,
    workspace: probe('workspace', wsPath),
  };
  annotateFreshness(result);
  return result;
}

/**
 * Compare each Octocode-managed location of a bundled skill against its
 * source. A link resolving outside the skills home (a checkout the user
 * linked by hand) is theirs: it is neither compared nor repaired.
 */
function annotateFreshness(result: SkillCheckResult): void {
  const bundled = getSkill(result.skillName);
  if (!bundled) return;
  let skillsHome: string;
  try {
    skillsHome = fs.realpathSync(getSkillsHome());
  } catch {
    skillsHome = path.resolve(getSkillsHome());
  }
  const compared = new Map<string, Freshness | undefined>();
  for (const location of [result.home, ...result.platforms, result.workspace]) {
    if (location.status !== 'installed' && location.status !== 'linked') {
      continue;
    }
    let real: string;
    try {
      real = fs.realpathSync(location.path);
    } catch {
      continue;
    }
    if (
      location.status === 'linked' &&
      !real.startsWith(skillsHome + path.sep)
    ) {
      continue;
    }
    if (!compared.has(real))
      compared.set(real, contentFreshness(bundled.dir, real));
    const freshness = compared.get(real);
    if (freshness) location.content = freshness;
  }
}

/** Check a list of skills. */
export function checkSkills(
  skillNames: string[],
  platforms?: SkillPlatform[]
): SkillCheckResult[] {
  return skillNames.map(n => checkSkill(n, platforms));
}

// ─── Derived helpers ──────────────────────────────────────────────────────────

const present = (location: CheckedLocation): boolean =>
  location.status === 'installed' || location.status === 'linked';

/** True when the skill is present anywhere: home, a platform, or the workspace. */
export function isInstalled(r: SkillCheckResult): boolean {
  return [r.home, ...r.platforms, r.workspace].some(present);
}

/** Platform labels where the skill is linked or installed. */
export function linkedPlatforms(r: SkillCheckResult): string[] {
  return r.platforms
    .filter(p => p.status === 'linked' || p.status === 'installed')
    .map(p => p.label);
}

/** Any location has a broken symlink. */
export function hasBroken(r: SkillCheckResult): boolean {
  return (
    r.home.status === 'broken' ||
    r.workspace.status === 'broken' ||
    r.platforms.some(p => p.status === 'broken')
  );
}

/** Any present location's content differs from the bundled source. */
export function hasStale(r: SkillCheckResult): boolean {
  return [r.home, ...r.platforms, r.workspace].some(
    location => location.content === 'stale'
  );
}

export type SkillStatus = 'ok' | 'broken' | 'stale' | 'not-installed';

/** One status for `skill list` and `skill check`: broken > stale > ok > not-installed. */
export function overallStatus(r: SkillCheckResult): SkillStatus {
  if (hasBroken(r)) return 'broken';
  if (hasStale(r)) return 'stale';
  return isInstalled(r) ? 'ok' : 'not-installed';
}
