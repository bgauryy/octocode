import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';

export type WithOptionalMeta<T> = Partial<T>;

export interface ToolExecutionArgs<TQuery> {
  queries: TQuery[];

  responseCharOffset?: number;

  responseCharLength?: number;

  authInfo?: AuthInfo;

  sessionId?: string;

  signal?: AbortSignal;

  hintContext?: Record<string, unknown>;
}
