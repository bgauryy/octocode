import type { ApprovalClass } from '@octocodeai/agent-contracts/protocols';
import { z } from 'zod';
import { contextObservationSchema, contextFeedbackSchema, contextObservationJsonSchema, contextFeedbackJsonSchema } from '../context-regulation.js';
import { knowledgeSetSchema, knowledgeGetSchema, knowledgeRevalidateSchema } from '../knowledge-contract.js';
import { experienceInputSchema } from '../experience-contract.js';
import type { KnowledgeOperation } from '../knowledge-executor.js';
import type {
  AwarenessOperationEffect,
  AwarenessOperationResult,
  CanonicalDomainHandler,
  CanonicalRouteBinding,
} from '../operation-contracts.js';
import type { AwarenessStorageScope } from '../storage-scope.js';
import { projectCommandInput } from './command-input.js';
import { compactAwarenessInputSchema } from './compact-json-schema.js';
import { schemas, type SchemaName } from './registry.js';
import type {
  AwarenessConcept,
  AwarenessOperation,
  AwarenessOperationParams,
} from './operation-types.js';
export {
  AWARENESS_CONCEPTS,
  ROUTINE_AWARENESS_OPERATIONS,
  type AwarenessConcept,
  type AwarenessOperation,
  type AwarenessOperationCall,
  type AwarenessOperationParams,
} from './operation-types.js';

type Params = Record<string, unknown>;
const BOUNDED_ROUTINE_OUTPUT_BYTES = 32 * 1024;

export interface AwarenessOperationExecutionContext {
  database?: string;
  workspace: string;
  agentId: string;
  sessionId?: string;
  scope?: AwarenessStorageScope;
  signal?: AbortSignal;
}

export interface AwarenessOperationDescriptor<K extends AwarenessOperation = AwarenessOperation> {
  operation: K;
  concept: AwarenessConcept;
  use: string;
  visibility: 'routine';
  effects: readonly AwarenessOperationEffect[];
  inputSchema: Readonly<Record<string, unknown>>;
  /** Exact compact serialization of inputSchema for prompt/tool transports. */
  inputSchemaText: string;
  validate(params?: unknown): AwarenessOperationParams[K];
  effect(params?: AwarenessOperationParams[K]): AwarenessOperationEffect;
  approval(params?: AwarenessOperationParams[K]): ApprovalClass | undefined;
  outputBudget: number;
  continuations(payload: unknown): unknown;
  handler(context: AwarenessOperationExecutionContext, params?: AwarenessOperationParams[K]): Promise<AwarenessOperationResult>;
}

interface Route extends Omit<CanonicalRouteBinding, 'schema'> {
  schemaName: SchemaName;
  selector?: readonly [key: string, value: string];
  default?: boolean;
}
export const AWARENESS_HOST_PARAMETER_NAMES = Object.freeze([
  'db',
  'database',
  'workspace',
  'agent_id',
  'lead_agent_id',
  'session_id',
  'compact',
] as const);
const HOST_FIELDS = new Set<string>(AWARENESS_HOST_PARAMETER_NAMES);
const record = (value: unknown): Record<string, unknown> | undefined =>
  value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined;

function stripHostBindings(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stripHostBindings);
  const object = record(value);
  if (!object) return value;
  const output = Object.fromEntries(Object.entries(object).map(([key, child]) => [key, stripHostBindings(child)]));
  const properties = record(output['properties']);
  if (properties) for (const field of HOST_FIELDS) delete properties[field];
  if (Array.isArray(output['required'])) {
    const required = output['required'].filter(field => typeof field === 'string' && !HOST_FIELDS.has(field));
    if (required.length) output['required'] = required;
    else delete output['required'];
  }
  for (const key of Object.keys(output)) if (key.startsWith('x-')) delete output[key];
  return output;
}

function routeSchema(route: Route): Record<string, unknown> {
  const schema = stripHostBindings(structuredClone(projectCommandInput(route.command, schemas[route.schemaName]))) as Record<string, unknown>;
  if (!route.selector) return schema;
  const [key, value] = route.selector;
  const properties = record(schema['properties']) ?? {};
  properties[key] = { type: 'string', const: value };
  schema['properties'] = properties;
  const required = new Set(Array.isArray(schema['required']) ? schema['required'].filter(item => typeof item === 'string') as string[] : []);
  if (!route.default) required.add(key);
  if (required.size) schema['required'] = [...required];
  return schema;
}

