/**
 * First-class Pi coordination tools over the in-process Awareness library.
 * The model-facing surface is intentionally narrow: exceptional exclusive locks
 * and peer messages. Shared state is signalled automatically; plans, tasks,
 * verification, and work presence belong to unified plan/mutation flows.
 */

import {
  AWARENESS_COMMANDS,
  type CommandGroup,
  type CommandParam,
  type AwarenessCommandRequest,
  evaluatePeerInbound,
} from '@octocodeai/octocode-awareness';
import type { ToolDefinition, ToolCallResult, PiTheme, PiContext } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { buildQueryCallBlocks, buildQueryResultRows } from './render-helpers.js';
import {
  buildQueryEnvelopeSchema,
  executeQueryBatch,
  QUERY_BATCH_MAX_ITEMS,
  QUERY_REASONING_MAX_LENGTH,
} from './query-envelope.js';
import {
  getAwarenessAgentId,
  runAwarenessCommand,
  awarenessError,
  awarenessOk,
  renderAwarenessCall,
  renderAwarenessResult,
  countRows,
} from './awareness-shared.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type TSchema = import('typebox').TSchema;
type RegisterFn = typeof registerUniqueTool;
type Params = Record<string, unknown>;

const str = (v: unknown): string => (v === undefined || v === null ? '' : String(v)).trim();
const has = (v: unknown): boolean => str(v).length > 0;
const renderedQuery = (value: Params): Params => Array.isArray(value['queries'])
  ? (value['queries'][0] as Params | undefined) ?? {}
  : value;

// ─── Schema generation (best-practice discriminated union by `action`) ─────────

function paramSchema(Type: TypeBoxBuilder, param: CommandParam): TSchema {
  const base = { description: param.description };
  if (param.enum) return Type.Unsafe({ type: 'string', enum: [...param.enum], ...base });
  if (param.type === 'integer') {
    return Type.Integer({
      ...(param.min !== undefined ? { minimum: param.min } : {}),
      ...(param.max !== undefined ? { maximum: param.max } : {}),
      ...base,
    });
  }
  if (param.type === 'boolean') return Type.Boolean(base);
  if (param.type === 'string[]') return Type.Array(Type.String(), base);
  return Type.String(base);
}

const toolParamName = (param: CommandParam): string => param.name === 'reason' ? 'reasoning' : param.name;

function buildQuerySchema(Type: TypeBoxBuilder, group: CommandGroup): TSchema {
  const props: Record<string, TSchema> = {
    reasoning: Type.String({ minLength: 1, maxLength: QUERY_REASONING_MAX_LENGTH, description: 'Why this operation is necessary. For lock/work declarations, this is also stored as the ledger reason.' }),
  };
  if (!group.singleton) {
    props['action'] = Type.Unsafe({ type: 'string', enum: group.actions.map((a) => a.action), description: `${group.resource} action` });
  }
  const seen = new Set(Object.keys(props));
  for (const action of group.actions) {
    for (const param of action.params) {
      const name = toolParamName(param);
      if (seen.has(name)) continue;
      seen.add(name);
      props[name] = Type.Optional(paramSchema(Type, param));
    }
  }
  const options: Record<string, unknown> = { additionalProperties: false };
  if (!group.singleton) {
    // Discriminate on `action`: each variant declares exactly its required fields
    // so the model's tool schema is self-documenting (not "all fields optional").
    options['oneOf'] = group.actions.map((action) => ({
      title: action.action,
      properties: { action: { const: action.action } },
      required: ['action', 'reasoning', ...action.params.filter((p) => p.required).map(toolParamName)],
    }));
  }
  return Type.Object(props, options);
}

function buildParameters(Type: TypeBoxBuilder, group: CommandGroup): TSchema {
  return buildQueryEnvelopeSchema(Type, buildQuerySchema(Type, group), {
    maxItems: QUERY_BATCH_MAX_ITEMS,
  });
}

// ─── Request generation (from the same descriptor) ─────────────────────────────
// Build the STRUCTURED dispatcher request straight from the command descriptor —
// no CLI arg-vector, no round-trip through the parser. `dispatchAwarenessCommand`
// (shared with the CLI) owns the command→library mapping, so this only maps the
// tool-facing values onto the descriptor's canonical param names. The host agent
// id is passed as `agentId`; the dispatcher routes it (e.g. to `fromAgentId` for
// `message send`), so `agentIdFlag` is no longer the host's concern.

