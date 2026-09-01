import type { CallToolResult } from '@modelcontextprotocol/server';
import { type z } from 'zod';

export type ToolDirectSecurity = 'basic' | 'remote';

export interface ToolDirectExecutionConfig {
  schema: z.ZodType;

  inputSchema: z.ZodType;
  executionFn: (input: never) => Promise<CallToolResult>;
  security: ToolDirectSecurity;
  requiresServerRuntime?: boolean;
  requiresProviders?: boolean;
  /** Outer security boundary for tools whose own bounded operation may exceed
   * the shared 60-second default (for example a shallow repository clone). */
  timeoutMs?: number;
}

export interface ToolConfig {
  name: string;
  title: string;
  description: string;
  isDefault: boolean;
  isLocal: boolean;

  isClone?: boolean;
  type: 'search' | 'content' | 'history' | 'debug';

  direct: ToolDirectExecutionConfig;
}
