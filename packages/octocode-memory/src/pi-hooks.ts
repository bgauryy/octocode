import path from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { connectDb, resolveDbPath } from './db.js';
import { preFlightIntent, releaseFileLock } from './intents.js';
import { auditUnverified } from './verify.js';
import { notifyGet, sessionCapture } from './stubs.js';

export interface PiLikeSessionManager {
  getSessionFile?: () => string | null | undefined;
}

export interface PiLikeUi {
  notify?: (message: string, level?: string) => void;
}

export interface PiLikeContext {
  cwd?: string;
  dbPath?: string;
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
  dbPath?: string | null;
  getDb?: (ctx?: PiLikeContext) => DatabaseSync;
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

export function extractPiWriteTargetPaths(toolName: unknown, input: unknown = {}): string[] {
  const normalizedToolName = String(toolName ?? '').toLowerCase();
  const isWriteTool = ['write', 'edit', 'multi_edit', 'multiedit', 'notebookedit', 'notebook_edit'].includes(normalizedToolName);
  const payload = objectOrEmpty(input);
  const command = payload.command;

  if (!isWriteTool && typeof command !== 'string') return [];

  const paths: string[] = [];
  addPathValue(paths, payload.path);
  addPathValue(paths, payload.filePath);
  addPathValue(paths, payload.file_path);
  addPathValue(paths, payload.paths);
  addPathValue(paths, payload.filePaths);
  addPathValue(paths, payload.file_paths);
  addApplyPatchPaths(paths, command);

  return [...new Set(paths)];
}

export function getPiAwarenessAgentId(ctx?: PiLikeContext): string {
  if (process.env.OCTOCODE_AGENT_ID) return process.env.OCTOCODE_AGENT_ID;

  const sessionFile = ctx?.sessionManager?.getSessionFile?.();
  if (sessionFile) return `pi:${path.basename(sessionFile, path.extname(sessionFile))}`;

  return `pi:${process.pid}`;
}

function notify(ctx: PiLikeContext | undefined, message: string, level: string = 'info'): void {
  ctx?.ui?.notify?.(message, level);
}

function defaultGetDb(options: PiAwarenessBridgeOptions, ctx?: PiLikeContext): DatabaseSync {
  return connectDb(ctx?.dbPath ?? options.dbPath ?? resolveDbPath(null));
}

export function createPiAwarenessBridge(options: PiAwarenessBridgeOptions = {}) {
  const pendingToolFiles = options.pendingToolFiles ?? new Map<string, string[]>();
  const getDb = options.getDb ?? ((ctx?: PiLikeContext) => defaultGetDb(options, ctx));

  return {
    pendingToolFiles,

    async handleToolCall(event: PiToolEvent, ctx?: PiLikeContext) {
      const targetFiles = extractPiWriteTargetPaths(event?.toolName, event?.input);
      if (targetFiles.length === 0) return undefined;

      const agentId = getPiAwarenessAgentId(ctx);
      try {
        const db = getDb(ctx);
        const result = preFlightIntent(db, {
          agentId,
          workspacePath: ctx?.cwd ?? process.cwd(),
          rationale: 'auto: Pi write/edit tool call via octocode-awareness',
          testPlan: targetFiles.length > 0
            ? `verify edit applied to: ${targetFiles.slice(0, 3).join(', ')}${targetFiles.length > 3 ? ` + ${targetFiles.length - 3} more` : ''}`
            : 'post-edit verification',
          targetFiles,
          ttlMs: 15 * 60000,
        });

        if (!result.ok) {
          const detail = (result.conflicts || [])
            .map((conflict) => `${conflict.file_path} (held by ${conflict.agent_id})`)
            .join(', ');
          return { block: true, reason: `Octocode awareness blocked this edit: ${detail || 'conflict'}` };
        }

        if (event?.toolCallId) pendingToolFiles.set(event.toolCallId, targetFiles);
        return undefined;
      } catch (error) {
        notify(ctx, `Octocode awareness warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
        return undefined;
      }
    },

    async handleToolResult(event: PiToolEvent, ctx?: PiLikeContext) {
      const targetFiles = event?.toolCallId ? pendingToolFiles.get(event.toolCallId) : undefined;
      if (!targetFiles) return undefined;

      pendingToolFiles.delete(event.toolCallId!);
      try {
        const db = getDb(ctx);
        releaseFileLock(db, {
          agentId: getPiAwarenessAgentId(ctx),
          targetFiles,
          status: 'PENDING',
        });
      } catch (error) {
        notify(ctx, `Octocode awareness warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
      }
      return undefined;
    },

    async handleBeforeAgentStart(_event: Record<string, unknown> = {}, ctx?: PiLikeContext) {
      if (process.env.OCTOCODE_NO_NOTIFY === '1') return undefined;
      try {
        const db = getDb(ctx);
        const result = notifyGet(db, {
          agent_id: getPiAwarenessAgentId(ctx),
          workspace: ctx?.cwd ?? process.cwd(),
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
  pi.on('before_agent_start', async (event, ctx) => bridge.handleBeforeAgentStart(event, ctx));
  pi.on('agent_end', async (_event, ctx) => {
    if (process.env.OCTOCODE_NO_VERIFY_GATE === '1') return undefined;
    try {
      const db = (options.getDb ?? ((hookCtx?: PiLikeContext) => defaultGetDb(options, hookCtx)))(ctx);
      const result = auditUnverified(db, {
        agentId: getPiAwarenessAgentId(ctx),
        workspacePath: ctx?.cwd ?? process.cwd(),
      });
      if (result.count === 0) return undefined;
      const plans = result.unverified
        .map((intent) => `${intent.status}:${intent.intent_id}: ${intent.test_plan}`)
        .join('; ');
      pi.sendMessage?.({
        customType: 'octocode-awareness-verify-gate',
        content: [
          'Octocode awareness verify gate: you have unverified edits before concluding.',
          `Pending: ${plans}`,
          'Run the stated verification, then call memory_verify or octocode-awareness verify to clear the pending intents.',
        ].join('\n'),
        display: true,
      }, { deliverAs: 'followUp', triggerTurn: true });
      return undefined;
    } catch (error) {
      notify(ctx, `Octocode awareness verify warning; continuing: ${error instanceof Error ? error.message : String(error)}`, 'warning');
      return undefined;
    }
  });
  pi.on('session_shutdown', async (event, ctx) => bridge.handleSessionShutdown(event, ctx));

  return bridge;
}
