/**
 * Core skill installation logic.
 *
 * Installation model:
 *   1. The canonical copy lives at ~/.octocode/skills/<name>/ (home step).
 *   2. Platform / workspace directories receive a symlink pointing to that copy.
 *   3. --path bypasses the home step and writes directly to the target dir.
 *
 * This approach ensures:
 *   - A single source of truth for each skill (no copies drifting apart).
 *   - Updating the home copy automatically updates all linked locations.
 *   - Platform dirs stay clean — they just contain symlinks.
 */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { getPlatformSkillsDir, type Platform } from './platforms.js';
import { getSkillsHome } from './home.js';

const isWindows = os.platform() === 'win32';

// ─── Types ────────────────────────────────────────────────────────────────────

/** copy = always copy, symlink = always symlink, hybrid = copy for claude targets, symlink for all others */
export type InstallMode = 'copy' | 'symlink' | 'hybrid';

/** Resolve the effective write mode for a specific platform when mode is hybrid */
function resolveEffectiveMode(mode: InstallMode, platform: string): 'copy' | 'symlink' {
  if (mode === 'hybrid' && (platform === 'claude' || platform === 'claude-desktop')) return 'copy';
  if (mode === 'copy') return 'copy';
  return 'symlink';
}

export type LinkStatus = 'linked' | 'skipped' | 'failed';

export interface LinkResult {
  /** Human label for this link target */
  target: string;
  /** Absolute path of the link / copy */
  destPath: string;
  status: LinkStatus;
  error?: string;
}

export interface SkillInstallOutcome {
  skillName: string;
  /** Canonical home path (null when --path skips home step) */
  homePath: string | null;
  homeStatus: 'installed' | 'skipped' | 'failed' | 'bypassed';
  homeError?: string;
  /** Symlinks / copies created in platform / workspace dirs */
  links: LinkResult[];
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

function copyDir(src: string, dest: string): void {
  fs.mkdirSync(dest, { recursive: true });
  const entries = fs.readdirSync(src, { withFileTypes: true });

  for (const entry of entries) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);

