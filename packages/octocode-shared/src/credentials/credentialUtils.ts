import type { StoredCredentials } from './types.js';

export function normalizeHostname(hostname: string): string {
  return hostname
    .toLowerCase()
    .replace(/^https?:\/\//, '')
    .replace(/\/$/, '');
}

export function isTokenExpired(credentials: StoredCredentials): boolean {
  if (!credentials.token.expiresAt) {
    return false;
  }

  const expiresAt = new Date(credentials.token.expiresAt);

  if (isNaN(expiresAt.getTime())) {
    return true;
  }

  const now = new Date();

  return expiresAt.getTime() - now.getTime() < 5 * 60 * 1000;
}

export function isRefreshTokenExpired(credentials: StoredCredentials): boolean {
  if (!credentials.token.refreshTokenExpiresAt) {
    return false;
  }

  const expiresAt = new Date(credentials.token.refreshTokenExpiresAt);

  if (isNaN(expiresAt.getTime())) {
    return true;
  }

  return new Date() >= expiresAt;
}
