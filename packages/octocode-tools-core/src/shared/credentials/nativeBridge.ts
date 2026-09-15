import { createRequire } from 'node:module';
import type { StoredCredentials } from './types.js';

type NativeRuntime = {
  storeCredentials(value: StoredCredentials): { success: boolean };
  getCredentials(hostname?: string): StoredCredentials | null;
  deleteCredentials(hostname?: string): { success: boolean };
  refreshAuthToken(hostname?: string): Promise<{
    success: boolean;
    username?: string;
    hostname?: string;
    error?: string;
  }>;
  getTokenWithRefresh(hostname?: string): Promise<{
    token: string | null;
    source: string;
    username?: string;
    refreshError?: string;
  }>;
};

let cached: NativeRuntime | null | undefined;

export function nativeCredentials(): NativeRuntime | null {
  if (cached !== undefined) return cached;
  const bindingPath = process.env.OCTOCODE_NATIVE_BINDING;
  if (!bindingPath) {
    cached = null;
    return null;
  }
  try {
    const require = createRequire(import.meta.url);
    const binding = require(bindingPath) as {
      NativeRuntime?: new () => NativeRuntime;
    };
    if (typeof binding.NativeRuntime !== 'function') {
      cached = null;
      return null;
    }
    cached = new binding.NativeRuntime();
    return cached;
  } catch {
    cached = null;
    return null;
  }
}

export function _resetNativeCredentialsForTests(): void {
  cached = undefined;
}
