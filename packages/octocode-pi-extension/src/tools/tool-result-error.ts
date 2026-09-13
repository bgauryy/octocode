import type { ToolCallResult } from '../types.js';

/** Translate internal error results into Pi's thrown, text-only failure channel. */
export class ToolResultError extends Error {
  constructor(
    readonly result: ToolCallResult,
    toolName: string,
  ) {
    super(result.content.map(part => part.type === 'text' ? part.text : JSON.stringify(part)).join('\n\n') || `${toolName} failed`);
    this.name = 'ToolResultError';
  }
}
