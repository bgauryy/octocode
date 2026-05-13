/**
 * Path validation: resolves symlinks for security checks; tool traversal may
 * disable symlink following separately (see SECURITY_DEFAULTS).
 */

import path from 'path';
import fs from 'fs';
import os from 'os';
import type { PathValidationResult } from './types.js';
import { shouldIgnore } from './ignoredPathFilter.js';
import { redactPath } from './pathUtils.js';
import { resolveWorkspaceRoot } from './workspaceRoot.js';
import { securityRegistry } from './registry.js';

/**
 * PathValidator configuration options
 */
interface PathValidatorOptions {
  /** Primary workspace root directory. Defaults to CWD. */
  workspaceRoot?: string;
  /** Additional allowed root directories */
  additionalRoots?: string[];
  /** Include home directory as allowed root (default: true for local tools) */
  includeHomeDir?: boolean;
}

/**
 * PathValidator class for validating and sanitizing file system paths
 */
export class PathValidator {
  private allowedRoots: string[];

  constructor(options?: PathValidatorOptions) {
    const opts = options || {};

    const root = this.expandTilde(resolveWorkspaceRoot(opts.workspaceRoot));

    this.allowedRoots = [path.resolve(root)];

    // Add home directory by default (can be disabled with includeHomeDir: false)
    if (opts.includeHomeDir !== false) {
      const homeDir = os.homedir();
      if (homeDir && !this.allowedRoots.includes(homeDir)) {
        this.allowedRoots.push(homeDir);
      }
    }

    // Add additional roots from options
    if (opts.additionalRoots) {
      for (const additionalRoot of opts.additionalRoots) {
        this.addAllowedRoot(additionalRoot);
      }
    }

    // Add roots from ALLOWED_PATHS environment variable (comma-separated)
    const envPaths = process.env.ALLOWED_PATHS;
    if (envPaths) {
      const paths = envPaths
        .split(',')
        .map(p => p.trim())
        .filter(p => p.length > 0);
      for (const envPath of paths) {
        this.addAllowedRoot(envPath);
      }
    }

    // Add user-registered additional roots from SecurityRegistry
    for (const root of securityRegistry.extraAllowedRoots) {
      this.addAllowedRoot(root);
    }
  }

  /**
   * Expands ~ to home directory
   */
  private expandTilde(inputPath: string): string {
    if (inputPath.startsWith('~')) {
      return path.join(os.homedir(), inputPath.slice(1));
    }
    return inputPath;
  }

  /**
   * Adds an allowed root directory.
   *
   * @example
   * ```ts
   * validator.addAllowedRoot('/tmp/builds');
   * validator.validate('/tmp/builds/output.js');
   * // → { isValid: true, sanitizedPath: '/tmp/builds/output.js' }
   * ```
   */
  addAllowedRoot(root: string): void {
    const expandedRoot = this.expandTilde(root);
    const resolvedRoot = path.resolve(expandedRoot);
    if (!this.allowedRoots.includes(resolvedRoot)) {
      this.allowedRoots.push(resolvedRoot);
    }
  }

