import type { AuthInfo } from '@modelcontextprotocol/server';
import type { BulkResponseExecutionOptions } from './bulk.js';

export type WithOptionalMeta<T> = Partial<T>;

export interface BaseQueryLocal {
  goal?: string;
  reasoning?: string;
  path?: string;
}

export interface ToolExecutionArgs<
  TQuery,
> extends BulkResponseExecutionOptions {
  queries: TQuery[];

  authInfo?: AuthInfo;

  sessionId?: string;

  signal?: AbortSignal;

  hintContext?: Record<string, unknown>;
}
