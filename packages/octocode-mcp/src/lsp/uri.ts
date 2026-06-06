import * as path from 'path';
import { URI } from 'vscode-uri';

export function toUri(filePath: string): string {
  if (filePath.startsWith('file://')) {
    return filePath;
  }

  const absolutePath = path.resolve(filePath);
  return URI.file(absolutePath).toString();
}

export function fromUri(uri: string): string {
  if (!uri.startsWith('file://')) {
    return uri;
  }

  return URI.parse(uri).fsPath;
}

interface SafeUriResult {
  isValid: boolean;

  path: string | null;

  error?: string;
}

export class UnsafeUriError extends Error {
  constructor(uri: string, reason: string) {
    super(`Unsafe URI rejected (${reason}): ${uri}`);
    this.name = 'UnsafeUriError';
  }
}

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
