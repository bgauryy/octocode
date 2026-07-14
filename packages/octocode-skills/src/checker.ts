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
import { getSkillsHome } from './home.js';
import { getPlatformSkillsDir, type Platform } from './platforms.js';

// ─── Types ────────────────────────────────────────────────────────────────────

export type LocationStatus = 'installed' | 'linked' | 'broken' | 'missing';

export interface CheckedLocation {
  label: string;
  path: string;
  status: LocationStatus;
  /** Resolved symlink target (symlinks only) */
  linkTarget?: string;
}

export interface SkillCheckResult {
  skillName: string;
  home: CheckedLocation;
  platforms: CheckedLocation[];
  workspace: CheckedLocation;
}

// ─── All platforms to scan by default ────────────────────────────────────────

export const SCAN_PLATFORMS: Platform[] = [
  'pi',
  'cursor',
  'claude',
  'codex',
  'opencode',
  'copilot',
  'gemini',
  'agents',
];

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
      const resolved = path.isAbsolute(target) ? target : path.resolve(path.dirname(p), target);
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
  platforms: Platform[] = SCAN_PLATFORMS
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

  return {
    skillName,
    home: probe('home', homePath),
    platforms: platformChecks,
    workspace: probe('workspace', wsPath),
  };
}

/** Check a list of skills. */
export function checkSkills(
  skillNames: string[],
  platforms?: Platform[]
): SkillCheckResult[] {
  return skillNames.map((n) => checkSkill(n, platforms));
}

// ─── Derived helpers ──────────────────────────────────────────────────────────

/** True when the skill is present at home (real copy or valid symlink). */
export function isInstalledAtHome(r: SkillCheckResult): boolean {
  return r.home.status === 'installed' || r.home.status === 'linked';
}

/** Platform labels where the skill is linked or installed. */
export function linkedPlatforms(r: SkillCheckResult): string[] {
  return r.platforms
    .filter((p) => p.status === 'linked' || p.status === 'installed')
    .map((p) => p.label);
}

/** Any location has a broken symlink. */
export function hasBroken(r: SkillCheckResult): boolean {
  return (
    r.home.status === 'broken' ||
    r.workspace.status === 'broken' ||
    r.platforms.some((p) => p.status === 'broken')
  );
}

/** Overall health: ok | partial | not-installed */
export function overallStatus(r: SkillCheckResult): 'ok' | 'broken' | 'not-installed' {
  if (hasBroken(r)) return 'broken';
  if (isInstalledAtHome(r)) return 'ok';
  return 'not-installed';
}
