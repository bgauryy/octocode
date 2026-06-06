import { realpathSync, statSync } from 'fs';
import { readFile } from 'fs/promises';
import * as path from 'path';
import { pathValidator } from 'octocode-security-utils/pathValidator';

interface ValidationResult {
  isValid: boolean;

  resolvedPath?: string;

  error?: string;
}

export function validateLSPServerPath(
  binPath: string,
  baseDir: string
): ValidationResult {
  const absolutePath = path.isAbsolute(binPath)
    ? binPath
    : path.resolve(baseDir, binPath);

  if (!path.isAbsolute(binPath)) {
    const normalizedPath = path.normalize(absolutePath);
    if (!normalizedPath.startsWith(baseDir)) {
      return {
        isValid: false,
        error: 'LSP server path escapes base directory',
      };
    }
  }

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

export async function safeReadFile(filePath: string): Promise<string | null> {
  const validation = pathValidator.validate(filePath);
  if (!validation.isValid) return null;
  try {
    return await readFile(filePath, 'utf-8');
  } catch {
    return null;
  }
}
