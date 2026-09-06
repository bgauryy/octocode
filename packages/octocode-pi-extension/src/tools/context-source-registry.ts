import type { ContextSegmentV1 } from '@octocodeai/octocode-awareness';
import type { PiContext } from '../types.js';
import {
  assembleContextSegments,
  estimateContextTokens,
  type ContextSegmentInput,
} from './context-segments.js';
import { promptLifecycleFor } from './prompt-lifecycle.js';
import { resolveSessionIdentity } from './session-artifacts.js';
import type { CurrentRehydrationSource } from './rehydration-orchestrator.js';

export type RegisteredContextKind = Extract<ContextSegmentV1['kind'],
  'user-request' | 'peer-event' | 'tool-result' | 'memory-lead' | 'skill' | 'tool-contract'>;

export interface CurrentContextSourceOwnerV1 {
  version: 1;
  id: string;
  kind: RegisteredContextKind;
  origin: string;
  authority: ContextSegmentV1['authority'];
  scope: ContextSegmentV1['scope'];
  visibility: ContextSegmentV1['visibility'];
  rehydrate: ContextSegmentV1['rehydrate'];
  tokenBudget?: number;
  /** Restore-only owners can validate a checkpoint without being captured until selected. */
  capture?: boolean;
  /** Re-read the production owner's current bytes. Artifact bodies are never passed here. */
  readCurrent(ctx: PiContext): string | undefined;
}

export interface CurrentContextSourceCapture {
  segments: ContextSegmentV1[];
  contents: Record<string, string>;
  sources: CurrentRehydrationSource[];
  unavailable: string[];
  invalid: string[];
  overBudget: string[];
  estimatedTokens: number;
}

interface RegisteredOwner {
  owner: CurrentContextSourceOwnerV1;
  sequence: number;
}

const ownersBySession = new Map<string, Map<string, RegisteredOwner>>();
let sequence = 0;
const MAX_REGISTERED_CONTEXT_SOURCES = 128;
const CURRENT_CONTEXT_CAPTURE_TOKEN_BUDGET = 8_000;

function registryKey(ctx: PiContext): string {
  return resolveSessionIdentity(ctx).sessionKey;
}

function assertOwnerPolicy(owner: CurrentContextSourceOwnerV1): void {
  if (owner.version !== 1 || !owner.id.trim() || !owner.origin.trim()) {
    throw new Error('current context source identity is incomplete');
  }
  if (owner.kind === 'user-request' && owner.authority !== 'user') {
    throw new Error('user-request sources must retain user authority');
  }
  if (['peer-event', 'tool-result', 'memory-lead'].includes(owner.kind) && owner.authority !== 'external-data') {
    throw new Error(`${owner.kind} sources must retain external-data authority`);
  }
  if (owner.kind === 'skill' && owner.authority === 'product') {
    throw new Error('selected skill sources cannot select product authority');
  }
  if (owner.kind === 'tool-contract' && owner.authority === 'product' && owner.origin !== 'octocode-harness') {
    throw new Error('demand-loaded tool sources cannot select product authority');
  }
  if (owner.rehydrate === 'never') throw new Error('registered current sources must be rehydratable');
  if (owner.tokenBudget !== undefined && (!Number.isInteger(owner.tokenBudget) || owner.tokenBudget <= 0)) {
    throw new Error('current context source tokenBudget must be a positive integer');
  }
}

/** Register one production owner. Registration stores a reader, never captured bytes. */
export function registerCurrentContextSource(
  ctx: PiContext,
  owner: CurrentContextSourceOwnerV1,
): () => void {
  assertOwnerPolicy(owner);
  const key = registryKey(ctx);
  const owners = ownersBySession.get(key) ?? new Map<string, RegisteredOwner>();
  ownersBySession.set(key, owners);
  const prior = owners.get(owner.id);
  owners.set(owner.id, { owner, sequence: prior?.sequence ?? sequence++ });
  if (owners.size > MAX_REGISTERED_CONTEXT_SOURCES) {
    // Restore-only transcript readers can be rebuilt from durable entries. Keep
    // capture owners (especially session memory) through long tool histories.
    // Include the new owner so it cannot displace capture state at a full cap.
    const oldest = [...owners.entries()].sort(([, a], [, b]) =>
      Number(a.owner.capture !== false) - Number(b.owner.capture !== false)
      || a.sequence - b.sequence)[0];
    if (oldest) owners.delete(oldest[0]);
  }
  return () => {
    const current = ownersBySession.get(key);
    if (current?.get(owner.id)?.owner === owner) current.delete(owner.id);
    if (current?.size === 0) ownersBySession.delete(key);
  };
}

export function clearCurrentContextSources(ctx?: PiContext): void {
  if (!ctx) {
    ownersBySession.clear();
    sequence = 0;
    return;
  }
  ownersBySession.delete(registryKey(ctx));
}

