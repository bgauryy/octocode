/**
 * First-class Pi coordination tools over the in-process Awareness library.
 * The model-facing surface is intentionally narrow: exceptional exclusive locks
 * and peer messages. Shared state is signalled automatically; plans, tasks,
 * verification, and work presence belong to unified plan/mutation flows.
 */

import { evaluatePeerInbound } from '@octocodeai/octocode-awareness';
import type { ToolDefinition, ToolCallResult, PiTheme, PiContext } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { openPersistentAwareness } from './storage-policy.js';
import { buildQueryCallBlocks, buildQueryResultRows } from './render-helpers.js';
import {
  buildQueryEnvelopeSchema,
  executeQueryBatch,
  QUERY_BATCH_MAX_ITEMS,
  QUERY_REASONING_MAX_LENGTH,
} from './query-envelope.js';
import {
  getAwarenessAgentId,
  awarenessError,
  awarenessOk,
  renderAwarenessCall,
  renderAwarenessResult,
} from './awareness-shared.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type TSchema = import('typebox').TSchema;
type RegisterFn = typeof registerUniqueTool;
type Params = Record<string, unknown>;

const str = (v: unknown): string => (v === undefined || v === null ? '' : String(v)).trim();
const renderedQuery = (value: Params): Params => Array.isArray(value['queries'])
  ? (value['queries'][0] as Params | undefined) ?? {}
  : value;

// ─── Schema generation (best-practice discriminated union by `action`) ─────────

interface PreparedLockQuery {
  action: 'acquire' | 'release' | 'wait';
  params: Params;
}

function prepareLockQueries(raw: Record<string, unknown>): PreparedLockQuery[] | { error: string } {
  const queries = raw['queries'];
  if (!Array.isArray(queries) || queries.length === 0) return { error: 'queries must be a non-empty array.' };
  if (queries.length > QUERY_BATCH_MAX_ITEMS) return { error: `queries supports at most ${QUERY_BATCH_MAX_ITEMS} operations per call.` };
  const prepared: PreparedLockQuery[] = [];
  for (const [index, value] of queries.entries()) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return { error: `queries[${index}] must be an object.` };
    const params = value as Params;
    const reasoning = str(params['reasoning']);
    const action = str(params['action']);
    const file = str(params['file']);
    const testPlan = str(params['testPlan']);
    if (!reasoning) return { error: `queries[${index}] requires non-empty reasoning.` };
    if (reasoning.length > QUERY_REASONING_MAX_LENGTH) return { error: `queries[${index}] reasoning must be at most ${QUERY_REASONING_MAX_LENGTH} characters.` };
    if (action !== 'acquire' && action !== 'release' && action !== 'wait') return { error: `queries[${index}] has an unknown lock action.` };
    if (!file) return { error: `queries[${index}] lock ${action} requires file.` };
    if (action === 'acquire' && !testPlan) return { error: `queries[${index}] lock acquire requires testPlan.` };
    prepared.push({ action, params });
  }
  return prepared;
}

// ─── Exceptional explicit lock wrapper ────────────────────────────────────────

function buildLockParameters(Type: TypeBoxBuilder): TSchema {
  const withFile = (title: string, actionConst: string, required: string[] = []) => ({ title, properties: { action: { const: actionConst } }, required: ['reasoning', 'action', 'file', ...required] });
  const itemSchema = Type.Object({
    reasoning: Type.String({ minLength: 1, maxLength: QUERY_REASONING_MAX_LENGTH, description: 'Why this lock operation is necessary.' }),
    action: Type.Unsafe({ type: 'string', enum: ['acquire', 'release', 'wait'], description: 'Exceptional lock action.' }),
    file: Type.Optional(Type.String({ description: 'Workspace-relative file path.' })),
    testPlan: Type.Optional(Type.String({ description: 'Required verification plan for acquire.' })),
    ttlSeconds: Type.Optional(Type.Integer({ minimum: 1, maximum: 3600, description: 'Lease seconds (default 1800).' })),
    waitMs: Type.Optional(Type.Integer({ minimum: 0, maximum: 60000, description: 'Max ms to wait for a peer-held lock.' })),
  }, { additionalProperties: false, oneOf: [withFile('acquire', 'acquire', ['testPlan']), withFile('release', 'release'), withFile('wait', 'wait')] });
  return buildQueryEnvelopeSchema(Type, itemSchema, {
    maxItems: QUERY_BATCH_MAX_ITEMS,
    reasoningDescription: 'Why this lock operation is necessary.',
  });
}

function summarizeLock(action: string, json: unknown, p: Params): string {
  const file = str(p['file']);
  if (action === 'release') return (json as { released?: boolean } | null)?.released ? `Released lock on ${file}.` : `No lock to release on ${file}.`;
  if (action === 'wait') {
    const result = json as { lockFree?: boolean; conflict?: { agentId?: string } } | null;
    return result?.lockFree ? `Lock on ${file} is free.` : `Lock on ${file} still held by ${result?.conflict?.agentId ?? 'a peer'}.`;
  }
  return `Locked ${file}.`;
}