function selectRoute(operation: AwarenessOperation, routes: readonly Route[], params: Params): Route {
  if (routes.length === 1 && !routes[0]!.selector) return routes[0]!;
  const key = routes.find(route => route.selector)?.selector?.[0];
  const selected = routes.find(route => route.selector?.[1] === String(params[key ?? ''] ?? ''))
    ?? routes.find(route => route.default && params[key ?? ''] === undefined);
  if (selected) return selected;
  throw new Error(`${operation} ${key} must be one of: ${routes.map(route => route.selector?.[1]).filter(Boolean).join(', ')}`);
}

function paramsForRoute(route: Route, params: Params): Params {
  if (!route.selector) return { ...params };
  const { [route.selector[0]]: _selector, ...rest } = params;
  return rest;
}

const reverseRoutes = new Map<string, { operation: AwarenessOperation; selector?: readonly [string, string] }>();
function canonicalizeContinuation(value: unknown, withinContinuation = false): unknown {
  if (Array.isArray(value)) return value.map(child => canonicalizeContinuation(child, withinContinuation));
  const object = record(value);
  if (!object) return value;
  if (withinContinuation && typeof object['command'] === 'string') {
    const route = reverseRoutes.get(object['command']);
    if (route) {
      const params = { ...(record(object['params']) ?? record(object['args']) ?? {}) };
      for (const field of HOST_FIELDS) delete params[field];
      if (route.selector) params[route.selector[0]] = route.selector[1];
      return { operation: route.operation, ...(Object.keys(params).length ? { params } : {}) };
    }
  }
  return Object.fromEntries(Object.entries(object).map(([key, child]) => {
    const descend = withinContinuation || ['next', 'actions', 'continuations', 'undo_preview'].includes(key);
    return [key, descend ? canonicalizeContinuation(child, true) : child];
  }));
}

interface DescriptorInput<K extends AwarenessOperation> {
  operation: K;
  use: string;
  routes: readonly Route[];
  inputSchema?: Record<string, unknown>;
  outputBudget?: number;
}
function descriptor<K extends AwarenessOperation>(input: DescriptorInput<K>): AwarenessOperationDescriptor<K> {
  const routeSchemas = input.routes.map(routeSchema);
  const sourceSchema = input.inputSchema ?? (routeSchemas.length === 1
    ? routeSchemas[0]!
    : { oneOf: routeSchemas });
  const inputSchema = compactAwarenessInputSchema(sourceSchema);
  const validator = z.fromJSONSchema(inputSchema);
  const inputSchemaText = JSON.stringify(inputSchema);
  for (const route of input.routes) reverseRoutes.set(route.command, { operation: input.operation, ...(route.selector ? { selector: route.selector } : {}) });
  const effects = [...new Set(input.routes.map(route => route.effect))];
  const resolve = (value?: unknown) => {
    const params = record(value) ?? {};
    const route = selectRoute(input.operation, input.routes, params);
    const validation = validator.safeParse(params);
    if (!validation.success) {
      const issues = validation.error.issues.map(issue => ({
        path: issue.path.join('.') || '<root>', code: issue.code, message: issue.message,
      }));
      throw Object.assign(
        new Error(`${input.operation} invalid parameters: ${validation.error.issues.map(issue => `${issue.path.join('.') || '/'} ${issue.message}`).join('; ')}`),
        { issues },
      );
    }
    return { route, params: validation.data as Params, commandParams: paramsForRoute(route, validation.data as Params) };
  };
  return Object.freeze({
    operation: input.operation,
    concept: input.operation.split('.')[0] as AwarenessConcept,
    use: input.use,
    visibility: 'routine' as const,
    effects: Object.freeze(effects),
    inputSchema: Object.freeze(inputSchema),
    inputSchemaText,
    validate: (params?: unknown) => resolve(params).params as AwarenessOperationParams[K],
    effect: (params?: AwarenessOperationParams[K]) => resolve(params).route.effect,
    approval: (params?: AwarenessOperationParams[K]) => resolve(params).route.approval,
    outputBudget: input.outputBudget ?? 12_000,
    continuations: (payload: unknown) => canonicalizeContinuation(payload),
    handler: async (context: AwarenessOperationExecutionContext, params?: AwarenessOperationParams[K]) => {
      const selected = resolve(params);
      if (input.operation === 'context.orient') {
        const { executeContextOrient } = await import('../context-regulation-executor.js');
        return executeContextOrient(context, selected.params as AwarenessOperationParams['context.orient']);
      }
      const { executeCanonicalRoute } = await import('../operation-executor.js');
      return executeCanonicalRoute(input.operation, {
          command: selected.route.command,
          schema: projectCommandInput(selected.route.command, schemas[selected.route.schemaName]),
          handler: selected.route.handler,
          ...(selected.route.action ? { action: selected.route.action } : {}),
          effect: selected.route.effect,
          ...(selected.route.approval ? { approval: selected.route.approval } : {}),
        }, selected.commandParams, context);
    },
  });
}

