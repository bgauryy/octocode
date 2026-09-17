import type { ObservedCheckReceipt, ExternalPlanScope } from '@octocodeai/octocode-awareness/host';
import type { QueryRecord } from '../query-envelope.js';
import type { StepInput } from './plan-types.js';

export type PlanAction = 'set' | 'propose' | 'clarify' | 'add' | 'start' | 'complete' | 'remove' | 'clear' | 'show';

export interface ClarifyQuestion {
  prompt: string;
  options?: Array<{ value?: string; label: string; description?: string; recommended?: boolean; pros?: string[]; cons?: string[] }>;
}

export interface PlanParams extends QueryRecord {
  action: PlanAction;
  scope?: ExternalPlanScope;
  receipt?: ObservedCheckReceipt;
  steps?: StepInput[];
  text?: string;
  activeForm?: string;
  dependsOn?: number[];
  paths?: string[];
  taskReasoning?: string;
  acceptance?: string;
  checkCommand?: string;
  index?: number;
  revision?: string;
  authorizationInteractionId?: string;
  consequential?: boolean;
  rfcPath?: string;
  questions?: ClarifyQuestion[];
  reason?: string;
}

const ACTION_FIELDS: Readonly<Record<PlanAction, readonly string[]>> = Object.freeze({
  set: ['scope', 'steps', 'consequential', 'reason', 'rfcPath'],
  propose: ['scope', 'steps', 'consequential', 'reason', 'rfcPath'],
  clarify: ['questions'],
  add: ['scope', 'text', 'activeForm', 'dependsOn', 'paths', 'taskReasoning', 'acceptance', 'checkCommand'],
  start: ['scope', 'index', 'revision', 'authorizationInteractionId'],
  complete: ['scope', 'index', 'receipt'],
  remove: ['scope', 'index'],
  clear: ['scope'],
  show: ['scope'],
});

const ACTIONS: readonly PlanAction[] = ['set', 'propose', 'clarify', 'add', 'start', 'complete', 'remove', 'clear', 'show'];

export function preflightPlanQuery(query: QueryRecord): void {
  const action = String(query['action'] ?? '');
  if (!ACTIONS.includes(action as PlanAction)) {
    throw new Error(`unknown plan action: "${action}". Must be one of: ${ACTIONS.join(', ')}.`);
  }
  const allowed = new Set(['reasoning', 'action', ...ACTION_FIELDS[action as PlanAction]]);
  const extra = Object.keys(query).filter((field) => !allowed.has(field));
  if (extra.length) throw new Error(`action:${action} does not accept ${extra.join(', ')}.`);
  if (query['scope'] !== undefined && !['auto', 'session', 'shared'].includes(String(query['scope']))) {
    throw new Error(`invalid scope: ${String(query['scope'])}. Must be auto, session, or shared.`);
  }
  if (action === 'set' || action === 'propose') {
    if (!Array.isArray(query['steps'])) throw new Error(`action:${action} — steps must be an array.`);
    if (!query['steps'].length) throw new Error(`action:${action} — steps must not be empty; use action:clear to remove a plan.`);
  }
  if (action === 'add' && (typeof query['text'] !== 'string' || !query['text'].trim())) {
    throw new Error('action:add requires a non-empty text field.');
  }
  validateReceipt(action, query['receipt']);
  validateStart(action, query);
  if ((action === 'start' || action === 'complete' || action === 'remove') && query['index'] != null) {
    const index = Number(query['index']);
    if (!Number.isInteger(index) || index < 1) throw new Error(`action:${action} — index must be a positive integer when provided (got ${String(query['index'])}).`);
  }
}

function validateReceipt(action: string, receipt: unknown): void {
  if (receipt === undefined) return;
  if (action !== 'complete' || !receipt || typeof receipt !== 'object' || Array.isArray(receipt)) {
    throw new Error('receipt is only valid as an object for action:complete.');
  }
  const record = receipt as Record<string, unknown>;
  if (typeof record['command'] !== 'string' || !record['command'].trim()) throw new Error('receipt.command is required.');
  if (record['status'] !== 'SUCCESS' && record['status'] !== 'FAILED') throw new Error('receipt.status must be SUCCESS or FAILED.');
  if (typeof record['message'] !== 'string' || !record['message'].trim()) throw new Error('receipt.message is required.');
}

function validateStart(action: string, query: QueryRecord): void {
  if (action !== 'start') return;
  const hasRevision = typeof query['revision'] === 'string' && query['revision'].trim().length > 0;
  const hasInteraction = typeof query['authorizationInteractionId'] === 'string' && query['authorizationInteractionId'].trim().length > 0;
  if (hasInteraction && !hasRevision) throw new Error('reviewed action:start authorizationInteractionId requires revision.');
  if (hasRevision && query['index'] !== undefined) throw new Error('reviewed action:start cannot include index.');
}
