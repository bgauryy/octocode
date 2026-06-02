/**
 * Standardized execution arguments for all tools
 * @module types/execution
 */

import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import type { HintContext } from './metadata.js';

/**
 * Makes all upstream fields optional so tests can call src functions without
 * providing ZodDefault fields (which are required in the inferred TypeScript type
 * but have runtime defaults). This covers metadata fields (id, researchGoal, etc.)
 * as well as ZodDefault fields (limit, page, details, entriesPerPage, etc.).
 */
export type WithOptionalMeta<T> = Partial<T>;

/**
 * Standardized execution arguments for all tools.
 * Provides a consistent interface across GitHub, Local, LSP, and Package tools.
 *
 * @template TQuery - The query type specific to each tool
 *
 * @example
 * ```typescript
 * // GitHub tool
 * export async function searchMultipleGitHubCode(
 *   args: ToolExecutionArgs<GitHubCodeSearchQuery>
 * ): Promise<CallToolResult>
 *
 * // Local tool
 * export async function executeViewStructure(
 *   args: ToolExecutionArgs<ViewStructureQuery>
 * ): Promise<CallToolResult>
 * ```
 */
export interface ToolExecutionArgs<TQuery> {
  /** Array of queries to execute (1-N per call) */
  queries: TQuery[];

  /** Character offset for top-level bulk response pagination across results[] */
  responseCharOffset?: number;

  /** Character length budget for top-level bulk response pagination across results[] */
  responseCharLength?: number;

  /** Optional OAuth authentication info (GitHub tools) */
  authInfo?: AuthInfo;

  /** Optional session ID for tracking/logging */
  sessionId?: string;

  /** Optional hint context for customizing generated hints */
  hintContext?: HintContext;
}
