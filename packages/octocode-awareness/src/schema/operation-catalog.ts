import type { ApprovalClass } from '@octocodeai/agent-contracts/protocols';
import { z } from 'zod';
import type {
  AwarenessOperationEffect,
  AwarenessOperationResult,
  CanonicalDomainHandler,
  CanonicalRouteBinding,
} from '../operation-contracts.js';
import type { AwarenessStorageScope } from '../storage-scope.js';
import { projectCommandInput } from './command-input.js';
import { schemas, type SchemaName } from './registry.js';

export const AWARENESS_CONCEPTS = Object.freeze(['context', 'work', 'message', 'memory', 'history'] as const);
export type AwarenessConcept = typeof AWARENESS_CONCEPTS[number];

export const ROUTINE_AWARENESS_OPERATIONS = Object.freeze([
  'context.orient',
  'work.create', 'work.list', 'work.show', 'work.claim', 'work.update', 'work.depend', 'work.protect', 'work.verify',
  'message.list', 'message.send', 'message.reply', 'message.resolve',
  'memory.recall', 'memory.record',
  'history.status', 'history.timeline', 'history.read', 'history.restore',
] as const);
export type AwarenessOperation = typeof ROUTINE_AWARENESS_OPERATIONS[number];
export type AwarenessOperationVisibility = 'routine' | 'operator' | 'recovery';

type Params = Record<string, unknown>;
type KindParams<K extends string> = Params & { kind: K };
type ActionParams<A extends string> = Params & { action: A };
type TransitionParams<T extends string> = Params & { transition: T };

export interface AwarenessOperationParams {
  'context.orient': { if_revision?: string; limit?: number; offset?: number; file?: string | string[]; query?: string };
  'work.create': KindParams<'plan' | 'task' | 'standalone'>;
  'work.list': Params & { kind?: 'plan' | 'task' | 'ready' | 'presence' | 'workboard' };
  'work.show': KindParams<'plan' | 'task' | 'presence'>;
  'work.claim': Params;
  'work.update': TransitionParams<'heartbeat' | 'submit' | 'release' | 'retry' | 'touch' | 'end' | 'join' | 'document' | 'status'>;
  'work.depend': Params;
  'work.protect': ActionParams<'acquire' | 'wait' | 'release'>;
  'work.verify': ActionParams<'audit' | 'mark'>;
  'message.list': Params;
  'message.send': Params;
  'message.reply': Params;
  'message.resolve': Params;
  'memory.recall': Params;
  'memory.record': Params;
  'history.status': Params;
  'history.timeline': Params;
  'history.read': Params;
  'history.restore': Params & { action?: 'preview' | 'apply' };
}

