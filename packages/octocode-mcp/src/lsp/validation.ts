/**
 * Security validation for LSP server binary paths
 * Prevents path traversal and validates binary existence
 * @module lsp/validation
 */

import { realpathSync, statSync } from 'fs';
import { readFile } from 'fs/promises';
import * as path from 'path';
import { pathValidator } from 'octocode-security-utils/pathValidator';

/**
 * Result of LSP server path validation
 */
interface ValidationResult {
  /** Whether the path is valid and safe to execute */
  isValid: boolean;
  /** Resolved absolute path (if valid) */
  resolvedPath?: string;
  /** Error message (if invalid) */
  error?: string;
}

/**
 * Validates that an LSP server binary path is safe to execute.
 *
 * Security checks:
 * 1. Path must exist
 * 2. Path must be a regular file (not a directory or dangling symlink)
 * 3. Path traversal patterns are blocked
 * 4. Symlinks are resolved and their targets validated
 *
 * @param binPath - The resolved binary path to validate
 * @param baseDir - The base directory the path should be relative to
 * @returns ValidationResult with isValid flag and error message if invalid
 *
 * @example
 * const result = validateLSPServerPath('./bin/server', '/usr/local/lib');
 * if (!result.isValid) {
 *   console.error(result.error);
 * }
 */
export function validateLSPServerPath(
  binPath: string,
  baseDir: string
): ValidationResult {
  // Resolve to absolute path
  const absolutePath = path.isAbsolute(binPath)
    ? binPath
    : path.resolve(baseDir, binPath);

  // Check for path traversal attempt (relative path escaping base directory)
  if (!path.isAbsolute(binPath)) {
    // Normalize and check if it stays within baseDir
    const normalizedPath = path.normalize(absolutePath);
    if (!normalizedPath.startsWith(baseDir)) {
      return {
        isValid: false,
        error: 'LSP server path escapes base directory',
      };
    }
  }

  // Resolve symlinks to get real path
  let realPath: string;
  try {
    realPath = realpathSync(absolutePath);
  } catch (error) {
    const nodeError = error as Error & { code?: string };
    if (nodeError.code === 'ENOENT') {
      return {
        isValid: false,
        error: 'LSP server binary not found',
      };
    }
    if (nodeError.code === 'ELOOP') {
      return {
        isValid: false,
        error: 'Symlink loop detected in LSP server path',
      };
    }
    return {
      isValid: false,
      error: 'Cannot resolve LSP server path',
    };
  }

  // Verify it's a file (not a directory)
  try {
    const stats = statSync(realPath);
    if (!stats.isFile()) {
      return {
        isValid: false,
        error: 'LSP server path is not a file',
      };
    }
  } catch {
    return {
      isValid: false,
      error: 'Cannot stat LSP server binary',
    };
  }

  return { isValid: true, resolvedPath: realPath };
}

/**
 * Safely read a file after validating its path is within allowed roots.
 * Used for LSP-returned file paths (definition locations, references, etc.)
 * that need defense-in-depth validation before reading.
 *
 * @param filePath - Absolute path to read
 * @returns File content as string, or null if path is outside allowed roots or unreadable
 */
export async function safeReadFile(filePath: string): Promise<string | null> {
  const validation = pathValidator.validate(filePath);
  if (!validation.isValid) return null;
  try {
    return await readFile(filePath, 'utf-8');
  } catch {
    return null;
  }
}
