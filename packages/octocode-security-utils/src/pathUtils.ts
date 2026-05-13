/**
 * Path redaction utilities for safe error messages.
 * Normalizes and redacts filesystem paths to prevent information leakage.
 */

import path from 'path';
import os from 'os';
import { resolveWorkspaceRoot } from './workspaceRoot.js';

/**
 * Normalizes a path for consistent cross-platform comparison.
 * Converts backslashes to forward slashes and resolves . / .. / double-slashes.
 */
function normalizePath(p: string): string {
  if (!p) return p;
  const normalized = path.posix.normalize(p.replace(/\\/g, '/'));
  // Strip trailing slash (except for root '/') so getRelativeIfChild works cleanly
  return normalized.length > 1 && normalized.endsWith('/')
    ? normalized.slice(0, -1)
    : normalized;
}

/**
 * Returns the relative portion of `child` inside `parent`, or null if not contained.
 * Ensures directory-boundary matching (prevents prefix collisions).
 * Inputs must be pre-normalized via normalizePath().
 */
function getRelativeIfChild(child: string, parent: string): string | null {
  if (child === parent) return '.';

  const parentPrefix = parent + '/';
  if (child.startsWith(parentPrefix)) {
    return child.slice(parentPrefix.length);
  }

  return null;
}

/** Cached home directory (never changes during process lifetime) */
const HOME_DIR = normalizePath(os.homedir());

/**
 * Redacts a filesystem path for safe inclusion in error messages.
 *
 * Resolution order:
 * 1. Within workspaceRoot → project-relative path (e.g. src/file.ts)
 * 2. Within home directory → ~/... (defense-in-depth)
 * 3. Outside all roots → filename only (defense-in-depth)
 *
 * Security: normalizes ../ and backslashes, boundary-safe prefix matching.
 * Cross-platform: Windows, macOS, Linux.
 *
 * @param absolutePath - The full path to redact
 * @param workspaceRoot - Optional workspace root (resolved from config/CWD if omitted)
 * @returns Redacted path string safe for error messages
 *
 * @example
 * ```ts
 * redactPath('/home/alice/project/src/index.ts', '/home/alice/project');
 * // → 'src/index.ts'
 * redactPath('/opt/system/config.yaml');
 * // → 'config.yaml'
 * ```
 */
export function redactPath(
  absolutePath: string,
  workspaceRoot?: string
): string {
  if (!absolutePath) return '';

  const normalized = normalizePath(absolutePath);
  const rootSource = workspaceRoot ?? resolveWorkspaceRoot();
  const root = normalizePath(rootSource);

  // Primary: show project-relative path
  const relative = getRelativeIfChild(normalized, root);
  if (relative !== null) return relative;

  // Defense-in-depth: show ~/... for home directory paths
  if (HOME_DIR) {
    const homeRelative = getRelativeIfChild(normalized, HOME_DIR);
    if (homeRelative !== null) {
      return homeRelative === '.' ? '~' : '~/' + homeRelative;
    }
  }

  // Last resort: filename only
  return path.basename(normalized);
}
