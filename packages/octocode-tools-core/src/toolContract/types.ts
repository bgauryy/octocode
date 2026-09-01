export type {
  ToolNames,
  ToolSchema,
  ToolSpec,
  ToolType,
} from './input/types/index.js';

import type { ToolNames } from './input/types/index.js';

export interface LocalCompleteMetadata {
  readonly systemPrompt: string;
  readonly toolNames: ToolNames;
  readonly baseSchema: Readonly<Record<string, string>>;
  readonly tools: Readonly<
    Record<string, { description?: string; schema?: Record<string, string> }>
  >;
}