export type AwarenessOperationCall<K extends AwarenessOperation = AwarenessOperation> = K extends AwarenessOperation
  ? { operation: K; params?: AwarenessOperationParams[K] }
  : never;

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
const HOST_FIELDS = new Set(['db', 'database', 'workspace', 'agent_id', 'lead_agent_id', 'session_id', 'compact']);
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
  const inputSchema = input.inputSchema ?? (routeSchemas.length === 1
    ? routeSchemas[0]!
    : { $schema: 'https://json-schema.org/draft/2020-12/schema', oneOf: routeSchemas });
  const validator = z.fromJSONSchema(inputSchema);
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
    validate: (params?: unknown) => resolve(params).params as AwarenessOperationParams[K],
    effect: (params?: AwarenessOperationParams[K]) => resolve(params).route.effect,
    approval: (params?: AwarenessOperationParams[K]) => resolve(params).route.approval,
    outputBudget: input.outputBudget ?? 12_000,
    continuations: (payload: unknown) => canonicalizeContinuation(payload),
    handler: async (context: AwarenessOperationExecutionContext, params?: AwarenessOperationParams[K]) => {
      const selected = resolve(params);
      const executor = await import('../operation-executor.js');
      return input.operation === 'context.orient'
        ? executor.executeContextOrient(context, selected.params as AwarenessOperationParams['context.orient'])
        : executor.executeCanonicalRoute({
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

const operationDescriptors = Object.freeze([
  descriptor({
    operation: 'context.orient',
    use: 'Read bounded decision-changing context; reuse the revision on the next call.',
    routes: [route('attend', 'attend', 'query', 'read')],
    inputSchema: {
      $schema: 'https://json-schema.org/draft/2020-12/schema',
      type: 'object',
      properties: {
        if_revision: { type: 'string', minLength: 1, maxLength: 80 },
        limit: { type: 'integer', minimum: 1, maximum: 3 },
        offset: { type: 'integer', minimum: 0 },
        file: { oneOf: [
          { type: 'string', minLength: 1, maxLength: 1024 },
          { type: 'array', minItems: 1, maxItems: 20, items: { type: 'string', minLength: 1, maxLength: 1024 } },
        ] },
        query: { type: 'string', minLength: 1, maxLength: 500 },
      },
      additionalProperties: false,
    },
    outputBudget: 1_500,
  }),
  descriptor({ operation: 'work.create', use: 'Create a plan, task, or standalone work declaration.', routes: choice('kind', {
    plan: route('plan create', 'plan', 'plan', 'coordination-write', 'create'),
    task: route('task create', 'task', 'task', 'coordination-write', 'create'),
    standalone: route('work start', 'work', 'work', 'coordination-write', 'start'),
  }) }),
  descriptor({ operation: 'work.list', use: 'List scoped plans, tasks, ready work, active presence, or the workboard.', routes: choice('kind', {
    plan: route('plan list', 'plan', 'plan', 'read', 'list'), task: route('task list', 'task', 'task', 'read', 'list'),
    ready: route('task ready', 'task', 'task', 'read', 'ready'), presence: route('work list', 'work', 'work', 'read', 'list'),
    workboard: route('query workboard', 'query', 'query', 'read', 'workboard'),
  }, 'presence') }),
  descriptor({ operation: 'work.show', use: 'Inspect one plan, task, or active file.', routes: choice('kind', {
    plan: route('plan show', 'plan', 'plan', 'read', 'show'), task: route('task show', 'task', 'task', 'read', 'show'),
    presence: route('work show', 'work', 'work', 'read', 'show'),
  }) }),
  descriptor({ operation: 'work.claim', use: 'Atomically claim a task and start its attempt.', routes: [route('task claim', 'task', 'task', 'coordination-write', 'claim')] }),
  descriptor({ operation: 'work.update', use: 'Transition an existing work item or refresh its lease.', routes: choice('transition', {
    heartbeat: route('task heartbeat', 'task', 'task', 'coordination-write', 'heartbeat'), submit: route('task submit', 'task', 'task', 'coordination-write', 'submit'),
    release: route('task release', 'task', 'task', 'coordination-write', 'release'), retry: route('task retry', 'task', 'task', 'coordination-write', 'retry'),
    touch: route('work touch', 'work', 'work', 'coordination-write', 'touch'), end: route('work end', 'work', 'work', 'coordination-write', 'end'),
    join: route('plan join', 'plan', 'plan', 'coordination-write', 'join'), document: route('plan doc', 'plan', 'plan', 'coordination-write', 'doc'),
    status: route('plan status', 'plan', 'plan', 'coordination-write', 'status'),
  }) }),
  descriptor({ operation: 'work.depend', use: 'Add a dependency between work items.', routes: [route('task depend', 'task', 'task', 'coordination-write', 'depend')] }),
  descriptor({ operation: 'work.protect', use: 'Acquire, wait for, or release exceptional exclusive protection.', routes: choice('action', {
    acquire: route('lock acquire', 'lock_acquire', 'lock-acquire', 'coordination-write'),
    wait: route('lock wait', 'lock_wait', 'lock-wait', 'read'), release: route('lock release', 'lock_release', 'lock-release', 'coordination-write'),
  }) }),
  descriptor({ operation: 'work.verify', use: 'Audit verification debt or record an observed check result.', routes: choice('action', {
    audit: route('verify audit', 'verify_audit', 'verify-audit', 'read'), mark: route('verify mark', 'verify', 'verify', 'coordination-write'),
  }) }),
  descriptor({
    operation: 'message.list', use: 'Read decision-changing messages.',
    routes: [route('signal list', 'agent_signal', 'signal', 'read', 'list')],
    outputBudget: 256_000,
  }),
  descriptor({ operation: 'message.send', use: 'Send a decision-changing message.', routes: [route('signal publish', 'agent_signal', 'signal', 'coordination-write', 'publish')] }),
  descriptor({ operation: 'message.reply', use: 'Reply in an existing message thread.', routes: [route('signal reply', 'agent_signal', 'signal', 'coordination-write', 'reply')] }),
  descriptor({ operation: 'message.resolve', use: 'Resolve a handled message thread.', routes: [route('signal resolve', 'agent_signal', 'signal', 'coordination-write', 'resolve')] }),
  descriptor({ operation: 'memory.recall', use: 'Recall scoped reusable learning.', routes: [route('memory recall', 'memory_recall', 'memory-recall', 'read')] }),
  descriptor({ operation: 'memory.record', use: 'Record reusable evidence-linked learning.', routes: [route('memory record', 'memory_record', 'memory-record', 'coordination-write')] }),
  descriptor({ operation: 'history.status', use: 'Inspect LocalGit availability and durability.', routes: [route('history status', 'history_status', 'history', 'read', 'history_status')] }),
  descriptor({ operation: 'history.timeline', use: 'List bounded recoverable file history.', routes: [route('history timeline', 'history_timeline', 'history', 'read', 'history_timeline')] }),
  descriptor({ operation: 'history.read', use: 'Read one recoverable historical version.', routes: [route('history read', 'history_read', 'history', 'read', 'history_read')] }),
  descriptor({ operation: 'history.restore', use: 'Preview or apply one bound restore.', routes: choice('action', {
    preview: route('history restore-preview', 'history_restore_preview', 'history', 'read', 'history_restore_preview'),
    apply: route('history restore-apply', 'history_restore_apply', 'history', 'workspace-write', 'history_restore_apply', 'fs-delete'),
  }, 'preview') }),
] as const);

if (operationDescriptors.length > 19) throw new Error('Routine Awareness surface exceeds the nineteen-operation budget');
const byOperation = new Map<AwarenessOperation, AwarenessOperationDescriptor>(operationDescriptors.map(row => [row.operation, row]));
export function listAwarenessOperationDescriptors(): readonly AwarenessOperationDescriptor[] { return operationDescriptors; }
export function getAwarenessOperationDescriptor(operation: string): AwarenessOperationDescriptor | undefined { return byOperation.get(operation as AwarenessOperation); }
