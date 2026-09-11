import type {
  AwarenessCommandContext,
  AwarenessCommandResult,
} from './command-api.js';
import {
  getAwarenessOperationDescriptor,
  listAwarenessOperationDescriptors,
  type AwarenessOperation,
  type AwarenessOperationDescriptor,
  type AwarenessOperationParams,
} from './schema/operation-catalog.js';

export interface AwarenessClientContext extends Omit<AwarenessCommandContext, 'compact' | 'continuations'> {
  workspace: string;
  agentId: string;
  sessionId?: string;
}

export interface AwarenessExecutableCall<K extends AwarenessOperation = AwarenessOperation> {
  operation: K;
  params?: AwarenessOperationParams[K];
}

export interface AwarenessPeerSummary {
  actorId: string;
  name?: string;
  status?: string;
  lastSeenAt?: string;
}

export interface AwarenessItemSummary {
  id?: string;
  title?: string;
  detail?: string;
  actorId?: string;
  path?: string;
  status?: string;
  locked?: boolean;
}

export interface AwarenessOrientation {
  revision: string;
  unchanged: false;
  self: { actorId: string; sessionId?: string };
  peers: { items: AwarenessPeerSummary[]; partial: boolean };
  work: { owned?: AwarenessItemSummary; overlaps: AwarenessItemSummary[] };
  inbox: AwarenessItemSummary[];
  verification: { pending: number; stale: number };
  continuation?: AwarenessItemSummary;
  recovery?: { degraded: boolean; pressure?: string };
  next: AwarenessExecutableCall[];
  partial: boolean;
  partialReasons: string[];
}

export interface AwarenessOrientationUnchanged {
  revision: string;
  unchanged: true;
}

export type AwarenessOrientationResult = AwarenessOrientation | AwarenessOrientationUnchanged;

export interface AwarenessClient {
  readonly context: Readonly<AwarenessClientContext>;
  orient(params?: AwarenessOperationParams['context.orient']): Promise<AwarenessOrientationResult>;
  execute<K extends AwarenessOperation>(call: AwarenessExecutableCall<K>): Promise<AwarenessCommandResult>;
  operations(): readonly AwarenessOperationDescriptor[];
}

export function createAwarenessClient(context: AwarenessClientContext): AwarenessClient {
  const bound = Object.freeze({ ...context });
  const execute = async <K extends AwarenessOperation>(call: AwarenessExecutableCall<K>): Promise<AwarenessCommandResult> => {
    const descriptor = getAwarenessOperationDescriptor(call.operation) as AwarenessOperationDescriptor<K> | undefined;
    if (!descriptor) {
      return { exitCode: 1, payload: { ok: false, operation: call.operation, error: `Unknown Awareness operation: ${call.operation}` } };
    }
    try {
      const params = descriptor.validate(call.params);
      const executed = await descriptor.handler(bound, params);
      return { ...executed, payload: descriptor.continuations(executed.payload) };
    } catch (error) {
      return {
        exitCode: 1,
        payload: { ok: false, operation: call.operation, error: error instanceof Error ? error.message : String(error) },
      };
    }
  };
  return Object.freeze({
    context: bound,
    operations: listAwarenessOperationDescriptors,
    async orient(params?: AwarenessOperationParams['context.orient']): Promise<AwarenessOrientationResult> {
      const result = await execute({ operation: 'context.orient', ...(params ? { params } : {}) });
      if (result.exitCode !== 0) throw new Error(`context.orient failed: ${JSON.stringify(result.payload)}`);
      return result.payload as AwarenessOrientationResult;
    },
    execute,
  });
}
