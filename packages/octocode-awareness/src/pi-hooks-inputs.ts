import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { realpathSync } from 'node:fs';
import type { DatabaseSync } from 'node:sqlite';
import { connectCachedDb, resolveDbPath } from './db.js';
import { normalizeArtifact } from './helpers.js';
import { ensureRunSession } from './sessions.js';

// HOOK-2: A one-time session startup token that survives process.pid reuse across
// OS restarts. We combine the session file name (if available) with a UUID suffix
// generated once at import time so the agentId is stable within a session but
// unique across sessions even when PIDs repeat.
export const _sessionStartupToken = randomUUID().slice(0, 8);

export interface PiLikeSessionManager {
  getSessionFile?: () => string | null | undefined;
}

export interface PiLikeUi {
  notify?: (message: string, level?: string) => void;
}

export interface PiLikeContext {
  cwd?: string;
  dbPath?: string;
  artifact?: string;
  sessionManager?: PiLikeSessionManager;
  ui?: PiLikeUi;
}

export interface PiLikeApi {
  on?: (eventName: string, handler: (event: Record<string, unknown>, ctx: PiLikeContext) => unknown | Promise<unknown>) => void;
  sendMessage?: (message: Record<string, unknown>, options?: Record<string, unknown>) => void;
}

export interface PiToolEvent {
  toolName?: string;
  toolCallId?: string;
  input?: unknown;
  isError?: boolean;
}

export interface PiAwarenessBridgeOptions {
  pendingToolFiles?: Map<string, string[]>;
  pendingToolRuns?: Map<string, string>;
  peerFingerprints?: Map<string, string>;
  dbPath?: string | null;
  getDb?: (ctx?: PiLikeContext) => DatabaseSync;
  skillRoot?: string | null;
}

export function addPathValue(paths: string[], value: unknown): void {
  if (typeof value === 'string' && value.trim().length > 0) {
    paths.push(value.trim());
  } else if (Array.isArray(value)) {
    for (const item of value) addPathValue(paths, item);
  }
}

export function addApplyPatchPaths(paths: string[], command: unknown): void {
  if (typeof command !== 'string') return;
  for (const line of command.split('\n')) {
    const addUpdDel = line.match(/^\*\*\* (?:Add|Update|Delete) File: (.+)$/);
    if (addUpdDel) {
      paths.push(addUpdDel[1]!.trim());
      continue;
    }
    const moveTo = line.match(/^\*\*\* Move to: (.+)$/);
    if (moveTo) paths.push(moveTo[1]!.trim());
  }
}

export function objectOrEmpty(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {};
}

export function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return null;
}

export function addQueryPaths(paths: string[], value: unknown): void {
  if (!Array.isArray(value)) return;
  for (const query of value) {
    const payload = objectOrEmpty(query);
    addPathValue(paths, payload.path);
    addPathValue(paths, payload.filePath);
    addPathValue(paths, payload.file_path);
    addPathValue(paths, payload.paths);
    addPathValue(paths, payload.filePaths);
    addPathValue(paths, payload.file_paths);
  }
}

export function extractPiWriteTargetPaths(
  toolName: unknown,
  input: unknown = {},
  options: { assumeWrite?: boolean } = {},
): string[] {
  const normalizedToolName = String(toolName ?? '').toLowerCase();
  const isWriteTool = Boolean(options.assumeWrite) || [
    'write',
    'edit',
    'multi_edit',
    'multiedit',
    'notebookedit',
    'notebook_edit',
    'apply_patch',
    'applypatch',
  ].includes(normalizedToolName);
  const payload = objectOrEmpty(input);
  // Source for apply_patch marker scanning (addApplyPatchPaths). Only true patch
  // carriers — a raw string input, or `command`/`patch` fields — are scanned.
  // `text`/`content` are the FILE BODY for Write/Edit; scanning them would turn
  // any file line like `*** Add File: X` (e.g. these very docs) into a phantom
  // lock + edit_log target. Write/Edit paths come from the explicit path fields
  // below, not from the body.
  const command = typeof input === 'string'
    ? input
    : firstString(payload.command, payload.patch);

  if (!isWriteTool) {
    const patchPaths: string[] = [];
    addApplyPatchPaths(patchPaths, command);
    return [...new Set(patchPaths)];
  }

  const paths: string[] = [];
  addPathValue(paths, payload.path);
  addPathValue(paths, payload.filePath);
  addPathValue(paths, payload.file_path);
  addPathValue(paths, payload.paths);
  addPathValue(paths, payload.filePaths);
  addPathValue(paths, payload.file_paths);
  addQueryPaths(paths, payload.queries);
  addApplyPatchPaths(paths, command);

  return [...new Set(paths)];
}

export function artifactFrom(ctx?: PiLikeContext, event?: Record<string, unknown>): string | null {
  const input = objectOrEmpty(event?.input);
  return normalizeArtifact(firstString(
    process.env.OCTOCODE_ARTIFACT,
    process.env.OCTOCODE_PACKAGE,
    process.env.OCTOCODE_SERVICE,
    ctx?.artifact,
    event?.artifact,
    event?.package,
    event?.service,
    input.artifact,
    input.package,
    input.service,
  ));
}

export function getPiAwarenessSessionId(ctx?: PiLikeContext): string {
  const sessionFile = ctx?.sessionManager?.getSessionFile?.();
  if (sessionFile) return `pi-session:${path.basename(sessionFile, path.extname(sessionFile))}`;
  // HOOK-2: Same pid-reuse fix as getPiAwarenessAgentId — append startup token so
  // sessions from different OS boots with the same PID don't share lock scope.
  return `pi-session:${process.pid}-${_sessionStartupToken}`;
}

export function getPiAwarenessAgentId(ctx?: PiLikeContext): string {
  if (process.env.OCTOCODE_AGENT_ID) return process.env.OCTOCODE_AGENT_ID;

  const sessionFile = ctx?.sessionManager?.getSessionFile?.();
  if (sessionFile) return `pi:${path.basename(sessionFile, path.extname(sessionFile))}`;

  // HOOK-2: Append the startup token to the pid so that two processes with the
  // same pid (OS pid reuse across restarts) produce different agent IDs and do
  // not mix memory contexts. The token is stable for the lifetime of this process.
  return `pi:${process.pid}-${_sessionStartupToken}`;
}

export function notify(ctx: PiLikeContext | undefined, message: string, level: string = 'info'): void {
  ctx?.ui?.notify?.(message, level);
}

export function defaultGetDb(options: PiAwarenessBridgeOptions, ctx?: PiLikeContext): DatabaseSync {
  // HOOK-1: Use the cached connection; never call connectDb twice for the same path.
  return connectCachedDb(ctx?.dbPath ?? options.dbPath ?? resolveDbPath(null));
}

export function ensurePiSession(
  db: DatabaseSync,
  params: { agentId: string; sessionId: string; workspacePath: string; artifact: string | null },
): void {
  ensureRunSession(db, params);
}

export function canonicalPath(input: string): string {
  const resolved = path.resolve(input);
  try {
    return realpathSync(resolved);
  } catch {
    const missingParts: string[] = [];
    let cursor = resolved;
    while (true) {
      const parent = path.dirname(cursor);
      if (parent === cursor) return resolved;
      missingParts.unshift(path.basename(cursor));
      cursor = parent;
      try {
        return path.join(realpathSync(cursor), ...missingParts);
      } catch {
        continue;
      }
    }
  }
}