const route = (
  command: string, schemaName: SchemaName, handler: CanonicalDomainHandler,
  effect: AwarenessOperationEffect, action?: string, approval?: ApprovalClass,
): Route => ({ command, schemaName, handler, effect, ...(action ? { action } : {}), ...(approval ? { approval } : {}) });
const choice = (
  key: string,
  values: Readonly<Record<string, Route>>,
  defaultValue?: string,
): Route[] => Object.entries(values).map(([value, binding]) => ({
  ...binding, selector: [key, value] as const, ...(value === defaultValue ? { default: true } : {}),
}));

function contextWriteDescriptor<K extends 'context.observe' | 'context.feedback'>(
  operation: K, use: string, schema: z.ZodType, inputSchema: Record<string, unknown>,
): AwarenessOperationDescriptor<K> {
  const validate = (params?: unknown) => schema.parse(params) as AwarenessOperationParams[K];
  const compactInputSchema = compactAwarenessInputSchema(inputSchema);
  const inputSchemaText = JSON.stringify(compactInputSchema);
  return Object.freeze({
    operation, concept: 'context', use, visibility: 'routine',
    effects: ['coordination-write'] as const,
    inputSchema: Object.freeze(compactInputSchema),
    inputSchemaText,
    validate, effect: () => 'coordination-write' as const, approval: () => undefined,
    outputBudget: 1_500, continuations: canonicalizeContinuation,
    handler: async (context: AwarenessOperationExecutionContext, params?: AwarenessOperationParams[K]) => {
      const { executeContextReport } = await import('../context-regulation-executor.js');
      return executeContextReport(operation, context, validate(params));
    },
  });
}

function knowledgeDescriptor(
  operation: KnowledgeOperation, use: string, schema: z.ZodType,
): AwarenessOperationDescriptor {
  const validate = (params?: unknown) => schema.parse(params ?? {}) as Params;
  const inputSchema = compactAwarenessInputSchema(
    z.toJSONSchema(schema, { io: 'input' }) as Record<string, unknown>,
  );
  const effect = (params?: Params): AwarenessOperationEffect => {
    const input = validate(params);
    return operation === 'memory.set' || (operation === 'history.experience' && ['record', 'seal'].includes(String(input.action)))
      ? 'coordination-write' : 'read';
  };
  return Object.freeze({
    operation, concept: operation.startsWith('memory.') ? 'memory' : 'history', use, visibility: 'routine',
    effects: operation === 'history.experience' ? ['read', 'coordination-write'] as const
      : operation === 'memory.set' ? ['coordination-write'] as const : ['read'] as const,
    inputSchema: Object.freeze(inputSchema),
    inputSchemaText: JSON.stringify(inputSchema),
    validate, effect, approval: () => undefined,
    outputBudget: operation === 'history.experience' ? BOUNDED_ROUTINE_OUTPUT_BYTES : 32_000,
    continuations: canonicalizeContinuation,
    handler: async (context: AwarenessOperationExecutionContext, params?: Params) => {
      const { executeKnowledgeOperation } = await import('../knowledge-executor.js');
      return executeKnowledgeOperation(operation, context, validate(params));
    },
  });
}