function coerceParam(param: CommandParam, value: unknown): unknown {
  if (param.type === 'string[]') return Array.isArray(value) ? value.map(str).filter(Boolean) : str(value);
  if (param.type === 'integer' || param.durationMs) return Number(value);
  return str(value);
}

function buildRequest(group: CommandGroup, p: Params, agentId: string): AwarenessCommandRequest | { error: string } {
  const action = group.singleton ? group.actions[0]!.action : str(p['action']);
  const spec = group.actions.find((a) => a.action === action);
  if (!spec) return { error: `unknown ${group.resource} action "${action || '(none)'}".` };
  const params: Params = {};
  if (spec.needsAgentId) params['agentId'] = agentId;
  for (const param of spec.params) {
    const v = p[toolParamName(param)];
    if (param.required && !has(v)) return { error: `${group.resource} ${spec.action} requires ${toolParamName(param)}.` };
    if (param.type === 'boolean') { if (v === true) params[param.name] = true; continue; }
    if (!has(v)) continue;
    params[param.name] = coerceParam(param, v);
  }
  return { command: group.cli, action: spec.action, params };
}

interface PreparedQuery {
  action: string;
  params: Params;
  request: AwarenessCommandRequest;
}

function prepareQueries(
  group: CommandGroup,
  raw: Record<string, unknown>,
  agentId: string,
): PreparedQuery[] | { error: string } {
  const queries = raw['queries'];
  if (!Array.isArray(queries) || queries.length === 0) return { error: 'queries must be a non-empty array.' };
  if (queries.length > QUERY_BATCH_MAX_ITEMS) return { error: `queries supports at most ${QUERY_BATCH_MAX_ITEMS} operations per call.` };

  const prepared: PreparedQuery[] = [];
  for (const [index, value] of queries.entries()) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
      return { error: `queries[${index}] must be an object.` };
    }
    const params = value as Params;
    const reasoning = str(params['reasoning']);
    if (!reasoning) return { error: `queries[${index}] requires non-empty reasoning.` };
    if (reasoning.length > QUERY_REASONING_MAX_LENGTH)
      return { error: `queries[${index}] reasoning must be at most ${QUERY_REASONING_MAX_LENGTH} characters.` };
    const built = buildRequest(group, params, agentId);
    if ('error' in built) return { error: `queries[${index}] ${built.error}` };
    prepared.push({
      action: group.singleton ? group.actions[0]!.action : str(params['action']),
      params,
      request: built,
    });
  }
  return prepared;
}

// ─── Exceptional explicit lock wrapper ────────────────────────────────────────

