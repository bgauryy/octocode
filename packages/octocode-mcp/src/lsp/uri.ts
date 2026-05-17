/**
 * URI conversion utilities for LSP
 * Handles file path <-> file:// URI conversion
 * @module lsp/uri
 */

import * as path from 'path';
import { URI } from 'vscode-uri';

/**
 * Convert a file path to a file:// URI using proper encoding.
 * Handles Windows paths, UNC paths, and special characters correctly.
 *
 * @param filePath - Absolute or relative file path
 * @returns Properly encoded file:// URI
 *
 * @example
 * toUri('/users/me/file.ts')           // 'file:///users/me/file.ts'
 * toUri('C:\\Users\\me\\file.ts')      // 'file:///c%3A/Users/me/file.ts'
 * toUri('/path/with spaces/file#1.ts') // 'file:///path/with%20spaces/file%231.ts'
 */
export function toUri(filePath: string): string {
  // Already a URI - return as-is
  if (filePath.startsWith('file://')) {
    return filePath;
  }

  // Resolve to absolute path and convert to URI
  const absolutePath = path.resolve(filePath);
  return URI.file(absolutePath).toString();
}

/**
 * Convert a file:// URI back to a filesystem path.
 * Returns platform-specific path (forward slashes on Unix, backslashes on Windows).
 *
 * @param uri - A file:// URI string
 * @returns Platform-specific filesystem path
 *
 * @example
 * fromUri('file:///users/me/file.ts')           // '/users/me/file.ts'
 * fromUri('file:///c%3A/Users/me/file.ts')      // 'C:\Users\me\file.ts' (Windows)
 * fromUri('file:///path/with%20spaces/file.ts') // '/path/with spaces/file.ts'
 */
export function fromUri(uri: string): string {
  // Not a file URI - return as-is
  if (!uri.startsWith('file://')) {
    return uri;
  }

  // Parse and return filesystem path
  return URI.parse(uri).fsPath;
}

/**
 * Result of a safe URI parse — never throws on bad input by default.
 */
export interface SafeUriResult {
  isValid: boolean;
  /** Filesystem path; only meaningful when isValid=true. */
  path: string | null;
  /** Human-readable reason for rejection. */
  error?: string;
}

/** Thrown by `fromUriSafe(uri, { throwOnInvalid: true })`. */
export class UnsafeUriError extends Error {
  constructor(uri: string, reason: string) {
    super(`Unsafe URI rejected (${reason}): ${uri}`);
    this.name = 'UnsafeUriError';
  }
}

/**
 * Defensive variant of {@link fromUri}. Rejects non-`file:` schemes,
 * bare paths, embedded null bytes, and anything `vscode-uri` can't
 * parse. Used to harden the boundary between untrusted LSP server
 * output and our filesystem operations (T1.5 — fromUri hardening).
 *
 * @example
 *   const r = fromUriSafe(serverProvidedUri);
 *   if (!r.isValid) return rejectWithHint(r.error);
 *   const filePath = r.path!;
 */
export function fromUriSafe(
  uri: string,
  options: { throwOnInvalid?: boolean } = {}
): SafeUriResult {
  const reject = (reason: string): SafeUriResult => {
    if (options.throwOnInvalid) {
      throw new UnsafeUriError(uri, reason);
    }
    return { isValid: false, path: null, error: reason };
  };

  if (typeof uri !== 'string' || uri.length === 0) {
    return reject('uri must be a non-empty string');
  }

  if (uri.includes('\u0000')) {
    return reject('uri contains null byte');
  }

  // Bare path (no scheme) — caller probably had a bug; refuse so we don't
  // accidentally swallow attacker-controlled bytes from the LSP wire.
  if (!/^[a-zA-Z][a-zA-Z0-9+\-.]*:/.test(uri)) {
    return reject('uri is missing a scheme');
  }

  if (!uri.startsWith('file://')) {
    return reject('unsupported scheme (only file:// is allowed)');
  }

  try {
    const parsed = URI.parse(uri);
    if (parsed.scheme !== 'file') {
      return reject('parsed scheme is not file');
    }
    const fsPath = parsed.fsPath;
    if (!fsPath || fsPath.includes('\u0000')) {
      return reject('parsed fsPath is empty or contains null byte');
    }
    return { isValid: true, path: fsPath };
  } catch (err) {
    return reject(
      err instanceof Error ? `parse failed: ${err.message}` : 'parse failed'
    );
  }
}
