/**
 * Unified workspace root resolution.
 *
 * Priority chain:
 *   1. Explicit parameter (if provided)
 *   2. WORKSPACE_ROOT environment variable
 *   3. process.cwd() fallback
 */

import path from 'path';
import fs from 'fs';

function isExistingDirectory(candidate: string): boolean {
  try {
    return fs.statSync(candidate).isDirectory();
  } catch {
    return false;
  }
}

/**
 * Determine workspace root from explicit param, env var, or cwd.
 *
 * @example
 * ```ts
 * resolveWorkspaceRoot('/explicit/path'); // → '/explicit/path'
 * resolveWorkspaceRoot();                  // → WORKSPACE_ROOT env or process.cwd()
 * ```
 */
export function resolveWorkspaceRoot(explicit?: string): string {
  if (explicit) {
    return path.resolve(explicit);
  }

  const envRoot = process.env.WORKSPACE_ROOT?.trim();
  if (envRoot) {
    const resolvedEnvRoot = path.resolve(envRoot);
    if (isExistingDirectory(resolvedEnvRoot)) {
      return resolvedEnvRoot;
    }
  }

  return process.cwd();
}
