import type { QueryBatchResultRow, QueryRunType } from './query-envelope.js';
import { ToolResultError } from './tool-result-error.js';

/**
 * Runtime error for an ordered, non-transactional batch. Earlier successful
 * effects are intentionally retained and their count is exposed to callers.
 */
export class QueryBatchError extends Error {
  readonly failedIndex: number;
  readonly completedCount: number;
  readonly originalError: unknown;
  readonly rows: QueryBatchResultRow[];
  readonly queryRunType: QueryRunType;
  private hostReceiptAttached = false;

  constructor(
    failedIndex: number,
    completedCount: number,
    error: unknown,
    rows: QueryBatchResultRow[] = [],
    queryRunType: QueryRunType = "sequential",
  ) {
    const detail = error instanceof Error ? error.message : String(error);
    const icon = (status: QueryBatchResultRow["status"]): string =>
      status === "success" ? "✓" : status === "failed" ? "✗" : "○";
    const rowText =
      rows.length > 0
        ? `\n${rows.map((row) => `${icon(row.status)} [${row.index}] ${row.status}: ${row.summary}`).join("\n")}`
        : "";
    const prefix =
      queryRunType === "parallel"
        ? `queries[${failedIndex}] failed during parallel execution after ${completedCount} queries succeeded`
        : `queries[${failedIndex}] failed after ${completedCount} prior queries succeeded`;
    super(`${prefix}: ${detail}${rowText}`);
    this.name = "QueryBatchError";
    this.failedIndex = failedIndex;
    this.completedCount = completedCount;
    this.originalError = error;
    this.rows = rows;
    this.queryRunType = queryRunType;
  }

  /** Pi recognizes thrown failures only; preserve evidence in its text error channel. */
  withHostReceipt(): this {
    if (this.hostReceiptAttached) return this;
    const error = new ToolResultError({
      isError: true,
      content: [{ type: 'text', text: this.message }, ...this.rows.flatMap(row => row.content?.length ? [
        { type: 'text' as const, text: `queries[${row.index}] ${row.status} evidence:` }, ...row.content,
      ] : [])],
      details: {
        queryRunType: this.queryRunType,
        failedIndex: this.failedIndex,
        completedCount: this.completedCount,
        results: this.rows.map(({ content: _content, ...row }) => row),
      },
    }, 'query-batch');
    this.message = error.message;
    this.hostReceiptAttached = true;
    return this;
  }
}