const contextOrientInputSchema = z.strictObject({
  if_revision: z.string().min(1).max(80).optional(),
  limit: z.number().int().min(1).max(3).optional(),
  offset: z.number().int().min(0).optional(),
  file: z.union([
    z.string().min(1).max(1024),
    z.array(z.string().min(1).max(1024)).min(1).max(20),
  ]).optional(),
  query: z.string().min(1).max(500).optional(),
  flow: z.string().min(1).max(400).optional(),
  failure_signature: z.string().min(1).max(400).optional(),
});

const operationDescriptors = Object.freeze([
  descriptor({
    operation: 'context.orient',
    use: 'Read a bounded interpreted snapshot when changed context could alter the next action; pass if_revision to suppress unchanged output.',
    routes: [route('attend', 'attend', 'query', 'read')],
    inputSchema: z.toJSONSchema(contextOrientInputSchema, { io: 'input' }) as Record<string, unknown>,
    outputBudget: 6_000,
  }),
  contextWriteDescriptor('context.observe', 'Record one measured state change; use passive only for lifecycle observations and reuse observation_id on retry.', contextObservationSchema, contextObservationJsonSchema),
  contextWriteDescriptor('context.feedback', 'Link an advisory to the action actually taken and its outcome; helpful claims remain provisional until later evidence.', contextFeedbackSchema, contextFeedbackJsonSchema),
  descriptor({ operation: 'work.create', use: 'Create kind=plan, task, or standalone; use standalone only for file work outside a durable plan.', routes: choice('kind', {
    plan: route('plan create', 'plan', 'plan', 'coordination-write', 'create'),
    task: route('task create', 'task', 'task', 'coordination-write', 'create'),
    standalone: route('work start', 'work', 'work', 'coordination-write', 'start'),
  }) }),
  descriptor({ operation: 'work.list', use: 'List one scoped view selected by kind: plans, tasks, ready work, active presence, agents, or workboard.', routes: choice('kind', {
    plan: route('plan list', 'plan', 'plan', 'read', 'list'), task: route('task list', 'task', 'task', 'read', 'list'),
    ready: route('task ready', 'task', 'task', 'read', 'ready'), presence: route('work list', 'work', 'work', 'read', 'list'),
    agents: route('query agents', 'agents', 'query', 'read', 'agents'),
    workboard: route('query workboard', 'query', 'query', 'read', 'workboard'),
  }, 'presence') }),
  descriptor({ operation: 'work.show', use: 'Inspect one plan, task, or active file-presence record selected by kind.', routes: choice('kind', {
    plan: route('plan show', 'plan', 'plan', 'read', 'show'), task: route('task show', 'task', 'task', 'read', 'show'),
    presence: route('work show', 'work', 'work', 'read', 'show'),
  }) }),
  descriptor({ operation: 'work.claim', use: 'Atomically claim an exact task, or the next ready task in a plan, and start its attempt.', routes: [route('task claim', 'task', 'task', 'coordination-write', 'claim')] }),
  descriptor({ operation: 'work.update', use: 'Apply one explicit transition to a task, plan, or presence record; heartbeat/touch only refresh leases.', routes: choice('transition', {
    heartbeat: route('task heartbeat', 'task', 'task', 'coordination-write', 'heartbeat'), submit: route('task submit', 'task', 'task', 'coordination-write', 'submit'),
    release: route('task release', 'task', 'task', 'coordination-write', 'release'), retry: route('task retry', 'task', 'task', 'coordination-write', 'retry'),
    touch: route('work touch', 'work', 'work', 'coordination-write', 'touch'), end: route('work end', 'work', 'work', 'coordination-write', 'end'),
    join: route('plan join', 'plan', 'plan', 'coordination-write', 'join'), document: route('plan doc', 'plan', 'plan', 'coordination-write', 'doc'),
    status: route('plan status', 'plan', 'plan', 'coordination-write', 'status'),
  }) }),
  descriptor({ operation: 'work.depend', use: 'Add declared prerequisite task IDs to one existing task.', routes: [route('task depend', 'task', 'task', 'coordination-write', 'depend')] }),
  descriptor({ operation: 'work.protect', use: 'Acquire, wait for, or release exceptional exclusive file protection; routine ownership uses work.create.', routes: choice('action', {
    acquire: route('lock acquire', 'lock_acquire', 'lock-acquire', 'coordination-write'),
    wait: route('lock wait', 'lock_wait', 'lock-wait', 'read'), release: route('lock release', 'lock_release', 'lock-release', 'coordination-write'),
  }) }),
  descriptor({ operation: 'work.verify', use: 'Audit pending verification debt, or mark only runs covered by an observed check receipt.', routes: choice('action', {
    audit: route('verify audit', 'verify_audit', 'verify-audit', 'read'), mark: route('verify mark', 'verify', 'verify', 'coordination-write'),
  }) }),
  descriptor({
    operation: 'message.list', use: 'Read bounded decision-changing messages; follow the returned cursor and request bodies only when needed.',
    routes: [route('signal list', 'agent_signal', 'signal', 'read', 'list')],
    outputBudget: BOUNDED_ROUTINE_OUTPUT_BYTES,
  }),
  descriptor({ operation: 'message.send', use: 'Send one typed, decision-changing message to exact actor IDs with optional file and reference scope.', routes: [route('signal publish', 'agent_signal', 'signal', 'coordination-write', 'publish')] }),
  descriptor({ operation: 'message.reply', use: 'Reply to the exact signal ID in an existing thread and provide a new subject.', routes: [route('signal reply', 'agent_signal', 'signal', 'coordination-write', 'reply')] }),
  descriptor({ operation: 'message.resolve', use: 'Resolve one handled signal or thread only after no response or work remains.', routes: [route('signal resolve', 'agent_signal', 'signal', 'coordination-write', 'resolve')] }),
  descriptor({ operation: 'memory.recall', use: 'Search legacy scoped memories by evidence, label, file, or failure signal with bounded results.', routes: [route('memory recall', 'memory_recall', 'memory-recall', 'read')] }),
  descriptor({ operation: 'memory.record', use: 'Record one legacy evidence-linked memory when the learning is reusable beyond the current run.', routes: [route('memory record', 'memory_record', 'memory-record', 'coordination-write')] }),
  knowledgeDescriptor('memory.set', 'Create or compare-and-set a keyed, attributed lesson with typed anchors and explicit applicability.', knowledgeSetSchema),
  knowledgeDescriptor('memory.get', 'Read an exact lesson revision or discover scoped path, flow, and failure knowledge; follow snapshot continuations.', knowledgeGetSchema),
  knowledgeDescriptor('memory.revalidate', 'Check declared evidence freshness for lessons; unchanged bytes do not verify the lesson claim.', knowledgeRevalidateSchema),
  descriptor({ operation: 'history.status', use: 'Inspect whether LocalGit recovery is available and durable for this workspace.', routes: [route('history status', 'history_status', 'history', 'read', 'history_status')] }),
  descriptor({ operation: 'history.timeline', use: 'List a bounded page of recoverable history for one file and follow its cursor.', routes: [route('history timeline', 'history_timeline', 'history', 'read', 'history_timeline')] }),
  descriptor({ operation: 'history.read', use: 'Read one bounded before/after version from an exact recoverable operation.', routes: [route('history read', 'history_read', 'history', 'read', 'history_read')] }),
  descriptor({ operation: 'history.restore', use: 'Preview a bound restore first; apply only the unchanged preview after required approval.', routes: choice('action', {
    preview: route('history restore-preview', 'history_restore_preview', 'history', 'read', 'history_restore_preview'),
    apply: route('history restore-apply', 'history_restore_apply', 'history', 'workspace-write', 'history_restore_apply', 'fs-delete'),
  }, 'preview') }),
  knowledgeDescriptor('history.experience', 'Record meaningful attempts or decisions, inspect and compare traces, archive selected evidence, or recover unfinished work.', experienceInputSchema),
] as const);

if (operationDescriptors.length > 25) throw new Error('Routine Awareness surface exceeds the twenty-five-operation budget');
const byOperation = new Map<AwarenessOperation, AwarenessOperationDescriptor>(operationDescriptors.map(row => [row.operation, row]));
export function listAwarenessOperationDescriptors(): readonly AwarenessOperationDescriptor[] { return operationDescriptors; }
export function getAwarenessOperationDescriptor(operation: string): AwarenessOperationDescriptor | undefined { return byOperation.get(operation as AwarenessOperation); }
