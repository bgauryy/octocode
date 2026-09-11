import type { ApprovalClass } from '@octocodeai/agent-contracts/protocols';
import { z } from 'zod';
import type { AwarenessCommandResult } from '../command-api.js';
import type { AwarenessStorageScope } from '../storage-scope.js';
import { getAwarenessCommandDescriptor } from './cli.js';
import type { AwarenessCommandEffect } from './command-catalog.js';

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
  effects: readonly AwarenessCommandEffect[];
  legacyCommands: readonly string[];
  inputSchema: Readonly<Record<string, unknown>>;
  validate(params?: unknown): AwarenessOperationParams[K];
  effect(params?: AwarenessOperationParams[K]): AwarenessCommandEffect;
  approval(params?: AwarenessOperationParams[K]): ApprovalClass | undefined;
  outputBudget: number;
  continuations(payload: unknown): unknown;
  handler(context: AwarenessOperationExecutionContext, params?: AwarenessOperationParams[K]): Promise<AwarenessCommandResult>;
}

interface Route { command: string; selector?: readonly [key: string, value: string]; default?: boolean }
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
  const command = getAwarenessCommandDescriptor(route.command);
  if (!command) throw new Error(`missing Awareness command descriptor: ${route.command}`);
  const schema = stripHostBindings(structuredClone(command.inputSchema)) as Record<string, unknown>;
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
      const params = { ...(record(object['params']) ?? {}) };
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
const routeSets = new Map<AwarenessOperation, readonly Route[]>();

function descriptor<K extends AwarenessOperation>(input: DescriptorInput<K>): AwarenessOperationDescriptor<K> {
  routeSets.set(input.operation, input.routes);
  const schemas = input.routes.map(routeSchema);
  const inputSchema = input.inputSchema ?? (schemas.length === 1
    ? schemas[0]!
    : { $schema: 'https://json-schema.org/draft/2020-12/schema', oneOf: schemas });
  const validator = z.fromJSONSchema(inputSchema);
  for (const route of input.routes) reverseRoutes.set(route.command, { operation: input.operation, ...(route.selector ? { selector: route.selector } : {}) });
  const effects = [...new Set(input.routes.map(route => getAwarenessCommandDescriptor(route.command)!.effect))];
  const resolve = (value?: unknown) => {
    const params = record(value) ?? {};
    const route = selectRoute(input.operation, input.routes, params);
    const validation = validator.safeParse(params);
    if (!validation.success) throw new Error(`${input.operation} invalid parameters: ${validation.error.issues.map(issue => `${issue.path.join('.') || '/'} ${issue.message}`).join('; ')}`);
    return { route, params: validation.data as Params, commandParams: paramsForRoute(route, validation.data as Params) };
  };
  return Object.freeze({
    operation: input.operation,
    concept: input.operation.split('.')[0] as AwarenessConcept,
    use: input.use,
    visibility: 'routine' as const,
    effects: Object.freeze(effects),
    legacyCommands: Object.freeze(input.routes.map(route => route.command)),
    inputSchema: Object.freeze(inputSchema),
    validate: (params?: unknown) => resolve(params).params as AwarenessOperationParams[K],
    effect: (params?: AwarenessOperationParams[K]) => getAwarenessCommandDescriptor(resolve(params).route.command)!.effect,
    approval: (params?: AwarenessOperationParams[K]) => getAwarenessCommandDescriptor(resolve(params).route.command)!.approvalClass,
    outputBudget: input.outputBudget ?? 12_000,
    continuations: (payload: unknown) => canonicalizeContinuation(payload),
    handler: async (context: AwarenessOperationExecutionContext, params?: AwarenessOperationParams[K]) => {
      const selected = resolve(params);
      const executor = await import('../operation-executor.js');
      return input.operation === 'context.orient'
        ? executor.executeContextOrient(context, selected.params as AwarenessOperationParams['context.orient'])
        : executor.executeCanonicalCommand(selected.route.command, selected.commandParams, context);
    },
  });
}

const choice = (key: string, values: Readonly<Record<string, string>>, defaultValue?: string): Route[] =>
  Object.entries(values).map(([value, command]) => ({ command, selector: [key, value] as const, ...(value === defaultValue ? { default: true } : {}) }));

