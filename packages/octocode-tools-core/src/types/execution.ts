import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type { HintContext } from './metadata.js';

export type WithOptionalMeta<T> = Partial<T>;

export interface ToolExecutionArgs<TQuery> {
  queries: TQuery[];

  responseCharOffset?: number;

  responseCharLength?: number;

  authInfo?: AuthInfo;

  sessionId?: string;

  hintContext?: HintContext;
}