function buildLockTool(Type: TypeBoxBuilder): ToolDefinition {
  return {
    name: 'lock', label: 'Lock',
    description: ['Exceptional exclusive file lock for non-mergeable work: single-writer configs, migration scripts, shared counters, or files where concurrent edits cannot be merged.', 'Mutation-time conflict checks are automatic — do not lock for ordinary mergeable edits.', 'On peer conflict: inspect the holder (message inbox); use waitMs to wait briefly; release your lock when done (always release).', 'Actions: acquire, wait (blocks until free or waitMs exceeded), release.'].join('\n'),
    promptSnippet: 'Exceptional exclusive locks; mutation-time conflict checks are automatic', parameters: buildLockParameters(Type),
    async execute(toolCallId: string, raw: Record<string, unknown>, signal: AbortSignal | undefined, onUpdate: unknown, ctx?: PiContext): Promise<ToolCallResult> {
      const prepared = prepareLockQueries(raw);
      if (!Array.isArray(prepared)) return awarenessError(`[lock] ${prepared.error}`);
      return executeQueryBatch({
        toolCallId,
        raw,
        signal,
          onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
          ctx,
          passthroughSingle: true,
          async execute(_query, index) {
            const operation = prepared[index]!;
            const agentId = getAwarenessAgentId(ctx);
            const filePath = str(operation.params['file']);
            const workspace = ctx?.cwd ?? process.cwd();
            let aw: ReturnType<typeof openPersistentAwareness> | undefined;
            try {
              aw = openPersistentAwareness({ workspace });
              const json = operation.action === 'acquire'
                ? aw.acquireLock({ filePath, agentId, reason: str(operation.params['reasoning']), testPlan: str(operation.params['testPlan']), ttlSeconds: Number(operation.params['ttlSeconds']) || undefined })
                : operation.action === 'wait'
                  ? aw.waitForLock({ filePath, agentId, waitMs: Number(operation.params['waitMs']) || 0 })
                  : (() => {
                    const lock = aw.listLocks().find((candidate) => candidate.agentId === agentId
                      && (candidate.filePath === filePath || candidate.filePath.endsWith(`/${filePath}`)));
                    return lock ? aw.releaseLock({ filePath: lock.filePath, agentId, runId: lock.runId }) : { released: false };
                  })();
              return awarenessOk(summarizeLock(operation.action, json, operation.params), operation.action, json);
            } catch (error) {
              return awarenessError(`[lock] ${error instanceof Error ? error.message : String(error)}`);
            } finally {
              aw?.close();
            }
          },
      });
    },
    renderCall(raw: unknown, theme?: PiTheme) {
      return buildQueryCallBlocks(raw, theme, (envelope) => {
        const query = renderedQuery(envelope);
        return renderAwarenessCall('lock', str(query['action']), str(query['file']), theme);
      });
    },
    renderResult(result: ToolCallResult, _opts: unknown, theme?: PiTheme) {
      return buildQueryResultRows('lock', result, theme) ?? renderAwarenessResult('lock', result, theme);
    },
  } as unknown as ToolDefinition;
}

export function applyPeerInboundPolicy(value: unknown, expectedAgentId: string): unknown {
  if (!Array.isArray(value)) return value;
  return value.map((candidate) => {
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) return candidate;
    const message = candidate as Record<string, unknown>;
    const policy = evaluatePeerInbound({
      fromAgentId: str(message['fromAgentId']),
      toAgentId: str(message['toAgentId']) || null,
      expectedAgentId,
      topic: str(message['topic']) || null,
      text: str(message['text']),
    });
    const safeText = policy.decision === 'accept'
      ? policy.attributedText
      : `[peer message ${policy.decision}: ${policy.reason}; class:${policy.messageClass}]`;
    return { ...message, text: safeText, inboundPolicy: policy };
  });
}

interface PreparedMessageQuery {
  action: 'send' | 'read';
  params: Params;
}

function buildMessageParameters(Type: TypeBoxBuilder): TSchema {
  const item = Type.Object({
    reasoning: Type.String({ minLength: 1, maxLength: QUERY_REASONING_MAX_LENGTH, description: 'Reason.' }),
    action: Type.Unsafe({ type: 'string', enum: ['send', 'read'] }),
    to: Type.Optional(Type.String({ description: 'Recipient; omit to broadcast.' })),
    text: Type.Optional(Type.String({ description: 'Send text.' })),
    topic: Type.Optional(Type.String()),
    files: Type.Optional(Type.Array(Type.String())),
    includeRead: Type.Optional(Type.Boolean({ description: 'Include read.' })),
    markRead: Type.Optional(Type.Boolean({ description: 'Mark accepted read.' })),
    limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 100 })),
  }, {
    additionalProperties: false,
    oneOf: [
      { title: 'send', properties: { action: { const: 'send' } }, required: ['action', 'reasoning', 'text'] },
      { title: 'read', properties: { action: { const: 'read' } }, required: ['action', 'reasoning'] },
    ],
  });
  return buildQueryEnvelopeSchema(Type, item, { maxItems: QUERY_BATCH_MAX_ITEMS, reasoningDescription: 'Reason.' });
}

