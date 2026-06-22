import {
  existsSync,
  readFileSync,
  writeFileSync,
  unlinkSync,
  statSync,
  chmodSync,
} from 'node:fs';
import { createCipheriv, createDecipheriv, randomBytes } from 'node:crypto';
import { ensureHome, paths } from '../paths.js';
import { CredentialsStoreSchema } from './schemas.js';
import type { CredentialsStore } from './types.js';

export const OCTOCODE_DIR = paths.home;
export const CREDENTIALS_FILE = paths.credentials;
export const KEY_FILE = paths.key;

const ALGORITHM = 'aes-256-gcm';
const IV_LENGTH = 16;

export function ensureOctocodeDir(): void {
  ensureHome();
}

function getOrCreateKey(): Buffer {
  ensureOctocodeDir();

  if (existsSync(KEY_FILE)) {
    const mode = statSync(KEY_FILE).mode & 0o777;
    if (mode & 0o077) {
      chmodSync(KEY_FILE, 0o600);
    }
    return Buffer.from(readFileSync(KEY_FILE, 'utf8'), 'hex');
  }

  const key = randomBytes(32);
  writeFileSync(KEY_FILE, key.toString('hex'), { mode: 0o600 });
  return key;
}

export function encrypt(data: string): string {
  const key = getOrCreateKey();
  const iv = randomBytes(IV_LENGTH);
  const cipher = createCipheriv(ALGORITHM, key, iv);

  let encrypted = cipher.update(data, 'utf8', 'hex');
  encrypted += cipher.final('hex');

  const authTag = cipher.getAuthTag();

  return `${iv.toString('hex')}:${authTag.toString('hex')}:${encrypted}`;
}

export function decrypt(encryptedData: string): string {
  const key = getOrCreateKey();
  const [ivHex, authTagHex, encrypted] = encryptedData.split(':');

  if (!ivHex || !authTagHex || !encrypted) {
    throw new Error('Invalid encrypted data format');
  }

  const iv = Buffer.from(ivHex, 'hex');
  const authTag = Buffer.from(authTagHex, 'hex');
  const decipher = createDecipheriv(ALGORITHM, key, iv);
  decipher.setAuthTag(authTag);

  let decrypted = decipher.update(encrypted, 'hex', 'utf8');
  decrypted += decipher.final('utf8');

  return decrypted;
}

export function readCredentialsStore(): CredentialsStore {
  ensureOctocodeDir();

  if (!existsSync(CREDENTIALS_FILE)) {
    return { version: 1, credentials: {} };
  }

  try {
    const encryptedContent = readFileSync(CREDENTIALS_FILE, 'utf8');
    const decrypted = decrypt(encryptedContent);
    const parsed = JSON.parse(decrypted);
    const result = CredentialsStoreSchema.safeParse(parsed);
    if (!result.success) {
      return { version: 1, credentials: {} };
    }
    return result.data;
  } catch {
    return { version: 1, credentials: {} };
  }
}

export function writeCredentialsStore(store: CredentialsStore): void {
  ensureOctocodeDir();

  const encrypted = encrypt(JSON.stringify(store, null, 2));
  writeFileSync(CREDENTIALS_FILE, encrypted, { mode: 0o600 });
}

export function cleanupKeyFile(): void {
  try {
    if (existsSync(CREDENTIALS_FILE)) {
      unlinkSync(CREDENTIALS_FILE);
    }
    if (existsSync(KEY_FILE)) {
      unlinkSync(KEY_FILE);
    }
  } catch {
    void 0;
  }
}
