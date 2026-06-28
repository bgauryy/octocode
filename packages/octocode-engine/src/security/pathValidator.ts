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

    if (opts.includeHomeDir !== false) {
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
    absolutePath: string,
    resolvedPath: string
  ): boolean {
    return this.allowedRoots.some(root => {
      if (resolvedPath === root || resolvedPath.startsWith(root + path.sep)) {
        return true;
      }

      try {
        const realRoot = fs.realpathSync(root);
        if (absolutePath === root) {
          return resolvedPath === realRoot;
        }
        return resolvedPath.startsWith(realRoot + path.sep);
      } catch {
        return false;
      }
    });
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

export const pathValidator = new PathValidator();

export function resetPathValidator(
  options?: PathValidatorOptions
): PathValidator {
  const newValidator = new PathValidator(options);
  pathValidator.replaceAllowedRoots(newValidator.getAllowedRoots());
  return pathValidator;
}
