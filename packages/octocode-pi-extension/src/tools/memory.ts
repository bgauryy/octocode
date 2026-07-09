/**
 * In-process awareness operation runner.
 *
 * NOTE: awareness memory/coordination is NOT exposed as agent tools. Agents
 * reach awareness state through the `octocode-awareness` CLI (see
 * $OCTOCODE_AWARENESS_CLI) driven by the octocode-awareness skill, and the
 * awareness hooks automate the edit/verify lifecycle.
 *
 * This module remains only to back the user-facing `/octocode-memory-digest`
 * and `/octocode-memory-forget` slash commands, which run digest/forget
 * in-process against the shared awareness store.
 */
import {
  connectCachedDb,
  getPiAwarenessSessionId,
  resolveDbPath,
  runAwarenessToolOperation,
} from '@octocodeai/octocode-awareness';
import type { PiContext, ToolCallResult } from '../types.js';

type AgentIdResolver = (ctx: PiContext | undefined) => string;

export type MemoryType = 'digest' | 'forget';

function withMemoryDb(
  type: MemoryType,
  params: Record<string, unknown>,
  getAgentId: AgentIdResolver,
  ctx: PiContext | undefined
): ToolCallResult {
  const db = connectCachedDb(ctx?.dbPath ?? resolveDbPath(null));
  const cwd = ctx?.cwd ?? process.cwd();
  const result = runAwarenessToolOperation(db, type, params, {
    agentId: getAgentId(ctx),
    cwd,
    sessionId: getPiAwarenessSessionId(ctx),
  });
  return {
    content: [{ type: 'text', text: JSON.stringify(result.payload) }],
    details: { exit: result.exitCode },
  };
}

export function executeMemoryOperation(
  type: MemoryType,
  params: Record<string, unknown>,
  getAgentId: AgentIdResolver,
  ctx?: PiContext
): ToolCallResult {
  try {
    return withMemoryDb(type, params, getAgentId, ctx);
  } catch (err) {
    return {
      content: [{ type: 'text', text: `Failed: ${(err as Error).message}` }],
      details: { exit: 1 },
    };
  }
}
