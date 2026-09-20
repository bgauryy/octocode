/**
 * Content freshness of installed skills.
 *
 * The installer materializes a real copy at the canonical home
 * (~/.octocode/skills/<name>) and symlinks vendor dirs to it, so an
 * installed skill silently goes stale when the bundled package updates.
 * Freshness is a content-hash comparison between the installed copy and
 * the bundled source shipped with this package version — no separate
 * manifest to drift.
 */

import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

export type Freshness = 'fresh' | 'stale';

const IGNORED_FILES = new Set(['.DS_Store']);

function collectFiles(root: string, dir: string, out: string[]): void {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (IGNORED_FILES.has(entry.name)) continue;
    const p = path.join(dir, entry.name);
    // Follow symlinks so a linked tree hashes as its content.
    const stat = fs.statSync(p);
    if (stat.isDirectory()) collectFiles(root, p, out);
    else if (stat.isFile()) out.push(path.relative(root, p));
  }
}

/**
 * Stable content hash of a directory: sorted relative paths plus file bytes.
 * Returns null when the directory is missing or unreadable.
 */
export function hashDirContent(dir: string): string | null {
  const files: string[] = [];
  try {
    collectFiles(dir, dir, files);
  } catch {
    return null;
  }
  if (files.length === 0) return null;
  const digest = crypto.createHash('sha256');
  try {
    for (const rel of files.sort()) {
      digest.update(rel.split(path.sep).join('/'));
      digest.update('\0');
      digest.update(fs.readFileSync(path.join(dir, rel)));
      digest.update('\0');
    }
  } catch {
    return null;
  }
  return digest.digest('hex');
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
  const bundledHash = hashDirContent(bundled);
  const installedHash = hashDirContent(installed);
  if (!bundledHash || !installedHash) return undefined;
  return bundledHash === installedHash ? 'fresh' : 'stale';
}
