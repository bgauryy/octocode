/** Stable identity shared by native Pi events and the Awareness CLI environment. */
import path from 'node:path';
import { createHash } from 'node:crypto';
import type { PiContext } from '../types.js';

/**
 * Keep explicit user/worker identities, but refresh identities generated here
 * when Pi switches sessions. Calls without a session retain the outgoing ID so
 * shutdown can leave its registry row before the next session joins.
 * Shared by native event delivery, mutation guards and the CLI environment.
 */
let generatedAgentId: string | undefined;

function nonEmptyString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

/** Restart-stable identity for session-scoped routing and durable event cursors. */
export function resolveAwarenessSessionAgentId(ctx?: PiContext): string | undefined {
  const sessionId = nonEmptyString(ctx?.sessionManager?.getSessionId?.());
  if (sessionId) return `pi:${sessionId}`;
  const sessionFile = nonEmptyString(ctx?.sessionManager?.getSessionFile?.());
  if (!sessionFile) return undefined;
  const digest = createHash('sha256').update(path.resolve(sessionFile)).digest('hex').slice(0, 24);
  return `pi:file:${digest}`;
}

export function getAwarenessAgentId(ctx?: PiContext): string {
  const configured = process.env.OCTOCODE_AGENT_ID;
  if (configured && configured !== generatedAgentId) return configured;
  const sessionAgentId = resolveAwarenessSessionAgentId(ctx);
  if (!sessionAgentId && configured) return configured;
  const resolvedAgentId = sessionAgentId ?? `pi:${process.pid}`;
  generatedAgentId = resolvedAgentId;
  process.env.OCTOCODE_AGENT_ID = resolvedAgentId;
  return resolvedAgentId;
}

/** Human labels are separate from routing IDs; provider labels are reported, never guessed. */
export function getAwarenessAgentIdentity(ctx?: PiContext): {
  agentId: string;
  name: string;
  metadata: { vendor: string | null; host: string };
} {
  const agentId = getAwarenessAgentId(ctx);
  return {
    agentId,
    name: process.env.OCTOCODE_AGENT_NAME?.trim() || ctx?.sessionManager?.getSessionName?.()?.trim() || agentId,
    metadata: {
      // Workers inherit their parent's environment, but may select a different provider.
      vendor: ctx?.model?.provider?.trim() || process.env.OCTOCODE_AGENT_VENDOR?.trim() || null,
      host: 'pi',
    },
  };
}