  /**
   * Validates a path to ensure it's within allowed directories.
   *
   * SECURITY NOTE: This method ALWAYS resolves symlinks to their real paths
   * before validation. This prevents symlink-based path traversal attacks.
   * This behavior cannot be disabled as it's a core security requirement.
   *
   * @param inputPath - The path to validate
   * @example
   * ```ts
   * const v = new PathValidator({ workspaceRoot: '/app' });
   * v.validate('/app/src/index.ts');
   * // → { isValid: true, sanitizedPath: '/app/src/index.ts' }
   * v.validate('../../etc/passwd');
   * // → { isValid: false, error: "Path '../../etc/passwd' is outside allowed directories" }
   * ```
   */
  validate(inputPath: string): PathValidationResult {
    if (!inputPath || inputPath.trim() === '') {
      return {
        isValid: false,
        error: 'Path cannot be empty',
      };
    }

    const expandedPath = this.expandTilde(inputPath);
    const absolutePath = path.resolve(expandedPath);

    const isAllowed = this.allowedRoots.some(root => {
      if (absolutePath === root) {
        return true;
      }
      return absolutePath.startsWith(root + path.sep);
    });

    if (!isAllowed) {
      return {
        isValid: false,
        error: `Path '${redactPath(inputPath)}' is outside allowed directories`,
      };
    }

    if (shouldIgnore(absolutePath)) {
      return {
        isValid: false,
        error: `Path '${redactPath(inputPath)}' is in an ignored directory or matches an ignored pattern`,
      };
    }

    try {
      const realPath = fs.realpathSync(absolutePath);
      const isRealPathAllowed = this.allowedRoots.some(root => {
        return realPath === root || realPath.startsWith(root + path.sep);
      });

      if (!isRealPathAllowed) {
        return {
          isValid: false,
          error: `Symlink target '${redactPath(realPath)}' is outside allowed directories`,
        };
      }

      if (shouldIgnore(realPath)) {
        return {
          isValid: false,
          error: `Symlink target '${redactPath(realPath)}' is in an ignored directory or matches an ignored pattern`,
        };
      }

      return {
        isValid: true,
        sanitizedPath: realPath,
      };
    } catch (error) {
      if (error instanceof Error) {
        const nodeError = error as Error & { code?: string };

        if (nodeError.code === 'ENOENT') {
          return {
            isValid: true,
            sanitizedPath: absolutePath,
          };
        }

        if (nodeError.code === 'EACCES') {
          return {
            isValid: false,
            error: `Permission denied accessing path: ${redactPath(inputPath)}`,
          };
        }

        if (nodeError.code === 'ELOOP') {
          return {
            isValid: false,
            error: `Symlink loop detected at path: ${redactPath(inputPath)}`,
          };
        }

        if (nodeError.code === 'ENAMETOOLONG') {
          return {
            isValid: false,
            error: `Path name too long: ${redactPath(inputPath)}`,
          };
        }
      }

      return {
        isValid: false,
        error: `Unexpected error validating path: ${redactPath(inputPath)}`,
      };
    }
  }

  /**
   * Checks if a path exists and is accessible.
   *
   * @example
   * ```ts
   * await validator.exists('/app/src/index.ts'); // true
   * await validator.exists('/etc/shadow');         // false (outside root)
   * ```
   */
  async exists(inputPath: string): Promise<boolean> {
    const validation = this.validate(inputPath);
    if (!validation.isValid || !validation.sanitizedPath) {
      return false;
    }

    try {
      await fs.promises.access(validation.sanitizedPath, fs.constants.R_OK);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Gets the type of a path (file, directory, symlink).
   *
   * @example
   * ```ts
   * await validator.getType('/app/src');          // 'directory'
   * await validator.getType('/app/src/index.ts'); // 'file'
   * await validator.getType('/etc/passwd');        // null (outside root)
   * ```
   */
  async getType(
    inputPath: string
  ): Promise<'file' | 'directory' | 'symlink' | null> {
    const validation = this.validate(inputPath);
    if (!validation.isValid || !validation.sanitizedPath) {
      return null;
    }

    try {
      const stats = await fs.promises.lstat(validation.sanitizedPath);
      if (stats.isFile()) return 'file';
      if (stats.isDirectory()) return 'directory';
      if (stats.isSymbolicLink()) return 'symlink';
      return null;
    } catch {
      return null;
    }
  }

  /**
   * Gets the list of currently allowed root directories (for debugging)
   */
  getAllowedRoots(): readonly string[] {
    return [...this.allowedRoots];
  }

  /** @internal Replace all allowed roots. Used by reinitializePathValidator. */
  replaceAllowedRoots(roots: readonly string[]): void {
    this.allowedRoots = [...roots];
  }
}

/**
 * Global path validator instance.
 * Includes home directory by default for convenient local tool access.
 */
export const pathValidator = new PathValidator();

/**
 * Reinitialize the global path validator with custom options.
 * Useful for testing or runtime reconfiguration.
 */
export function reinitializePathValidator(
  options?: PathValidatorOptions
): PathValidator {
  const newValidator = new PathValidator(options);
  pathValidator.replaceAllowedRoots(newValidator.getAllowedRoots());
  return pathValidator;
}
