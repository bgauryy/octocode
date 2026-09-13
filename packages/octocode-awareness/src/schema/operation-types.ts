export const AWARENESS_CONCEPTS = Object.freeze(['context', 'work', 'message', 'memory', 'history'] as const);
export type AwarenessConcept = typeof AWARENESS_CONCEPTS[number];

export const ROUTINE_AWARENESS_OPERATIONS = Object.freeze([
  'context.orient', 'context.observe', 'context.feedback',
  'work.create', 'work.list', 'work.show', 'work.claim', 'work.update', 'work.depend', 'work.protect', 'work.verify',
  'message.list', 'message.send', 'message.reply', 'message.resolve',
  'memory.recall', 'memory.record',
  'memory.set', 'memory.get', 'memory.revalidate',
  'history.status', 'history.timeline', 'history.read', 'history.restore',
  'history.experience',
] as const);
export type AwarenessOperation = typeof ROUTINE_AWARENESS_OPERATIONS[number];

type Params = Record<string, unknown>;
type KindParams<K extends string> = Params & { kind: K };
type ActionParams<A extends string> = Params & { action: A };
type TransitionParams<T extends string> = Params & { transition: T };

export interface AwarenessOperationParams {
  'context.orient': { if_revision?: string; limit?: number; offset?: number; file?: string | string[]; query?: string; flow?: string; failure_signature?: string };
  'context.observe': ContextObservation;
  'context.feedback': ContextFeedback;
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
  'memory.set': Params;
  'memory.get': Params;
  'memory.revalidate': Params;
  'history.experience': ActionParams<'record' | 'get' | 'list' | 'seal' | 'compare' | 'recover'>;
  'history.status': Params;
  'history.timeline': Params;
  'history.read': Params;
  'history.restore': Params & { action?: 'preview' | 'apply' };
}

export type AwarenessOperationCall<K extends AwarenessOperation = AwarenessOperation> = K extends AwarenessOperation
  ? { operation: K; params?: AwarenessOperationParams[K] }
  : never;
import type { ContextObservation, ContextFeedback } from '../context-regulation.js';
