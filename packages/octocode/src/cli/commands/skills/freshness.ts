/**
 * Content freshness of installed skills.
 *
 * The installer materializes a real copy at the canonical home
 * (~/.octocode/skills/<name>) and symlinks vendor dirs to it, so an
 * installed skill silently goes stale when the bundled package updates.
 * Freshness compares file paths, sizes and bytes between the installed copy and
 * the bundled source shipped with this package version — no separate
 * manifest to drift.
 */

import fs from 'node:fs';
import path from 'node:path';

export type Freshness = 'fresh' | 'stale';

const IGNORED_FILES = new Set(['.DS_Store']);

function collectFiles(
  root: string,
  dir: string,
  out: Map<string, number>,
  ancestors = new Set<string>()
): void {
  const real = fs.realpathSync(dir);
  if (ancestors.has(real)) throw new Error('Skill directory symlink cycle');
  ancestors.add(real);
  try {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (IGNORED_FILES.has(entry.name)) continue;
      const p = path.join(dir, entry.name);
      const stat = fs.statSync(p);
      if (stat.isDirectory()) collectFiles(root, p, out, ancestors);
      else if (stat.isFile()) out.set(path.relative(root, p), stat.size);
    }
  } finally {
    ancestors.delete(real);
  }
}

/**
 * Compare an installed location against the bundled skill source.
 * Returns undefined when freshness cannot be determined (missing paths,
 * unreadable content) — absence of evidence is not staleness.
 */
export function contentFreshness(
  bundledDir: string,
  installedPath: string
): Freshness | undefined {
  let bundled: string;
  let installed: string;
  try {
    bundled = fs.realpathSync(bundledDir);
    installed = fs.realpathSync(installedPath);
  } catch {
    return undefined;
  }
  if (installed === bundled) return 'fresh';
  try {
    const expected = new Map<string, number>();
    const actual = new Map<string, number>();
    collectFiles(bundled, bundled, expected);
    collectFiles(installed, installed, actual);
    if (expected.size === 0 || actual.size === 0) return undefined;
    if (expected.size !== actual.size) return 'stale';
    for (const [relative, size] of expected) {
      if (actual.get(relative) !== size) return 'stale';
    }
    for (const relative of expected.keys()) {
      if (
        !fs
          .readFileSync(path.join(bundled, relative))
          .equals(fs.readFileSync(path.join(installed, relative)))
      )
        return 'stale';
    }
    return 'fresh';
  } catch {
    return undefined;
  }
}
