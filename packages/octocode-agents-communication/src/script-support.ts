import { createHash } from 'node:crypto';
import { readFileSync, renameSync, writeFileSync } from 'node:fs';

/** A parsed JSON object whose fields are not yet checked. */
export type Reply = Record<string, unknown>;

/** Hex SHA-256 of a string or buffer. */
export const sha256 = (data: string | Uint8Array): string => createHash('sha256').update(data).digest('hex');

/** True for a JSON object (not null, not an array). */
export const isReply = (value: unknown): value is Reply => typeof value === 'object' && value !== null && !Array.isArray(value);

/** The message of a caught value, which JavaScript lets be anything. */
export const errorMessage = (error: unknown): string => error instanceof Error ? error.message : String(error);

/** Writes a generated file only when its text changes, through a temporary file and a rename, so a reader never sees it half written. */
export function writeFileIfChanged(path: string, text: string): void {
  try { if (readFileSync(path, 'utf8') === text) return; } catch { /* a missing file is written */ }
  const temp = `${path}.${process.pid}.tmp`;
  writeFileSync(temp, text);
  renameSync(temp, path);
}
