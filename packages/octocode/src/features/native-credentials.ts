import { createRequire } from 'node:module';
import { join } from 'node:path';
import { getOctocodeHome } from '@octocodeai/config';
import type { StoredCredentials } from '../types/index.js';

interface NativeCredentialRuntime {
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
}

let runtime: NativeCredentialRuntime | undefined;

function credentialsRuntime(): NativeCredentialRuntime {
  if (runtime) return runtime;
  const require = createRequire(import.meta.url);
  const binding = require('@octocodeai/octocode-native/native.cjs') as {
    NativeRuntime?: new (options?: Record<string, unknown>) => NativeCredentialRuntime;
  };
  if (typeof binding.NativeRuntime !== 'function') {
    throw new Error('The native Octocode credential runtime is unavailable.');
  }
  runtime = new binding.NativeRuntime({ surface: 'cli' });
  return runtime;
}

export async function storeCredentials(
  credentials: StoredCredentials
): Promise<{ success: boolean }> {
  return credentialsRuntime().storeCredentials(credentials);
}

export async function getCredentials(
  hostname = 'github.com'
): Promise<StoredCredentials | null> {
  return credentialsRuntime().getCredentials(hostname);
}

export function getCredentialsSync(
  hostname = 'github.com'
): StoredCredentials | null {
  return credentialsRuntime().getCredentials(hostname);
}

export async function deleteCredentials(
  hostname = 'github.com'
): Promise<{ success: boolean }> {
  return credentialsRuntime().deleteCredentials(hostname);
}

export async function refreshAuthToken(hostname = 'github.com') {
  return credentialsRuntime().refreshAuthToken(hostname);
}

export async function getTokenWithRefresh(hostname = 'github.com') {
  return credentialsRuntime().getTokenWithRefresh(hostname);
}

export function isTokenExpired(credentials: StoredCredentials): boolean {
  const expiresAt = credentials.token.expiresAt;
  return expiresAt ? Date.parse(expiresAt) <= Date.now() : false;
}

export function getCredentialsFilePath(): string {
  return join(getOctocodeHome(process.env), 'credentials.json');
}
