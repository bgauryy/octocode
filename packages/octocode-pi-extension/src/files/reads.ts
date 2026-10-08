/**
 * Which tool calls showed the model a file whole: for the `file` tool's guard (`tool.ts`), and for compaction, which
 * forgets a read once its result is trimmed.
 */
import { resolveToolPath } from '../shared/home.js';
import { isRecord } from '../shared/util.js';

/** Octocode MCP's local file reader: its `fullContent` queries ask for a whole file (a large one comes back paged). */
export const MCP_READ_TOOL = 'mcp__octocode__localFetch';

/** `localFetch` query fields that select part of a file: line ranges, match windows, an offset/length window. */
const PARTIAL_READ_FIELDS = ['ranges', 'matchString', 'offset', 'length'] as const;

/**
 * Whether an MCP `localFetch` query returned its whole file unminified: `fullContent` without a range, match or
 * window, and without minification (a range, match or default view shows only part of it, or a compacted form).
 */
function wholeFileQuery(query: unknown): query is { path: string } {
  if (!isRecord(query) || typeof query['path'] !== 'string' || query['fullContent'] !== true) return false;
  if (PARTIAL_READ_FIELDS.some((field) => query[field] !== undefined)) return false;
  return query['minify'] === undefined || query['minify'] === 'none';
}

/**
 * Paths a call asked to read whole: Pi's `read` (any range: Pi's own guard semantics), and the whole-file queries of
 * Octocode MCP's `localFetch` (none when the call pages the response text with `responseOffset` or `responseLength`).
 * The request alone: `wholeReadPaths` also checks what came back.
 */
export function readPaths(toolName: string, input: Record<string, unknown>, cwd: string): string[] {
  if (toolName === 'read') return typeof input['path'] === 'string' ? [resolveToolPath(cwd, input['path'])] : [];
  return wholeFileQueries(toolName, input).map(({ query }) => resolveToolPath(cwd, query.path));
}

/** The whole-file `localFetch` queries of a call, with their positions (a result row's `index`). */
function wholeFileQueries(toolName: string, input: Record<string, unknown>): Array<{ index: number; query: { path: string } }> {
  if (toolName !== MCP_READ_TOOL || !Array.isArray(input['queries'])) return [];
  if (input['responseOffset'] !== undefined || input['responseLength'] !== undefined) return [];
  return input['queries'].flatMap((query: unknown, index) => (wholeFileQuery(query) ? [{ index, query }] : []));
}

/** The result of a tool call as the `tool_result` event carries it. */
export interface ReadResult {
  structuredContent?: unknown;
  details?: unknown;
}

/**
 * Octocode's result rows by query index. Pi passes the whole MCP `CallToolResult` as `structuredContent`, so the
 * server's `{ results: [{ index, data }] }` sits one level down. Undefined when there is none, or when the response
 * itself was paged (`responsePagination`): then the model saw only a window of it.
 */
function resultRows(structured: unknown): Map<number, unknown> | undefined {
  const payload = isRecord(structured) && !Array.isArray(structured['results']) ? structured['structuredContent'] : structured;
  if (!isRecord(payload) || !Array.isArray(payload['results']) || payload['responsePagination'] !== undefined) return undefined;
  const rows = new Map<number, unknown>();
  payload['results'].forEach((row: unknown, position) => rows.set(isRecord(row) && typeof row['index'] === 'number' ? row['index'] : position, row));
  return rows;
}

/**
 * Whether a `localFetch` result row shows its whole file: no error, no partial flag, continuation or further page,
 * and every line numbered once in order (`1\t…` to `totalLines`), so no line is missing.
 */
function completeRow(row: unknown): boolean {
  if (!isRecord(row) || (row['status'] !== undefined && row['status'] !== 'ok') || !isRecord(row['data'])) return false;
  const data = row['data'];
  if (data['errorCode'] !== undefined || data['error'] !== undefined || data['isPartial'] === true || data['next'] !== undefined) return false;
  if (isRecord(data['pagination']) && data['pagination']['hasMore'] === true) return false;
  const content = data['content'];
  const total = data['totalLines'];
  if (typeof content !== 'string' || typeof total !== 'number') return false;
  const lines = content === '' ? [] : content.replace(/\n$/, '').split('\n');
  return lines.length === total && lines.every((line, index) => line.startsWith(`${index + 1}\t`));
}

/**
 * Paths the model has now seen whole enough to change. Pi's `read`: as `readPaths`. Octocode MCP's `localFetch`: the
 * whole-file queries whose result row is complete (a large file comes back as a first page with `isPartial` and a
 * `next` continuation, which is not a whole read), and none when Pi cut the middle out of the result text
 * (`details.fullOutputPath`) or the result carries no rows. The guard keeps their stat to compare against later.
 */
export function wholeReadPaths(toolName: string, input: Record<string, unknown>, cwd: string, result: ReadResult): string[] {
  if (toolName === 'read') return readPaths(toolName, input, cwd);
  const queries = wholeFileQueries(toolName, input);
  if (queries.length === 0 || (isRecord(result.details) && result.details['fullOutputPath'] !== undefined)) return [];
  const rows = resultRows(result.structuredContent);
  if (!rows) return [];
  return queries.filter(({ index }) => completeRow(rows.get(index))).map(({ query }) => resolveToolPath(cwd, query.path));
}
