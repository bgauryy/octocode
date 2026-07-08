import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { realpathSync } from 'node:fs';
import type { DatabaseSync } from 'node:sqlite';
import { connectCachedDb, resolveDbPath } from './db.js';
import { insertEditLog } from './audit.js';

// HOOK-2: A one-time session startup token that survives process.pid reuse across
// OS restarts. We combine the session file name (if available) with a UUID suffix
// generated once at import time so the agentId is stable within a session but
// unique across sessions even when PIDs repeat.
const _sessionStartupToken = randomUUID().slice(0, 8);
import { normalizeArtifact } from './helpers.js';
import { preFlightIntent, releaseFileLock } from './intents.js';
import { auditUnverified } from './verify.js';
import { notifyGet, sessionCapture } from './maintenance.js';
import { registerAgent } from './agents.js';

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
}

export interface PiAwarenessBridgeOptions {
  pendingToolFiles?: Map<string, string[]>;
  pendingToolTasks?: Map<string, string>;
  dbPath?: string | null;
  getDb?: (ctx?: PiLikeContext) => DatabaseSync;
  skillRoot?: string | null;
}

function addPathValue(paths: string[], value: unknown): void {
  if (typeof value === 'string' && value.trim().length > 0) {
    paths.push(value.trim());
  } else if (Array.isArray(value)) {
    for (const item of value) addPathValue(paths, item);
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

function objectOrEmpty(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? value as Record<string, unknown> : {};
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
  const command = typeof input === 'string'
    ? input
    : firstString(payload.command, payload.patch, payload.text, payload.content);

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

function artifactFrom(ctx?: PiLikeContext, event?: Record<string, unknown>): string | null {
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

function notify(ctx: PiLikeContext | undefined, message: string, level: string = 'info'): void {
  ctx?.ui?.notify?.(message, level);
}

function defaultGetDb(options: PiAwarenessBridgeOptions, ctx?: PiLikeContext): DatabaseSync {
  // HOOK-1: Use the cached connection; never call connectDb twice for the same path.
  return connectCachedDb(ctx?.dbPath ?? options.dbPath ?? resolveDbPath(null));
}

function canonicalPath(input: string): string {
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

function resolvePiTargetPath(file: string, cwd: string): string {
  return path.isAbsolute(file) ? file : path.resolve(cwd, file);
}

function isInsidePath(candidate: string, root: string): boolean {
  const resolvedCandidate = canonicalPath(candidate);
  const resolvedRoot = canonicalPath(root);
  const rel = path.relative(resolvedRoot, resolvedCandidate);
  return rel === '' || Boolean(rel && !rel.startsWith('..') && !path.isAbsolute(rel));
}

function gitBranchOf(dir: string): string | null {
  try {
    const result = spawnSync('git', ['-C', dir, 'rev-parse', '--abbrev-ref', 'HEAD'], {
      encoding: 'utf8',
      timeout: 5000,
    });
    return result.status === 0 ? String(result.stdout).trim() : null;
  } catch {
    return null;
  }
}

function guardPiHarnessEdit(targetFiles: string[], ctx: PiLikeContext | undefined, skillRoot: string | null | undefined): string | null {
  if (!skillRoot) return null;
  const cwd = ctx?.cwd ?? process.cwd();
  const insideSkill = targetFiles.some((file) => isInsidePath(resolvePiTargetPath(file, cwd), skillRoot));
  if (!insideSkill) return null;

  if (process.env.OCTOCODE_ALLOW_HARNESS_APPLY !== '1') {
    return 'Octocode awareness blocked this edit: editing the skill itself is gated. A human must set OCTOCODE_ALLOW_HARNESS_APPLY=1.';
  }

  const branch = gitBranchOf(skillRoot);
  if (branch === 'main' || branch === 'master') {
    return `Octocode awareness blocked this edit: harness self-fix is never allowed on ${branch}. Create a dedicated branch first.`;
  }
  if (!branch || branch === 'HEAD') {
    if (process.env.OCTOCODE_HARNESS_BRANCH_OK !== '1') {
      return 'Octocode awareness blocked this edit: cannot confirm a dedicated git branch for the skill. Create one, or set OCTOCODE_HARNESS_BRANCH_OK=1 to acknowledge.';
    }
  }

  return null;
}

export function createPiAwarenessBridge(options: PiAwarenessBridgeOptions = {}) {
  const pendingToolFiles = options.pendingToolFiles ?? new Map<string, string[]>();
  const pendingToolTasks = options.pendingToolTasks ?? new Map<string, string>();
  const getDb = options.getDb ?? ((ctx?: PiLikeContext) => defaultGetDb(options, ctx));
  const skillRoot = options.skillRoot ?? process.env.OCTOCODE_SKILL_ROOT ?? null;

  return {
    pendingToolFiles,
    pendingToolTasks,

    async handleToolCall(event: PiToolEvent, ctx?: PiLikeContext) {
      if (event?.toolCallId && pendingToolTasks.has(event.toolCallId)) return undefined;
      const targetFiles = extractPiWriteTargetPaths(event?.toolName, event?.input);
      if (targetFiles.length === 0) return undefined;
      const harnessBlockReason = guardPiHarnessEdit(targetFiles, ctx, skillRoot);
      if (harnessBlockReason) return { block: true, reason: harnessBlockReason };

      const agentId = getPiAwarenessAgentId(ctx);
      try {
        const db = getDb(ctx);
        const result = preFlightIntent(db, {
          agentId,
          sessionId: getPiAwarenessSessionId(ctx),
          workspacePath: ctx?.cwd ?? process.cwd(),
          artifact: artifactFrom(ctx, event as Record<string, unknown>),
          rationale: 'auto: Pi write/edit tool call via octocode-awareness',
          testPlan: targetFiles.length > 0
            ? `verify edit applied to: ${targetFiles.slice(0, 3).join(', ')}${targetFiles.length > 3 ? ` + ${targetFiles.length - 3} more` : ''}`
            : 'post-edit verification',
          targetFiles,
          ttlMs: 10 * 60_000,
        });

        if (!result.ok) {
          const detail = (result.conflicts || [])
            .map((conflict) => `${conflict.file_path} (held by ${conflict.agent_id})`)
            .join(', ');
          return { block: true, reason: `Octocode awareness blocked this edit: ${detail || 'conflict'}` };
        }

        if (event?.toolCallId) {
          pendingToolFiles.set(event.toolCallId, targetFiles);
          pendingToolTasks.set(event.toolCallId, result.task.task_id);
        }
        return undefined;
      } catch (error) {
        notify(ctx, `Octocode awareness warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
        return undefined;
      }
    },

    async handleToolResult(event: PiToolEvent, ctx?: PiLikeContext) {
      const targetFiles = event?.toolCallId ? pendingToolFiles.get(event.toolCallId) : undefined;
      const taskId = event?.toolCallId ? pendingToolTasks.get(event.toolCallId) : undefined;
      const fallbackFiles = targetFiles ?? extractPiWriteTargetPaths(event?.toolName, event?.input);
      if (fallbackFiles.length === 0 && !taskId) return undefined;

      if (event?.toolCallId) {
        pendingToolFiles.delete(event.toolCallId);
        pendingToolTasks.delete(event.toolCallId);
      }
      try {
        const db = getDb(ctx);
        const agentId = getPiAwarenessAgentId(ctx);
        const sessionId = getPiAwarenessSessionId(ctx);
        const workspacePath = ctx?.cwd ?? process.cwd();
        const artifact = artifactFrom(ctx, event as Record<string, unknown>);
        releaseFileLock(db, {
          agentId,
          sessionId,
          taskId,
          targetFiles: taskId ? [] : fallbackFiles,
          workspacePath,
          artifact,
          status: 'PENDING',
        });
        for (const file of fallbackFiles) {
          insertEditLog(db, {
            sessionId,
            taskId,
            agentId,
            filePath: resolvePiTargetPath(file, workspacePath),
            operation: 'update',
            workspacePath,
            artifact,
          });
        }
      } catch (error) {
        notify(ctx, `Octocode awareness warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
      }
      return undefined;
    },

    async handleBeforeAgentStart(_event: Record<string, unknown> = {}, ctx?: PiLikeContext) {
      // ARCH-5: Register / refresh agent identity at the start of each session.
      // Uses OCTOCODE_AGENT_NAME env (if set) or session file basename as display name.
      try {
        const db = getDb(ctx);
        const agentId = getPiAwarenessAgentId(ctx);
        const envName = process.env.OCTOCODE_AGENT_NAME ?? '';
        const sessionFile = ctx?.sessionManager?.getSessionFile?.();
        const derivedName = envName
          || (sessionFile ? path.basename(sessionFile, path.extname(sessionFile)) : '');
        registerAgent(db, { agentId, agentName: derivedName, workspacePath: ctx?.cwd ?? process.cwd(), artifact: artifactFrom(ctx, _event), context: 'pi' });
      } catch { /* fail-open: identity registration is non-critical */ }

      if (process.env.OCTOCODE_NO_NOTIFY === '1') return undefined;
      try {
        const db = getDb(ctx);
        const result = notifyGet(db, {
          agent_id: getPiAwarenessAgentId(ctx),
          workspace: ctx?.cwd ?? process.cwd(),
          artifact: artifactFrom(ctx, _event),
          format: 'hook',
        }) as { additionalContext?: string };
        if (!result.additionalContext) return undefined;
        return {
          message: {
            customType: 'octocode-awareness-briefing',
            content: result.additionalContext,
            display: false,
          },
        };
      } catch (error) {
        notify(ctx, `Octocode awareness briefing warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
        return undefined;
      }
    },

    async handleSessionShutdown(event: Record<string, unknown> = {}, ctx?: PiLikeContext) {
      if (process.env.OCTOCODE_NO_SESSION_CAPTURE === '1') return undefined;
      if (event.reason === 'new') return undefined;
      try {
        const db = getDb(ctx);
        sessionCapture(db, {
          agent_id: getPiAwarenessAgentId(ctx),
          workspace: ctx?.cwd ?? process.cwd(),
          artifact: artifactFrom(ctx, event),
          reason: event.reason,
        });
      } catch {
        // fail-open: shutdown hooks must never wedge session replacement/quit
      }
      return undefined;
    },
  };
}

export function wirePiAwarenessHooks(pi: PiLikeApi, options: PiAwarenessBridgeOptions = {}) {
  if (!pi?.on) return null;
  const bridge = createPiAwarenessBridge(options);

  pi.on('tool_call', async (event, ctx) => bridge.handleToolCall(event as PiToolEvent, ctx));
  pi.on('tool_result', async (event, ctx) => bridge.handleToolResult(event as PiToolEvent, ctx));
  pi.on('tool_execution_start', async (event, ctx) => bridge.handleToolCall({
    toolCallId: String(event?.toolCallId ?? ''),
    toolName: String(event?.toolName ?? ''),
    input: event?.args,
  }, ctx));
  pi.on('tool_execution_end', async (event, ctx) => bridge.handleToolResult({
    toolCallId: String(event?.toolCallId ?? ''),
    toolName: String(event?.toolName ?? ''),
  }, ctx));
  pi.on('before_agent_start', async (event, ctx) => bridge.handleBeforeAgentStart(event, ctx));
  pi.on('agent_end', async (_event, ctx) => {
    if (process.env.OCTOCODE_NO_VERIFY_GATE === '1') return undefined;
    try {
      const db = (options.getDb ?? ((hookCtx?: PiLikeContext) => defaultGetDb(options, hookCtx)))(ctx);
      const result = auditUnverified(db, {
        agentId: getPiAwarenessAgentId(ctx),
        workspacePath: ctx?.cwd ?? process.cwd(),
        artifact: artifactFrom(ctx, _event),
      });
      if (result.count === 0) return undefined;
      const plans = result.unverified
        .map((intent) => `${intent.status}:${intent.task_id}: ${intent.test_plan}`)
        .join('; ');
      const stale = result.stale_active
        .map((intent) => `STALE:${intent.task_id}: ${intent.rationale}`)
        .join('; ');
      pi.sendMessage?.({
        customType: 'octocode-awareness-verify-gate',
        content: [
          'Octocode awareness verify gate: you have unverified edits before concluding.',
          plans ? `Pending: ${plans}` : '',
          stale ? `Expired active locks: ${stale}` : '',
          'Run the stated verification, then call memory_verify or octocode-awareness verify to clear the pending tasks.',
        ].filter(Boolean).join('\n'),
        display: true,
      }, { deliverAs: 'followUp', triggerTurn: true });
      return undefined;
    } catch (error) {
      notify(ctx, `Octocode awareness verify warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
      return undefined;
    }
  });
  pi.on('session_before_compact', async (event, ctx) => bridge.handleSessionShutdown({
    reason: typeof event?.reason === 'string' ? `compact:${event.reason}` : 'compact',
  }, ctx));
  pi.on('session_shutdown', async (event, ctx) => bridge.handleSessionShutdown(event, ctx));

  return bridge;
}