const operationDescriptors = Object.freeze([
  descriptor({
    operation: 'context.orient',
    use: 'Read bounded decision-changing context; reuse the revision on the next call.',
    routes: [{ command: 'attend' }],
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
  descriptor({ operation: 'work.create', use: 'Create a plan, task, or standalone work declaration.', routes: choice('kind', { plan: 'plan create', task: 'task create', standalone: 'work start' }) }),
  descriptor({ operation: 'work.list', use: 'List scoped plans, tasks, ready work, active presence, or the workboard.', routes: choice('kind', { plan: 'plan list', task: 'task list', ready: 'task ready', presence: 'work list', workboard: 'query workboard' }, 'presence') }),
  descriptor({ operation: 'work.show', use: 'Inspect one plan, task, or active file.', routes: choice('kind', { plan: 'plan show', task: 'task show', presence: 'work show' }) }),
  descriptor({ operation: 'work.claim', use: 'Atomically claim a task and start its attempt.', routes: [{ command: 'task claim' }] }),
  descriptor({ operation: 'work.update', use: 'Transition an existing work item or refresh its lease.', routes: choice('transition', { heartbeat: 'task heartbeat', submit: 'task submit', release: 'task release', retry: 'task retry', touch: 'work touch', end: 'work end', join: 'plan join', document: 'plan doc', status: 'plan status' }) }),
  descriptor({ operation: 'work.depend', use: 'Add a dependency between work items.', routes: [{ command: 'task depend' }] }),
  descriptor({ operation: 'work.protect', use: 'Acquire, wait for, or release exceptional exclusive protection.', routes: choice('action', { acquire: 'lock acquire', wait: 'lock wait', release: 'lock release' }) }),
  descriptor({ operation: 'work.verify', use: 'Audit verification debt or record an observed check result.', routes: choice('action', { audit: 'verify audit', mark: 'verify mark' }) }),
  descriptor({ operation: 'message.list', use: 'Read decision-changing messages.', routes: [{ command: 'signal list' }] }),
  descriptor({ operation: 'message.send', use: 'Send a decision-changing message.', routes: [{ command: 'signal publish' }] }),
  descriptor({ operation: 'message.reply', use: 'Reply in an existing message thread.', routes: [{ command: 'signal reply' }] }),
  descriptor({ operation: 'message.resolve', use: 'Resolve a handled message thread.', routes: [{ command: 'signal resolve' }] }),
  descriptor({ operation: 'memory.recall', use: 'Recall scoped reusable learning.', routes: [{ command: 'memory recall' }] }),
  descriptor({ operation: 'memory.record', use: 'Record reusable evidence-linked learning.', routes: [{ command: 'memory record' }] }),
  descriptor({ operation: 'history.status', use: 'Inspect LocalGit availability and durability.', routes: [{ command: 'history status' }] }),
  descriptor({ operation: 'history.timeline', use: 'List bounded recoverable file history.', routes: [{ command: 'history timeline' }] }),
  descriptor({ operation: 'history.read', use: 'Read one recoverable historical version.', routes: [{ command: 'history read' }] }),
  descriptor({ operation: 'history.restore', use: 'Preview or apply one bound restore.', routes: choice('action', { preview: 'history restore-preview', apply: 'history restore-apply' }, 'preview') }),
] as const);

if (operationDescriptors.length > 19) throw new Error('Routine Awareness surface exceeds the nineteen-operation budget');
const byOperation = new Map<AwarenessOperation, AwarenessOperationDescriptor>(operationDescriptors.map(row => [row.operation, row]));
export function listAwarenessOperationDescriptors(): readonly AwarenessOperationDescriptor[] { return operationDescriptors; }
export function getAwarenessOperationDescriptor(operation: string): AwarenessOperationDescriptor | undefined { return byOperation.get(operation as AwarenessOperation); }

export function resolveAwarenessOperation(call: Exclude<AwarenessOperationCall, { operation: 'context.orient' }>): { command: string; params: Params } {
  const descriptor = byOperation.get(call.operation);
  const routes = routeSets.get(call.operation);
  if (!descriptor || !routes) throw new Error(`Unknown Awareness operation: ${call.operation}`);
  const params = descriptor.validate(call.params) as Params;
  const route = selectRoute(call.operation, routes, params);
  return { command: route.command, params: paramsForRoute(route, params) };
}