function prepareMessageQueries(raw: Record<string, unknown>): PreparedMessageQuery[] | { error: string } {
  const queries = raw['queries'];
  if (!Array.isArray(queries) || queries.length === 0) return { error: 'queries must be a non-empty array.' };
  if (queries.length > QUERY_BATCH_MAX_ITEMS) return { error: `queries supports at most ${QUERY_BATCH_MAX_ITEMS} operations per call.` };
  const prepared: PreparedMessageQuery[] = [];
  for (const [index, value] of queries.entries()) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return { error: `queries[${index}] must be an object.` };
    const params = value as Params;
    const reasoning = str(params['reasoning']);
    const action = str(params['action']);
    if (!reasoning) return { error: `queries[${index}] requires non-empty reasoning.` };
    if (reasoning.length > QUERY_REASONING_MAX_LENGTH) return { error: `queries[${index}] reasoning must be at most ${QUERY_REASONING_MAX_LENGTH} characters.` };
    if (action !== 'send' && action !== 'read') return { error: `queries[${index}] has an unknown message action.` };
    if (action === 'send' && !str(params['text'])) return { error: `queries[${index}] message send requires text.` };
    if (params['files'] !== undefined && (!Array.isArray(params['files']) || params['files'].some((file) => !str(file)))) return { error: `queries[${index}] files must be an array of non-empty strings.` };
    prepared.push({ action, params });
  }
  return prepared;
}

function buildMessageTool(Type: TypeBoxBuilder): ToolDefinition {
  return {
    name: 'message',
    label: 'Message',
    description: 'Send messages or read a policy-filtered inbox; reads mark accepted messages read.',
    promptSnippet: 'Send messages or read a policy-filtered inbox',
    parameters: buildMessageParameters(Type),
    async execute(toolCallId: string, raw: Record<string, unknown>, signal: AbortSignal | undefined, onUpdate: unknown, ctx?: PiContext): Promise<ToolCallResult> {
      const prepared = prepareMessageQueries(raw);
      if (!Array.isArray(prepared)) return awarenessError(`[message] ${prepared.error}`);
      return executeQueryBatch({
        toolCallId,
        raw,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        passthroughSingle: true,
        async execute(_query, index) {
          const operation = prepared[index]!;
          const agentId = getAwarenessAgentId(ctx);
          let aw: ReturnType<typeof openPersistentAwareness> | undefined;
          try {
            aw = openPersistentAwareness({ workspace: ctx?.cwd ?? process.cwd() });
            if (operation.action === 'send') {
              const json = aw.sendMessage({
                fromAgentId: agentId,
                toAgentId: str(operation.params['to']) || null,
                topic: str(operation.params['topic']) || null,
                text: str(operation.params['text']),
                files: Array.isArray(operation.params['files']) ? operation.params['files'].map(str).filter(Boolean) : [],
              });
              return awarenessOk(str(operation.params['to']) ? `Sent message to ${str(operation.params['to'])}.` : 'Broadcast message to peers.', 'send', json);
            }
            const inbox = aw.listMessages({
              agentId,
              includeRead: operation.params['includeRead'] === true,
              topic: str(operation.params['topic']) || undefined,
              limit: typeof operation.params['limit'] === 'number' ? operation.params['limit'] : undefined,
            });
            const safe = applyPeerInboundPolicy(inbox, agentId) as Array<Record<string, unknown>>;
            if (operation.params['markRead'] !== false) {
              for (const message of safe) {
                const policy = message['inboundPolicy'] as { decision?: string } | undefined;
                if (policy?.decision === 'accept') aw.markMessageRead({ messageId: str(message['messageId']), agentId });
              }
            }
            return awarenessOk(`${safe.length} message(s) from peers.`, 'read', safe);
          } catch (error) {
            return awarenessError(`[message] ${error instanceof Error ? error.message : String(error)}`);
          } finally {
            aw?.close();
          }
        },
      });
    },
    renderCall(raw: unknown, theme?: PiTheme) {
      return buildQueryCallBlocks(raw, theme, (envelope) => {
        const query = renderedQuery(envelope);
        return renderAwarenessCall('message', str(query['action']), str(query['to']) || str(query['topic']), theme);
      });
    },
    renderResult(result: ToolCallResult, _opts: unknown, theme?: PiTheme) {
      return buildQueryResultRows('message', result, theme) ?? renderAwarenessResult('message', result, theme);
    },
  } as unknown as ToolDefinition;
}



// ─── Registry ─────────────────────────────────────────────────────────────────

/** Register the explicit Pi coordination actions: exceptional lock and message. */
export function registerAwarenessCoordinationTools(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, buildLockTool(Type));
  registerFn(pi, registeredToolNames, buildMessageTool(Type));
}