    if (entry.isDirectory()) {
      copyDir(srcPath, destPath);
    } else if (entry.isFile()) {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

function makeSymlink(targetPath: string, linkPath: string): void {
  fs.symlinkSync(targetPath, linkPath, isWindows ? 'junction' : 'dir');
}

function ensureParent(p: string): void {
  fs.mkdirSync(path.dirname(p), { recursive: true });
}

function removeIfExists(p: string, force: boolean): 'removed' | 'present' | 'absent' {
  try {
    fs.lstatSync(p); // lstat succeeds for broken symlinks; existsSync does not
  } catch {
    return 'absent';
  }
  if (!force) return 'present';
  fs.rmSync(p, { recursive: true, force: true });
  return 'removed';
}

// ─── Core install step ────────────────────────────────────────────────────────

interface InstallToDestParams {
  sourcePath: string;
  destPath: string;
  mode: InstallMode;
  force: boolean;
  label: string;
}

function installToDest(params: InstallToDestParams): LinkResult {
  const { sourcePath, destPath, mode, force, label } = params;

  try {
    if (!fs.existsSync(sourcePath)) {
      return { target: label, destPath, status: 'failed', error: `Source not found: ${sourcePath}` };
    }

    const existing = removeIfExists(destPath, force);
    if (existing === 'present') {
      return { target: label, destPath, status: 'skipped' };
    }

    ensureParent(destPath);

    if (mode === 'symlink') {
      makeSymlink(sourcePath, destPath);
    } else {
      copyDir(sourcePath, destPath);
    }

    return { target: label, destPath, status: 'linked' };
  } catch (err) {
    return {
      target: label,
      destPath,
      status: 'failed',
      error: err instanceof Error ? err.message : String(err),
    };
  }
}

// ─── Public API ───────────────────────────────────────────────────────────────

export interface InstallSkillParams {
  /** Absolute path to the bundled source skill directory */
  sourcePath: string;
  /** Skill name (folder name, e.g. "octocode-research") */
  skillName: string;
  /** Target platforms to link into */
  platforms: Platform[];
  /** Also link into <cwd>/.agents/skills/ */
  workspace: boolean;
  /** Custom install path — when set, skips the home step */
  customPath: string | null;
  /** Copy instead of symlink (home step always copies; links use symlink) */
  mode: InstallMode;
  /** Overwrite existing installations */
  force: boolean;
  /** Preview only — do not write anything */
  dryRun: boolean;
}

export function installSkill(params: InstallSkillParams): SkillInstallOutcome {
  const {
    sourcePath,
    skillName,
    platforms,
    workspace,
    customPath,
    mode,
    force,
    dryRun,
  } = params;

  // ── Step 1: Canonical home ─────────────────────────────────────────────────

  let homePath: string | null = null;
  let homeStatus: SkillInstallOutcome['homeStatus'] = 'bypassed';
  let homeError: string | undefined;

  if (!customPath) {
    homePath = path.join(getSkillsHome(), skillName);

    if (dryRun) {
      homeStatus = 'installed'; // preview
    } else {
      const result = installToDest({
        sourcePath,
        destPath: homePath,
        mode: 'copy', // home always copies (canonical)
        force,
        label: 'home',
      });

      homeStatus =
        result.status === 'linked'
          ? 'installed'
          : result.status === 'skipped'
            ? 'skipped'
            : 'failed';
      homeError = result.error;
    }
  }

  // ── Step 2: Custom path (bypasses home) ──────────────────────────────────────────

  const links: LinkResult[] = [];

  if (customPath) {
    const destPath = path.join(customPath, skillName);
    if (dryRun) {
      links.push({ target: 'custom', destPath, status: 'linked' });
    } else {
      links.push(
        installToDest({ sourcePath, destPath, mode, force, label: 'custom' })
      );
    }
  }

  // ── Step 3: Platform symlinks ──────────────────────────────────────────────

  if (!customPath && homePath && (homeStatus === 'installed' || homeStatus === 'skipped')) {
    for (const platform of platforms) {
      const platformDir = getPlatformSkillsDir(platform);
      const linkPath = path.join(platformDir, skillName);

      if (dryRun) {
        links.push({ target: platform, destPath: linkPath, status: 'linked' });
        continue;
      }

      const effectiveMode = resolveEffectiveMode(mode, platform);
      if (effectiveMode === 'copy') {
        links.push(
          installToDest({ sourcePath, destPath: linkPath, mode: 'copy', force, label: platform })
        );
      } else {
        links.push(
          createLink({ targetPath: homePath, linkPath, label: platform, force })
        );
      }
    }
  }

  // ── Step 4: Workspace symlink ─────────────────────────────────────────────

  if (!customPath && workspace && homePath && (homeStatus === 'installed' || homeStatus === 'skipped')) {
    const wsLinkPath = path.join(process.cwd(), '.agents', 'skills', skillName);

    if (dryRun) {
      links.push({ target: 'workspace', destPath: wsLinkPath, status: 'linked' });
    } else {
      links.push(
        createLink({ targetPath: homePath, linkPath: wsLinkPath, label: 'workspace', force })
      );
    }
  }

  return {
    skillName,
    homePath,
    homeStatus,
    ...(homeError !== undefined ? { homeError } : {}),
    links,
  };
}

/** Create a symlink (linkPath → targetPath). */
function createLink(params: {
  targetPath: string;
  linkPath: string;
  label: string;
  force: boolean;
}): LinkResult {
  const { targetPath, linkPath, label, force } = params;

  try {
    const existing = removeIfExists(linkPath, force);
    if (existing === 'present') {
      return { target: label, destPath: linkPath, status: 'skipped' };
    }

    ensureParent(linkPath);
    makeSymlink(targetPath, linkPath);
    return { target: label, destPath: linkPath, status: 'linked' };
  } catch (err) {
    return {
      target: label,
      destPath: linkPath,
      status: 'failed',
      error: err instanceof Error ? err.message : String(err),
    };
  }
}
