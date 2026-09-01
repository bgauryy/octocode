import type { AuthInfo } from '@modelcontextprotocol/server';

export type WithOptionalMeta<T> = Partial<T>;

export interface BaseQueryLocal {
  goal?: string;
  reasoning?: string;
  path?: string;
}

export interface ToolExecutionArgs<TQuery> {
  queries: TQuery[];

  responseCharOffset?: number;

  responseCharLength?: number;

  authInfo?: AuthInfo;

  sessionId?: string;

  signal?: AbortSignal;

  hintContext?: Record<string, unknown>;
}
