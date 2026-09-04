/**
 * Shared plumbing for the first-class Awareness coordination tools
 * (lock/task/work/handoff/verify/message/agent/status). Each is a thin,
 * in-process wrapper over `@octocodeai/octocode-awareness`, the same library
 * the CLI exposes — no child process — so the model reaches the shared ledger as
 * typed, tool-stream-visible operations instead of hand-built CLI flags.
 */

import path from 'node:path';
import {
  dispatchAwarenessCommand,
  type AwarenessCommandRequest,
} from '@octocodeai/octocode-awareness';
import type { ToolCallResult, PiContext, PiTheme } from '../types.js';
import { buildToolView } from './render-helpers.js';
import { openPersistentAwareness } from './storage-policy.js';

/**
 * The session-stable Awareness agent id. OCTOCODE_AGENT_ID (set at
 * session_start) wins so every surface — hooks, tools, the registry — agrees on
 * WHO this session is; otherwise derive `pi:<sessionId|pid>` and cache it.
 * Single source shared by index.ts and the coordination tools.
 */
export function getAwarenessAgentId(ctx?: PiContext): string {
  if (process.env.OCTOCODE_AGENT_ID) return process.env.OCTOCODE_AGENT_ID;
  const sessionId = ctx?.sessionManager?.getSessionId?.()
    ?? (ctx?.sessionManager?.getSessionFile?.() ? path.basename(ctx.sessionManager.getSessionFile()!) : undefined);
  const agentId = `pi:${sessionId || process.pid}`;
  process.env.OCTOCODE_AGENT_ID = agentId;
  return agentId;
}

export interface AwarenessJsonResult {
  ok: boolean;
  code: number;
  json: unknown;
  error?: string;
}

/**
 * Run a STRUCTURED Awareness command through the shared dispatcher — the
 * exact same command→library mapping the `octocode-awareness` CLI uses.
 * This is how the extension's first-class tools reach the ledger without
 * building CLI arg-vectors and round-tripping them back through the parser: one
 * logic path, so the tools and the CLI can never drift. The result shape mirrors
 * `runAwarenessJson` so call sites are interchangeable (exit 2 for a still-held
 * `lock wait` is a domain result, not an error). Never throws.
 */
export function runAwarenessCommand(req: AwarenessCommandRequest, cwd: string, dbPath?: string): AwarenessJsonResult {
  let aw: ReturnType<typeof openPersistentAwareness> | undefined;
  try {
    aw = openPersistentAwareness({ workspace: cwd, ...(dbPath ? { dbPath } : {}) });
    const { result, exitCode } = dispatchAwarenessCommand(aw, req);
    const record = result && typeof result === 'object' && !Array.isArray(result) ? (result as Record<string, unknown>) : null;
    const failed = exitCode !== 0 && exitCode !== 2;
    return {
      ok: !failed,
      code: exitCode,
      json: result,
      error: failed ? (typeof record?.['error'] === 'string' ? (record['error'] as string) : `exit ${exitCode}`) : undefined,
    };
  } catch (err) {
    // Unknown command/action or a missing required param throws — surface it the
    // same way the CLI's non-zero exit would (code 1), not as a crash.
    return { ok: false, code: 1, json: null, error: err instanceof Error ? err.message : String(err) };
  } finally {
    aw?.close();
  }
}

export function awarenessError(text: string): ToolCallResult {
  return { content: [{ type: 'text', text }], isError: true } as unknown as ToolCallResult;
}

/** Uniform success result: model payload once; details retain only renderer metadata. */
export function awarenessOk(summary: string, action: string, json: unknown): ToolCallResult {
  const text = json === null || json === undefined ? summary : `${summary}\n${JSON.stringify(json)}`;
  return { content: [{ type: 'text', text }], details: { action, count: countRows(json) } } as unknown as ToolCallResult;
}

/** Shared renderCall: `⟨title⟩ action hint`. */
export function renderAwarenessCall(toolName: string, action: string, hint: string, theme?: PiTheme) {
  return buildToolView({
    name: toolName,
    state: 'request',
    segments: [{ text: action, token: 'bright' }, ...(hint ? [{ text: hint, token: 'dim' as const }] : [])],
  }, theme);
}

/** Shared renderResult: success/error glyph + first summary line. */
export function renderAwarenessResult(toolName: string, result: ToolCallResult, theme?: PiTheme) {
  const first = ((result.content?.[0] as { text?: string } | undefined)?.text ?? '').split('\n')[0] || 'awareness';
  const waiting = /still held|not free|conflict/i.test(first);
  const ok = !result.isError;
  return buildToolView({
    name: toolName,
    state: !ok ? 'error' : waiting ? 'warning' : 'success',
    segments: [{ text: first, token: !ok ? 'error' : waiting ? 'warning' : 'dim' }],
  }, theme);
}

/** Count rows in an array-or-{items|results} JSON shape, for summaries. */
export function countRows(json: unknown): number {
  if (Array.isArray(json)) return json.length;
  if (json && typeof json === 'object') {
    for (const key of ['items', 'results', 'pending', 'plans', 'tasks', 'locks', 'work', 'handoffs', 'agents', 'messages']) {
      const v = (json as Record<string, unknown>)[key];
      if (Array.isArray(v)) return v.length;
    }
  }
  return 0;
}

// Pi names the generic external-host contract “unified plan”; the package owns
// every state transition and verification invariant.
export {
  completeExternalPlanTask as completeUnifiedPlanTask,
  finalizeExternalPlan as finalizeUnifiedPlan,
  projectExternalPlan as projectUnifiedPlan,
  type ExternalPlanCompletionResult as UnifiedPlanCompletionResult,
  type ExternalPlanProjectionInput as UnifiedPlanProjectionInput,
  type ExternalPlanProjectionResult as UnifiedPlanProjectionResult,
  type ExternalPlanProjectionStep as UnifiedPlanProjectionStep,
  type ExternalPlanScope as UnifiedPlanScope,
  type ObservedCheckReceipt,
} from '@octocodeai/octocode-awareness';
