/**
 * Execution context validation — prevents command execution outside workspace
 */

import path from 'path';
import fs from 'fs';
import type { PathValidationResult } from './types.js';
import { resolveWorkspaceRoot } from './workspaceRoot.js';
import { securityRegistry } from './registry.js';

/**
 * Validates that a command execution context (cwd) is within the workspace directory.
 * Prevents commands from being executed in parent directories or arbitrary locations.
 *
 * Allowed roots:
 *   - The resolved workspace root
 *   - Any roots registered via securityRegistry.addAllowedRoots()
 *
 * @param cwd - The current working directory where the command will execute
 * @param workspaceRoot - Optional workspace root override
 * @param additionalRoots - Extra root directories to allow
 *
 * @example
 * ```ts
 * validateExecutionContext('/app/packages/core');
 * // → { isValid: true, sanitizedPath: '/app/packages/core' }
 * validateExecutionContext('/etc');
 * // → { isValid: false, error: 'Can only execute commands within ...' }
 * ```
 */
export function validateExecutionContext(
  cwd: string | undefined,
  workspaceRoot?: string,
  additionalRoots?: string[]
): PathValidationResult {
  const workspace = resolveWorkspaceRoot(workspaceRoot);

  if (cwd === undefined) {
    return { isValid: true };
  }

  if (cwd.trim() === '') {
    return {
      isValid: false,
      error: 'Execution context (cwd) cannot be empty',
    };
  }

  const absoluteCwd = path.resolve(cwd);

  const allowedRoots = [workspace];
  for (const root of [
    ...(additionalRoots ?? []),
    ...securityRegistry.extraAllowedRoots,
  ]) {
    const resolved = path.resolve(root);
    if (!allowedRoots.includes(resolved)) {
      allowedRoots.push(resolved);
    }
  }

  const isInAllowedRoot = allowedRoots.some(
    root => absoluteCwd === root || absoluteCwd.startsWith(root + path.sep)
  );

  if (!isInAllowedRoot) {
    return {
      isValid: false,
      error:
        'Can only execute commands within the configured workspace directory',
    };
  }

  try {
    fs.lstatSync(absoluteCwd);
    const realPath = fs.realpathSync(absoluteCwd);

    const isRealPathAllowed = allowedRoots.some(
      root => realPath === root || realPath.startsWith(root + path.sep)
    );
    if (!isRealPathAllowed) {
      return {
        isValid: false,
        error: 'Symlink target is outside the configured workspace directory',
      };
    }
  } catch (error) {
    if (
      error &&
      typeof error === 'object' &&
      'code' in error &&
      error.code === 'ENOENT'
    ) {
      return {
        isValid: true,
        sanitizedPath: absoluteCwd,
      };
    }
    return {
      isValid: false,
      error: `Cannot validate execution context: ${error instanceof Error ? error.name : 'unknown error'}`,
    };
  }

  return {
    isValid: true,
    sanitizedPath: absoluteCwd,
  };
}
