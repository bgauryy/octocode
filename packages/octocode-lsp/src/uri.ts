import { nativeBinding } from './native.js';

export class UnsafeUriError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'UnsafeUriError';
  }
}

export type SafeUriResult =
  | { isValid: true; path: string }
  | { isValid: false; error: string };

export function toUri(filePath: string): string {
  return nativeBinding.toUri(filePath);
}

export function fromUri(uri: string): string {
  return nativeBinding.fromUri(uri);
}

export function fromUriSafe(
  uri: string,
  options?: { throwOnInvalid?: boolean }
): SafeUriResult {
  try {
    return { isValid: true, path: nativeBinding.fromUri(uri) };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (options?.throwOnInvalid) throw new UnsafeUriError(message);
    return { isValid: false, error: message };
  }
}