function buildLockParameters(Type: TypeBoxBuilder): TSchema {
  const withFile = (title: string, actionConst: string) => ({ title, properties: { action: { const: actionConst } }, required: ['reasoning', 'action', 'file'] });
  const itemSchema = Type.Object({
    reasoning: Type.String({ minLength: 1, maxLength: QUERY_REASONING_MAX_LENGTH, description: 'Why this lock operation is necessary.' }),
    action: Type.Unsafe({ type: 'string', enum: ['acquire', 'release', 'wait'], description: 'Exceptional lock action.' }),
    file: Type.Optional(Type.String({ description: 'Workspace-relative file path.' })),
    ttlSeconds: Type.Optional(Type.Integer({ minimum: 1, maximum: 3600, description: 'Lease seconds (default 1800).' })),
    waitMs: Type.Optional(Type.Integer({ minimum: 0, maximum: 60000, description: 'Max ms to wait for a peer-held lock.' })),
  }, { additionalProperties: false, oneOf: [withFile('acquire', 'acquire'), withFile('release', 'release'), withFile('wait', 'wait')] });
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
  const group = AWARENESS_COMMANDS.find((candidate) => candidate.resource === 'lock');
  if (!group) throw new Error('Awareness lock command descriptor is unavailable');
  return {
    name: 'lock', label: 'Lock',
    description: ['Exceptional exclusive file lock for non-mergeable work: single-writer configs, migration scripts, shared counters, or files where concurrent edits cannot be merged.', 'Mutation-time conflict checks are automatic — do not lock for ordinary mergeable edits.', 'On peer conflict: inspect the holder (message inbox); use waitMs to wait briefly; release your lock when done (always release).', 'Actions: acquire, wait (blocks until free or waitMs exceeded), release.'].join('\n'),
    promptSnippet: 'Exceptional exclusive locks; mutation-time conflict checks are automatic', parameters: buildLockParameters(Type),
    async execute(toolCallId: string, raw: Record<string, unknown>, signal: AbortSignal | undefined, onUpdate: unknown, ctx?: PiContext): Promise<ToolCallResult> {
      const prepared = prepareQueries(group, raw, getAwarenessAgentId(ctx));
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
          const response = runAwarenessCommand(operation.request, ctx?.cwd ?? process.cwd());
          if (!response.ok) return awarenessError(`[lock] ${response.error ?? 'unknown error'}`);
          return awarenessOk(summarizeLock(operation.action, response.json, operation.params), operation.action, response.json);
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

// ─── Human summaries (per resource; falls back to the action summary) ──────────

type Summarize = (action: string, json: unknown, p: Params) => string;

const SUMMARIES: Record<string, Summarize> = {
  message: (action, json, p) => {
    if (action === 'read') return `${countRows(json)} message(s) from peers.`;
    return str(p['to']) ? `Sent message to ${str(p['to'])}.` : 'Broadcast message to peers.';
  },

};

const HINT_FIELDS = ['to', 'topic', 'messageId'] as const;

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

function makeTool(group: CommandGroup, Type: TypeBoxBuilder): ToolDefinition {
  const operations = group.singleton
    ? group.actions[0]!.summary
    : group.actions.map((a) => `${a.action} — ${a.summary}`).join('; ');
  const description = [
    group.summary,
    `${operations}. Pass one or more operations in queries; each query requires reasoning. The batch is validated before ordered in-process execution.`,
  ].join('\n');
  const promptSnippet = group.summary.split(/[.—]/)[0]!.trim();
  const summarize: Summarize = SUMMARIES[group.resource] ?? ((action) => `${group.resource} ${action} ok.`);

  return {
    name: group.resource,
    label: group.label,
    description,
    promptSnippet,
    parameters: buildParameters(Type, group),
    async execute(toolCallId: string, raw: Record<string, unknown>, signal: AbortSignal | undefined, onUpdate: unknown, ctx?: PiContext): Promise<ToolCallResult> {
      const cwd = ctx?.cwd ?? process.cwd();
      const prepared = prepareQueries(group, raw, getAwarenessAgentId(ctx));
      if (!Array.isArray(prepared)) return awarenessError(`[${group.resource}] ${prepared.error}`);

      return executeQueryBatch({
        toolCallId,
        raw,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        passthroughSingle: true,
        async execute(_query, index) {
          const operation = prepared[index]!;
          const response = runAwarenessCommand(operation.request, cwd);
          if (!response.ok) return awarenessError(`[${group.resource}] ${response.error ?? 'unknown error'}`);
          const safeJson = group.resource === 'message' && operation.action === 'read'
            ? applyPeerInboundPolicy(response.json, getAwarenessAgentId(ctx))
            : response.json;
          return awarenessOk(
            summarize(operation.action, safeJson, operation.params),
            operation.action,
            safeJson,
          );
        },
      });
    },
    renderCall(raw: unknown, theme?: PiTheme) {
      return buildQueryCallBlocks(raw, theme, (envelope) => {
        const query = renderedQuery(envelope);
        const action = group.singleton ? group.actions[0]!.action : str(query['action']);
        const value = HINT_FIELDS.map((field) => str(query[field])).find(Boolean) ?? '';
        return renderAwarenessCall(group.resource, action, value, theme);
      });
    },
    renderResult(result: ToolCallResult, _opts: unknown, theme?: PiTheme) {
      return buildQueryResultRows(group.resource, result, theme)
        ?? renderAwarenessResult(group.resource, result, theme);
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
  for (const group of AWARENESS_COMMANDS) {
    if (group.resource !== 'message') continue;
    registerFn(pi, registeredToolNames, makeTool(group, Type));
  }
}