export function captureCurrentContextSources(
  ctx: PiContext,
  options: { totalTokenBudget?: number; includeRestoreOnly?: boolean } = {},
): CurrentContextSourceCapture {
  const owners = [...(ownersBySession.get(registryKey(ctx))?.values() ?? [])]
    .filter(({ owner }) => options.includeRestoreOnly === true || owner.capture !== false)
    .sort((a, b) => a.sequence - b.sequence || a.owner.id.localeCompare(b.owner.id));
  const contents: Record<string, string> = {};
  const segments: ContextSegmentV1[] = [];
  const sources: CurrentRehydrationSource[] = [];
  const unavailable: string[] = [];
  const invalid: string[] = [];
  const overBudget: string[] = [];
  let estimatedTokens = 0;
  const totalBudget = options.totalTokenBudget ?? CURRENT_CONTEXT_CAPTURE_TOKEN_BUDGET;

  for (const { owner } of owners) {
    let content: string | undefined;
    try {
      content = owner.readCurrent(ctx);
    } catch {
      unavailable.push(owner.id);
      continue;
    }
    if (content === undefined || content.trim().length === 0) {
      unavailable.push(owner.id);
      continue;
    }
    const tokenBudget = owner.tokenBudget ?? promptLifecycleFor(owner.kind).defaultTokenBudget;
    const tokens = estimateContextTokens(content);
    if (tokens > tokenBudget || estimatedTokens + tokens > totalBudget) {
      overBudget.push(owner.id);
      continue;
    }
    const input: ContextSegmentInput = {
      id: owner.id,
      content,
      kind: owner.kind,
      origin: owner.origin,
      authority: owner.authority,
      scope: owner.scope,
      visibility: owner.visibility,
      rehydrate: owner.rehydrate,
      tokenBudget,
    };
    try {
      const assembled = assembleContextSegments([input]);
      const segment = assembled.manifest[0];
      if (!segment) continue;
      segments.push(segment);
      contents[segment.id] = content;
      sources.push({ segment, content });
      estimatedTokens += tokens;
    } catch {
      invalid.push(owner.id);
    }
  }

  return {
    segments,
    contents,
    sources,
    unavailable: unavailable.sort(),
    invalid: invalid.sort(),
    overBudget: overBudget.sort(),
    estimatedTokens,
  };
}

/** Fixed owners win identity collisions so dynamic data cannot shadow product contracts. */
export function mergeCurrentContextSources(
  ctx: PiContext,
  fixed: CurrentRehydrationSource[],
  options: { totalTokenBudget?: number } = {},
): CurrentRehydrationSource[] {
  const fixedIds = new Set(fixed.map((source) => source.segment.id));
  const dynamic = captureCurrentContextSources(ctx, { ...options, includeRestoreOnly: true }).sources
    .filter((source) => !fixedIds.has(source.segment.id));
  return [...fixed, ...dynamic];
}

function record(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
}

function messageFromEntry(entry: unknown): Record<string, unknown> | undefined {
  const candidate = record(entry);
  if (!candidate) return undefined;
  return record(candidate.message) ?? candidate;
}

function textContent(content: unknown): string | undefined {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return undefined;
  const parts = content.flatMap((part) => {
    const value = record(part);
    return value?.type === 'text' && typeof value.text === 'string' ? [value.text] : [];
  });
  return parts.length > 0 ? parts.join('\n') : undefined;
}

function sessionEntries(ctx: PiContext): unknown[] {
  // getEntries includes sibling branches. Only the active root-to-leaf branch
  // can supply current user, tool, and peer context after tree navigation.
  return ctx.sessionManager?.getBranch?.() ?? ctx.sessionManager?.getEntries?.() ?? [];
}

export function readLatestSessionUserRequest(ctx: PiContext): string | undefined {
  const entries = sessionEntries(ctx);
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const message = messageFromEntry(entries[index]);
    if (message?.role === 'user') return textContent(message.content);
  }
  return undefined;
}

export function readSessionToolResult(ctx: PiContext, toolCallId: string): string | undefined {
  const entries = sessionEntries(ctx);
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const message = messageFromEntry(entries[index]);
    if (message?.role === 'toolResult' && message.toolCallId === toolCallId) return textContent(message.content);
  }
  return undefined;
}

export function readSessionPeerEvent(ctx: PiContext, eventId: string): string | undefined {
  const entries = sessionEntries(ctx);
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const message = messageFromEntry(entries[index]);
    const details = record(message?.details) ?? record(record(entries[index])?.data);
    if (message?.customType === 'octocode-peer-event' && details?.eventId === eventId) {
      return textContent(message.content) ?? (typeof message.content === 'string' ? message.content : undefined);
    }
  }
  return undefined;
}

export function sessionUserRequestOrigin(): string {
  return 'session-user:latest';
}

export function sessionToolResultOrigin(toolCallId: string): string {
  return `session-tool:${encodeURIComponent(toolCallId)}`;
}

export function sessionPeerEventOrigin(eventId: string): string {
  return `session-peer:${encodeURIComponent(eventId)}`;
}

/** Rebuild transcript-owned current sources directly from durable session entries. */
export function resolveSessionCheckpointSources(
  ctx: PiContext,
  checkpoints: ContextSegmentV1[],
): CurrentRehydrationSource[] {
  const sources: CurrentRehydrationSource[] = [];
  for (const segment of checkpoints) {
    let content: string | undefined;
    if (segment.origin === sessionUserRequestOrigin() && segment.kind === 'user-request') {
      content = readLatestSessionUserRequest(ctx);
    } else if (segment.origin.startsWith('session-tool:')) {
      content = readSessionToolResult(ctx, decodeURIComponent(segment.origin.slice('session-tool:'.length)));
    } else if (segment.origin.startsWith('session-peer:') && segment.kind === 'peer-event') {
      content = readSessionPeerEvent(ctx, decodeURIComponent(segment.origin.slice('session-peer:'.length)));
    }
    if (content === undefined) continue;
    sources.push({ segment: { ...segment, digest: assembleContextSegments([{
      id: segment.id,
      content,
      kind: segment.kind,
      origin: segment.origin,
      authority: segment.authority,
      scope: segment.scope,
      visibility: segment.visibility,
      rehydrate: segment.rehydrate,
      ...(segment.tokenBudget === undefined ? {} : { tokenBudget: segment.tokenBudget }),
    }]).manifest[0]!.digest }, content });
  }
  return sources;
}
