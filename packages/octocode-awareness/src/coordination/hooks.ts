import { resolve } from 'node:path';
import { openAwarenessStore } from './open.js';
import { type Lock } from '@octocodeai/octocode-shared/entities';
import type { AwarenessStorageScope } from '../storage-scope.js';

export type HookHost = 'claude' | 'codex' | 'copilot' | 'cursor' | 'gemini' | 'opencode' | 'pi' | 'generic';

export interface PreEditHookOptions {
  workspace?: string;
  dbPath?: string;
  scope?: AwarenessStorageScope;
  agentId: string;
  host?: HookHost;
  event?: unknown;
}

export interface LockConflict {
  filePath: string;
  lock: Lock;
}

export interface PreEditHookResult {
  ok: boolean;
  blocked: boolean;
  agentId: string;
  files: string[];
  conflicts: LockConflict[];
  message?: string;
}

function objectOrEmpty(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {};
}

function addPathValue(paths: string[], value: unknown): void {
  if (typeof value === 'string' && value.trim().length > 0) {
    paths.push(value.trim());
  } else if (Array.isArray(value)) {
    for (const item of value) addPathValue(paths, item);
  }
}

function firstString(...values: unknown[]): string | null {
  for (const value of values) {
    if (typeof value === 'string' && value.trim()) return value.trim();
  }
  return null;
}

function addQueryPaths(paths: string[], value: unknown): void {
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

function addApplyPatchPaths(paths: string[], command: unknown): void {
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

function normalizedToolName(event: Record<string, unknown>): string {
  const direct = firstString(event.toolName, event.tool_name, event.name, event.tool);
  if (direct) return direct.toLowerCase();
  const toolInput = objectOrEmpty(event.tool_input);
  return firstString(toolInput.toolName, toolInput.tool_name, toolInput.name, toolInput.tool)?.toLowerCase() ?? '';
}

export function extractHookTargetPaths(event: unknown): string[] {
  const payload = objectOrEmpty(event);
  const input = objectOrEmpty(payload.input ?? payload.tool_input ?? payload.params ?? payload.arguments);
  const toolName = normalizedToolName(payload);
  const isWriteTool = [
    'write',
    'edit',
    'multi_edit',
    'multiedit',
    'notebookedit',
    'notebook_edit',
    'strreplace',
    'delete',
    'apply_patch',
    'applypatch',
  ].includes(toolName);

  const paths: string[] = [];
  if (isWriteTool || !toolName) {
    addPathValue(paths, input.path);
    addPathValue(paths, input.filePath);
    addPathValue(paths, input.file_path);
    addPathValue(paths, input.paths);
    addPathValue(paths, input.filePaths);
    addPathValue(paths, input.file_paths);
    addPathValue(paths, payload.path);
    addPathValue(paths, payload.filePath);
    addPathValue(paths, payload.file_path);
    addQueryPaths(paths, input.queries);
  }

  const command = typeof event === 'string'
    ? event
    : firstString(input.command, input.patch, payload.command, payload.patch);
  addApplyPatchPaths(paths, command);

  return [...new Set(paths.filter(Boolean))];
}

export function checkLockConflicts(params: {
  workspace: string;
  dbPath?: string;
  scope?: AwarenessStorageScope;
  agentId: string;
  files: string[];
}): LockConflict[] {
  const aw = openAwarenessStore({ workspace: params.workspace, dbPath: params.dbPath, scope: params.scope });
  try {
    const targetSet = new Set(params.files.map((file) => resolve(params.workspace, file)));
    return aw.listLocks()
      .filter((lock) => targetSet.has(lock.filePath) && lock.agentId !== params.agentId)
      .map((lock) => ({ filePath: lock.filePath, lock }));
  } finally {
    aw.close();
  }
}

export function runPreEditLockGate(options: PreEditHookOptions): PreEditHookResult {
  const workspace = resolve(options.workspace ?? process.cwd());
  const files = extractHookTargetPaths(options.event);
  const conflicts = files.length > 0
    ? checkLockConflicts({ workspace, dbPath: options.dbPath, scope: options.scope, agentId: options.agentId, files })
    : [];
  const blocked = conflicts.length > 0;
  return {
    ok: !blocked,
    blocked,
    agentId: options.agentId,
    files: files.map((file) => resolve(workspace, file)),
    conflicts,
    message: blocked
      ? `Awareness lock conflict: ${conflicts.map((conflict) => `${conflict.filePath} held by ${conflict.lock.agentId}`).join('; ')}`
      : undefined,
  };
}
