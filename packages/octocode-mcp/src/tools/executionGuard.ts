import type { ProcessedBulkResult } from '../types/toolResults.js';
import { handleCatchError } from './utils.js';

interface GuardableQuery {
  mainResearchGoal?: string;
  researchGoal?: string;
  reasoning?: string;
}

interface ExecutionGuardOptions<TQuery extends GuardableQuery> {
  toolName: string;
  query: TQuery;
  execute: () => Promise<ProcessedBulkResult>;
  contextMessage?: string;
}

export async function executeWithToolBoundary<TQuery extends GuardableQuery>({
  toolName,
  query,
  execute,
  contextMessage,
}: ExecutionGuardOptions<TQuery>): Promise<ProcessedBulkResult> {
  try {
    return await execute();
  } catch (error) {
    return handleCatchError(error, query, contextMessage, toolName);
  }
}
