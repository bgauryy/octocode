import { createHash } from 'node:crypto';
import { readFileSync, renameSync, writeFileSync } from 'node:fs';

/** A parsed JSON object whose fields are not yet checked. */
export type Reply = Record<string, unknown>;

/** Hex SHA-256 of a string or buffer. */
export const sha256 = (data: string | Uint8Array): string => createHash('sha256').update(data).digest('hex');

/** The CLI flags that bind a call to one workspace, database and (optionally) session. */
export const bindingFlags = (workspace: string, database: string, session?: string): string[] => ['--workspace', workspace, '--database', database, ...(session ? ['--session', session] : [])];

/** True for a JSON object (not null, not an array). */
export const isReply = (value: unknown): value is Reply => typeof value === 'object' && value !== null && !Array.isArray(value);

/** Parses JSON text that must hold an object, such as one CLI reply or MCP frame. */
export function parseReply(text: string): Reply {
  const value: unknown = JSON.parse(text);
  if (!isReply(value)) throw new Error(`Expected a JSON object, received ${text.slice(0, 200)}`);
  return value;
}

/** The message of a caught value, which JavaScript lets be anything. */
export const errorMessage = (error: unknown): string => error instanceof Error ? error.message : String(error);

/** The stack of a caught value, or its text when it is not an Error. */
export const errorStack = (error: unknown): string => error instanceof Error ? error.stack ?? error.message : String(error);

/** Field readers that fail with the field name instead of letting a malformed reply flow on as `undefined`. */
export function text(value: unknown, label: string): string {
  if (typeof value !== 'string') throw new Error(`${label} must be a string, received ${JSON.stringify(value)}`);
  return value;
}

export function integer(value: unknown, label: string): number {
  if (typeof value !== 'number' || !Number.isInteger(value)) throw new Error(`${label} must be an integer, received ${JSON.stringify(value)}`);
  return value;
}

export function object(value: unknown, label: string): Reply {
  if (!isReply(value)) throw new Error(`${label} must be an object, received ${JSON.stringify(value)}`);
  return value;
}

export function list(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`${label} must be an array, received ${JSON.stringify(value)}`);
  return value;
}

/** Writes a generated file only when its text changes, through a temporary file and a rename, so a reader never sees it half written. */
export function writeFileIfChanged(path: string, text: string): void {
  try { if (readFileSync(path, 'utf8') === text) return; } catch { /* a missing file is written */ }
  const temp = `${path}.${process.pid}.tmp`;
  writeFileSync(temp, text);
  renameSync(temp, path);
}
