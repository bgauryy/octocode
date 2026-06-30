import path from 'path';
import fs from 'fs';
import os from 'os';
import type { PathValidationResult } from './types.js';
import { shouldIgnore } from './ignoredPathFilter.js';
import { redactPath } from './pathUtils.js';
import { securityRegistry } from './registry.js';

interface PathValidatorOptions {
  workspaceRoot?: string;

  additionalRoots?: string[];

  includeHomeDir?: boolean;
}

export class PathValidator {
  private allowedRoots: string[];

  constructor(options?: PathValidatorOptions) {
    const opts = options || {};

    this.allowedRoots = [];

    if (opts.workspaceRoot) {
      this.addAllowedRoot(opts.workspaceRoot);
    }

    // Home dir is opt-in. Local tools intentionally include it (users search
    // ~/projects, ~/Documents, etc.) and rely on ignoredPathFilter as the
    // second layer to block .ssh, .aws, .kube, etc. within it.
    if (opts.includeHomeDir === true) {
      const homeDir = os.homedir();
      if (homeDir && !this.allowedRoots.includes(homeDir)) {
        this.allowedRoots.push(homeDir);
      }
    }

    if (opts.additionalRoots) {
      for (const additionalRoot of opts.additionalRoots) {
        this.addAllowedRoot(additionalRoot);
      }
    }

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

    for (const root of securityRegistry.extraAllowedRoots) {
      this.addAllowedRoot(root);
    }
  }

  private expandTilde(inputPath: string): string {
    if (inputPath.startsWith('~')) {
      return path.join(os.homedir(), inputPath.slice(1));
    }
    return inputPath;
  }

  addAllowedRoot(root: string): void {
    const expandedRoot = this.expandTilde(root);
    const resolvedRoot = path.resolve(expandedRoot);
    if (!this.allowedRoots.includes(resolvedRoot)) {
      this.allowedRoots.push(resolvedRoot);
    }
    try {
      const realRoot = fs.realpathSync(resolvedRoot);
      if (!this.allowedRoots.includes(realRoot)) {
        this.allowedRoots.push(realRoot);
      }
    } catch {
      // Non-existent roots are still useful for validating future output paths.
    }
  }

  private isResolvedPathAllowed(
    _absolutePath: string,
    resolvedPath: string
  ): boolean {
    // addAllowedRoot already resolves both path.resolve() and realpathSync()
    // and pushes both into allowedRoots, so a plain string comparison is
    // sufficient here — no need for another realpathSync per validate() call.
    return this.allowedRoots.some(
      root => resolvedPath === root || resolvedPath.startsWith(root + path.sep)
    );
  }

  validate(inputPath: string): PathValidationResult {
    if (!inputPath || inputPath.trim() === '') {
      return {
        isValid: false,
        error: 'Path cannot be empty',
      };
    }

    const expandedPath = this.expandTilde(inputPath);
    const absolutePath = path.resolve(expandedPath);

    try {
      const realPath = fs.realpathSync(absolutePath);
      const isRealPathAllowed = this.isResolvedPathAllowed(
        absolutePath,
        realPath
      );

      if (!isRealPathAllowed) {
        return {
          isValid: false,
          error: `Symlink target '${redactPath(realPath)}' is outside allowed directories`,
        };
      }

      if (shouldIgnore(absolutePath)) {
        return {
          isValid: false,
          error: `Path '${redactPath(inputPath)}' is in an ignored directory or matches an ignored pattern`,
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
          return this.validateNonExistentPath(absolutePath, inputPath);
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

  private validateNonExistentPath(
    absolutePath: string,
    inputPath: string
  ): PathValidationResult {
    const { root } = path.parse(absolutePath);
    let ancestor = path.dirname(absolutePath);
    const remainder: string[] = [path.basename(absolutePath)];
    while (ancestor !== root && !fs.existsSync(ancestor)) {
      remainder.unshift(path.basename(ancestor));
      ancestor = path.dirname(ancestor);
    }

    let resolvedAncestor: string;
    try {
      resolvedAncestor = fs.realpathSync(ancestor);
    } catch {
      return {
        isValid: false,
        error: `Unexpected error validating path: ${redactPath(inputPath)}`,
      };
    }

    const resolvedPath = path.join(resolvedAncestor, ...remainder);
    const isAllowed = this.isResolvedPathAllowed(absolutePath, resolvedPath);

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

    if (shouldIgnore(resolvedPath)) {
      return {
        isValid: false,
        error: `Path '${redactPath(inputPath)}' resolves into an ignored directory or matches an ignored pattern`,
      };
    }

    return {
      isValid: true,
      sanitizedPath: resolvedPath,
    };
  }

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

  getAllowedRoots(): readonly string[] {
    return [...this.allowedRoots];
  }

  replaceAllowedRoots(roots: readonly string[]): void {
    this.allowedRoots = [...roots];
  }
}

// The production singleton explicitly opts in to the home dir because local
// tools are expected to search ~/projects, ~/Documents, etc. Security within
// the home directory is provided by ignoredPathFilter (.ssh, .aws, .kube, etc.).
export const pathValidator = new PathValidator({ includeHomeDir: true });

export function resetPathValidator(
  options?: PathValidatorOptions
): PathValidator {
  // When called with no arguments (e.g. in afterEach tear-down), restore the
  // same includeHomeDir state the singleton starts with.
  const effective: PathValidatorOptions = options ?? { includeHomeDir: true };
  const newValidator = new PathValidator(effective);
  pathValidator.replaceAllowedRoots(newValidator.getAllowedRoots());
  return pathValidator;
}
