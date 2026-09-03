/**
 * Shared rendering utilities for Octocode Pi extension tool renderers.
 *
 * Centralises:
 *  - ANSI-aware line truncation (replaces 3 copies across the codebase)
 *  - Per-tool call-summary extraction (smart param display instead of raw JSON)
 *  - Per-tool result-stats extraction (counts, paths, match totals)
 *  - A tiny `makeRenderer` factory for the Component interface
 */

import { truncateToWidth as piTruncateToWidth, visibleWidth as piVisibleWidth } from '@earendil-works/pi-tui';

import {
  CLI_STATUS_TEXT,
  paint,
} from '../tui/cli-design.js';
import type { PiTheme, RenderCallReturn, RenderContext, ToolCallResult } from '../types.js';
import {
  renderToolView,
  type InlineSegment,
  type ToolViewLine,
  type ToolViewProps,
  type TuiComponent,
  type TuiRenderContext,
} from '../tui/components.js';

// ─── ANSI-safe width helpers ──────────────────────────────────────────────────
//
// Width measurement and truncation delegate to pi-tui's own visibleWidth /
// truncateToWidth. pi's renderer crashes any line whose pi-tui-measured width
// exceeds the terminal width, and pi's extension loader aliases the
// `@earendil-works/pi-tui` import to the host's bundled copy — so delegating
// guarantees we can never disagree with the arbiter of that check.

// sanitizeLine lives in palette.ts so cli-design (which render-helpers imports) can
// reuse it without a cycle. Imported for internal use and re-exported for existing importers.
import { sanitizeLine } from '../tui/palette.js';
export { sanitizeLine };

export function visibleWidth(str: string): number {
  return piVisibleWidth(sanitizeLine(str));
}

/**
 * Truncate `str` so its *visible* width (ANSI codes excluded) ≤ `maxWidth`.
 * When truncated, pi-tui inserts SGR resets around the appended ellipsis so
 * open colour sequences don't bleed into subsequent lines.
 */
export function truncateToWidth(
  str: string,
  maxWidth: number,
  ellipsis = '\u2026',
): string {
  return piTruncateToWidth(sanitizeLine(str), maxWidth, ellipsis);
}

/**
 * Truncate PLAIN text (no ANSI codes) to at most `maxWidth` visible cells,
 * counting CJK/emoji as their true width. Unlike pi-tui's truncateToWidth this
 * injects no SGR reset sequences (the input has no colour to bleed), so it is
 * safe for values that are theme-wrapped afterwards. `ellipsis` is appended
 * within the budget when truncation occurs (pass '' for a hard cut).
 */
export function truncatePlainToWidth(text: string, maxWidth: number, ellipsis = '\u2026'): string {
  if (maxWidth <= 0) return '';
  if (visibleWidth(text) <= maxWidth) return text;
  const ellW = visibleWidth(ellipsis);
  if (maxWidth <= ellW) return ellipsis.slice(0, maxWidth) || ellipsis;
  const budget = maxWidth - ellW;
  let out = '';
  let used = 0;
  for (const ch of Array.from(text)) {
    const w = visibleWidth(ch);
    if (used + w > budget) break;
    out += ch;
    used += w;
  }
  return out + ellipsis;
}

/**
 * Word-wrap plain text (no ANSI codes) into lines of at most `maxWidth` visible
 * characters each. Words longer than `maxWidth` are hard-truncated on that boundary.
 */
export function wrapText(text: string, maxWidth: number): string[] {
  if (maxWidth <= 0) return [];
  const words = text.split(' ');
  const lines: string[] = [];
  let current = '';
  for (const word of words) {
    // Budget by visible cell width, not byte/code-unit length — CJK/emoji are
    // 2 cells, so a .length check under-counts and lets a visually-too-wide line
    // through, which pi's TUI hard-clips (or crashes on).
    const safeWord = visibleWidth(word) > maxWidth ? truncatePlainToWidth(word, maxWidth, '') : word;
    if (!current) {
      current = safeWord;
    } else {
      const candidate = `${current} ${safeWord}`;
      if (visibleWidth(candidate) <= maxWidth) {
        current = candidate;
      } else {
        lines.push(current);
        current = safeWord;
      }
    }
  }
  if (current) lines.push(current);
  return lines.length ? lines : [''];
}

// ─── Tiny component factory ───────────────────────────────────────────────────

/**
 * Build a multi-line terminal component from pre-built lines.
 *
 * Applies `truncateToWidth` to **every** emitted line as a final safety net so
 * that no line can ever exceed the terminal width and crash pi's TUI, regardless
 * of whether the caller remembered to truncate individually.  Because
 * `truncateToWidth` is idempotent on already-short strings this has zero cost.
 */
export function makeComponentRenderer<Props>(
  component: TuiComponent<Props>,
  props: Props | (() => Props),
  theme?: PiTheme,
): RenderCallReturn {
  return {
    render: (width = 80) => {
      const context: TuiRenderContext = { width, theme };
      const resolved = typeof props === 'function' ? (props as () => Props)() : props;
      return component(resolved, context).map((line) => truncateToWidth(line, width));
    },
    invalidate() { /* no-op */ },
  };
}

/** Compatibility adapter: every historical line callback now runs as a TuiComponent. */
export function makeRenderer(lines: (width: number) => string[]): RenderCallReturn {
  return makeComponentRenderer((_props: undefined, context) => lines(context.width), undefined);
}

export function singleLineRenderer(rawLine: string): RenderCallReturn {
  return makeRenderer((w) => [truncateToWidth(rawLine, w)]);
}

/** Public adapter for the shared tool-view composition. */
export function buildToolView(
  props: ToolViewProps | (() => ToolViewProps),
  theme?: PiTheme,
): RenderCallReturn {
  return makeComponentRenderer(renderToolView, props, theme);
}

/**
 * Like makeRenderer but memoizes rendered lines per width (docs/tui.md
 * "Performance"). Use ONLY when the line data is fixed at construction time — the
 * closure must capture no live mutable state. Safe for the tool-row builders
 * below (a fresh renderResult/renderCall call rebuilds them when data changes).
 * Do NOT use for the footer / status-panel / spinner renderers, whose closures
 * read live state at render time and must recompute every frame. invalidate()
 * drops the cache (Pi calls it on theme change).
 */
export function makeCachedRenderer(lines: (width: number) => string[]): RenderCallReturn {
  let cachedWidth: number | undefined;
  let cachedLines: string[] | undefined;
  return {
    render(width = 80) {
      if (cachedLines && cachedWidth === width) return cachedLines;
      cachedLines = lines(width).map((line) => truncateToWidth(line, width));
      cachedWidth = width;
      return cachedLines;
    },
    invalidate() {
      cachedWidth = undefined;
      cachedLines = undefined;
    },
  };
}

// ─── Tool-call summary (replaces raw JSON dump in renderCall) ─────────────────

type QueryLike = Record<string, unknown>;

export interface QueryCallRenderOptions {
  reason?: (query: QueryLike, index: number) => string;
  stripReasonKeys?: string[];
}

function queryEnvelope(args: unknown): { envelope: QueryLike; queries: QueryLike[] } {
  const envelope = args && typeof args === 'object' && !Array.isArray(args)
    ? args as QueryLike
    : {};
  const values = Array.isArray(envelope['queries'])
    ? envelope['queries'].filter((value): value is QueryLike => Boolean(value) && typeof value === 'object' && !Array.isArray(value))
    : Object.keys(envelope).length > 0
      ? [envelope]
      : [];
  return { envelope, queries: values };
}

/**
 * Render every submitted query as its existing single-operation block followed
 * immediately by one muted, unlabeled reasoning line.
 */
export function buildQueryCallBlocks(
  args: unknown,
  theme: PiTheme | undefined,
  renderSingle: (singleArgs: Record<string, unknown>, index: number) => RenderCallReturn,
  options: QueryCallRenderOptions = {},
): RenderCallReturn {
  const { envelope, queries } = queryEnvelope(args);
  if (queries.length === 0) return renderSingle(args as Record<string, unknown>, 0);
  const stripKeys = new Set(options.stripReasonKeys ?? ['reasoning', 'reason']);
  const reasonFor = options.reason ?? ((query: QueryLike) => str(query['reason'] ?? query['reasoning']).trim());
  const explicitRunType = envelope['queryRunType'] === 'parallel' || envelope['queryRunType'] === 'sequential'
    ? envelope['queryRunType'] as 'parallel' | 'sequential'
    : undefined;
  const runType = explicitRunType ?? 'sequential';

  return makeCachedRenderer((width) => {
    const lines: string[] = queries.length > 1 || explicitRunType
      ? [truncateToWidth(paint(theme, runType === 'parallel' ? 'link' : 'muted', `↳ ${queries.length} quer${queries.length === 1 ? 'y' : 'ies'} · ${runType}`), width)]
      : [];
    for (const [index, query] of queries.entries()) {
      const clean = Object.fromEntries(Object.entries(query).filter(([key]) => !stripKeys.has(key)));
      const singleArgs = Array.isArray(envelope['queries'])
        ? { ...envelope, queries: [clean] }
        : { queries: [clean] };
      lines.push(...renderSingle(singleArgs, index).render(width));
      const reason = reasonFor(query, index);
      if (reason) lines.push(truncateToWidth(paint(theme, 'muted', `  ${reason}`), width));
    }
    return lines;
  });
}

function str(v: unknown): string {
  return typeof v === 'string' && v ? v : '';
}
function arr(v: unknown): string[] {
  return Array.isArray(v) ? v.map(String).filter(Boolean) : [];
}
function basename(p: string): string {
  return p.replace(/^.*[\\/]/, '');
}
function shortPath(p: string, maxLen = 50): string {
  if (visibleWidth(p) <= maxLen) return p;
  // Keep the tail (the most specific path segments), cell-width aware so CJK
  // segments count double and surrogate pairs are never split.
  const chars = Array.from(p);
  let width = 1; // leading ellipsis
  let start = chars.length;
  while (start > 0 && width + visibleWidth(chars[start - 1]!) <= maxLen) {
    width += visibleWidth(chars[start - 1]!);
    start -= 1;
  }
  return '…' + chars.slice(start).join('');
}

/**
 * Extract a human-readable one-liner from a tool call's args object.
 * All octocode tools take `{ queries: [...] }` at the top level.
 * Dispatches per tool name to show the most useful information.
 */
export function buildToolCallSummary(toolName: string, args: unknown): string {
  const a = (args ?? {}) as Record<string, unknown>;
  const queries = Array.isArray(a.queries) ? (a.queries as QueryLike[]) : [];
  const q = queries[0] ?? {};

  // ── GitHub tools ─────────────────────────────────────────────────────────
  if (toolName.startsWith('gh')) {
    const repo = [str(q.owner), str(q.repo)].filter(Boolean).join('/');

    if (toolName === 'ghSearch' && q.operation === 'code') {
      const kw = arr(q.keywords).join(' ');
      const lang = str(q.language);
      const fn = str(q.filename);
      const parts = [
        kw ? `"${kw}"` : '',
        fn ? `file:${fn}` : '',
        lang ? `lang:${lang}` : '',
        repo ? `in ${repo}` : '',
      ].filter(Boolean).join(' ');
      return parts.trim();
    }

    if (toolName === 'ghSearch' && q.operation === 'repositories') {
      const kw = arr(q.keywords).join(' ');
      const lang = str(q.language);
      return [kw ? `"${kw}"` : '', lang ? `lang:${lang}` : ''].filter(Boolean).join(' ').trim();
    }

    if (toolName === 'ghGetFileContent') {
      const p = str(q.path);
      const matchStr = str(q.matchString);
      const start = q.startLine != null ? `:${q.startLine}` : '';
      const end = q.endLine != null ? `-${q.endLine}` : '';
      const anchor = matchStr ? ` /${truncatePlainToWidth(matchStr, 20, '')}/` : start + end;
      return `${repo}${p ? `:${p}` : ''}${anchor}`.trim();
    }

    if (toolName === 'ghSearch' && q.operation === 'tree') {
      const p = str(q.path);
      return `${repo}${p && p !== '.' ? `/${p}` : ''}`.trim();
    }

    if (toolName === 'ghSearchPullRequests' || toolName === 'ghSearchIssues') {
      const keywords = arr(q.keywordsToSearch).join(' ');
      const number = q.prNumber ?? q.issueNumber;
      const kind = toolName === 'ghSearchPullRequests' ? 'PR' : 'issue';
      const detail = number != null ? `${kind} #${number}` : keywords ? `"${keywords}"` : kind;
      return `${repo} ${detail}`.trim();
    }

    if (toolName === 'ghSearchCommits') {
      const pathValue = str(q.path);
      const range = [str(q.base), str(q.head)].filter(Boolean).join('..');
      return `${repo}${pathValue ? ` path:${pathValue}` : ''}${range ? ` ${range}` : ''}`.trim();
    }

    if (toolName === 'ghCloneRepo') {
      const sp = str(q.sparsePath);
      return `${repo}${sp ? `/${sp}` : ''}`.trim();
    }

    return repo.trim();
  }

  // ── Local tools ───────────────────────────────────────────────────────────
  if (toolName.startsWith('local') || toolName === 'lspGetSemantics') {
    if (toolName === 'localSearch' && (q.operation === 'text' || q.operation === 'structural')) {
      const kw = str(q.searchText ?? q.pattern ?? q.rule ?? q.keywords);
      const p = str(q.path);
      const mode = str(q.operation);
      const modeTag = mode ? `[${mode}] ` : '';
      return `${modeTag}${kw ? `"${kw}"` : ''}${p ? ` in ${shortPath(p)}` : ''}`.trim();
    }

    if (toolName === 'localGetFileContent') {
      const p = str(q.path);
      const start = q.startLine != null ? `:${q.startLine}` : '';
      const end = q.endLine != null ? `-${q.endLine}` : '';
      const matchStr = str(q.matchString);
      const anchor = matchStr ? ` /${truncatePlainToWidth(matchStr, 20, '')}/` : start + end;
      return (shortPath(p) + anchor).trim();
    }

    if (toolName === 'localSearch' && q.operation === 'tree') {
      const p = str(q.path);
      const depth = q.maxDepth != null ? ` depth:${q.maxDepth}` : '';
      return (shortPath(p) + depth).trim();
    }

    if (toolName === 'localSearch' && q.operation === 'files') {
      const p = str(q.path);
      const names = arr(q.names).join(', ');
      const pat = str(q.pathPattern);
      return `${shortPath(p)}${names ? ` [${names}]` : ''}${pat ? ` ${pat}` : ''}`.trim();
    }

    if (toolName === 'localAnalyzeGraph' && q.operation === 'deadCode') {
      const p = str(q.path);
      const entrypoints = arr(q.entrypoints).join(', ');
      return `${shortPath(p)}${entrypoints ? ` entries:[${entrypoints}]` : ''}`.trim();
    }

    if (toolName === 'lspGetSemantics') {
      const sym = str(q.symbolName);
      const type = str(q.type) || 'definition';
      const uri = str(q.uri);
      const file = uri ? basename(uri.replace(/\?.*$/, '')) : '';
      const line = q.lineHint != null ? `:${q.lineHint}` : '';
      return `${type}${sym ? ` "${sym}"` : ''}${file ? ` in ${file}${line}` : ''}`.trim();
    }

    // Other local-tool fallthrough.
    const p = str(q.path);
    return shortPath(p).trim();
  }

  // ── npm ──────────────────────────────────────────────────────────────────
  if (toolName === 'npmSearch') {
    const pkg = str(q.packageName);
    return pkg.trim();
  }

  // ── fallback: pick the 3 most informative string values ──────────────────
  const SKIP_KEYS = new Set(['id', 'reasoning', 'researchGoal', 'mainResearchGoal', 'resolvedPath']);
  const parts = Object.entries(q)
    .filter(([k]) => !SKIP_KEYS.has(k))
    .map(([, v]) => truncatePlainToWidth(String(v ?? ''), 40))
    .filter(Boolean)
    .slice(0, 3);
  return parts.join(' ').trim();
}

// ─── Result stats (replaces generic "N items" in renderResult) ────────────────

export interface ResultStats {
  /** Total query count that produced results */
  queryCount?: number;
  /** Human-readable match/result total */
  summary?: string;
  /** Short file/repo paths to show inline */
  paths?: string[];
  /** Small preview values that show what data came back without dumping the full payload. */
  previews?: string[];
  /** Whether any result had an error */
  hasError?: boolean;
}

/**
 * Extract meaningful result stats from a tool's `details` object.
 * The structured output from octocode tools is typically:
 *   `{ results: [{ id, data: { ... tool-specific ... } }] }`
 */
function previewText(value: unknown, max = 72): string {
  const clean = String(value ?? '').replace(/\s+/g, ' ').trim();
  return truncatePlainToWidth(clean, max);
}

export function buildResultStats(toolName: string, details: unknown): ResultStats {
  if (!details || typeof details !== 'object') return {};
  const d = details as Record<string, unknown>;

  const results = Array.isArray(d.results) ? (d.results as Record<string, unknown>[]) : [];
  const queryCount = results.length > 0 ? results.length : undefined;

  // Per-tool structured extraction
  if (toolName === 'ghSearch') {
    let total = 0;
    const repos: string[] = [];
    const previews: string[] = [];
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      const operation = str(data.operation);
      const items = operation === 'repositories'
        ? data.repositories
        : operation === 'code' ? data.files : data.structure;
      const rows = Array.isArray(items) ? items as Record<string, unknown>[] : [];
      total += rows.length;
      for (const item of rows.slice(0, 3)) {
          const repository = item.repository && typeof item.repository === 'object'
            ? item.repository as Record<string, unknown> : undefined;
          const name = str(item.fullName ?? item.name ?? repository?.fullName ?? item.path);
          if (operation === 'repositories' && name) repos.push(name);
          if (name) previews.push(previewText(name));
      }
    }
    return {
      queryCount,
      summary: total > 0 ? `${total} results` : undefined,
      paths: repos.length > 0 ? repos : undefined,
      previews: previews.length > 0 ? previews.slice(0, 3) : undefined,
    };
  }

  if (toolName === 'ghGetFileContent') {
    const paths: string[] = [];
    const previews: string[] = [];
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      const p = str(data.path ?? data.filePath);
      if (p) paths.push(basename(p));
      const text = str(data.content ?? data.text ?? data.contentView);
      if (text) previews.push(previewText(text));
    }
    return { queryCount, paths: paths.slice(0, 4), previews: previews.slice(0, 2) };
  }

  if (toolName === 'ghCloneRepo') {
    const paths: string[] = [];
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      const p = str(data.localPath ?? data.path);
      if (p) paths.push(shortPath(p, 45));
    }
    return { queryCount, paths: paths.slice(0, 2) };
  }

  if (toolName === 'localSearch') {
    let matchCount = 0;
    let fileCount = 0;
    let entryCount = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      const stats = data.stats && typeof data.stats === 'object' ? data.stats as Record<string, unknown> : {};
      if (typeof stats.totalOccurrences === 'number') matchCount += stats.totalOccurrences;
      if (typeof stats.filesMatched === 'number') fileCount += stats.filesMatched;
      if (Array.isArray(data.files)) entryCount += data.files.length;
      if (Array.isArray(data.folders)) entryCount += data.folders.length;
    }
    const parts = [
      matchCount > 0 ? `${matchCount} matches` : '',
      fileCount > 0 ? `${fileCount} files` : '',
      matchCount === 0 && entryCount > 0 ? `${entryCount} entries` : '',
    ].filter(Boolean);
    return { queryCount, summary: parts.join(', ') || undefined };
  }

  if (toolName === 'localGetFileContent') {
    const paths: string[] = [];
    const previews: string[] = [];
    let lines = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      const p = str(data.path ?? data.resolvedPath);
      if (p) paths.push(basename(p));
      if (typeof data.totalLines === 'number') lines += data.totalLines;
      const text = str(data.content ?? data.text ?? data.contentView);
      if (text) previews.push(previewText(text));
    }
    return {
      queryCount,
      paths: paths.slice(0, 4),
      summary: lines > 0 ? `${lines} lines` : undefined,
      previews: previews.slice(0, 2),
    };
  }

  if (toolName === 'localAnalyzeGraph') {
    let resultCount = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      if (Array.isArray(data.results)) resultCount += data.results.length;
    }
    return { queryCount, summary: resultCount > 0 ? `${resultCount} candidates` : undefined };
  }

  if (toolName === 'lspGetSemantics') {
    const paths: string[] = [];
    let refCount = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      // definition: data.location.uri
      if (data.location && typeof data.location === 'object') {
        const loc = data.location as Record<string, unknown>;
        const uri = str(loc.uri);
        if (uri) paths.push(`${basename(uri.replace(/\?.*$/, ''))}:${loc.line ?? ''}`);
      }
      // references: data.references[]
      if (Array.isArray(data.references)) refCount += data.references.length;
      if (Array.isArray(data.symbols)) refCount += data.symbols.length;
    }
    return {
      queryCount,
      paths: paths.slice(0, 3),
      summary: refCount > 0 ? `${refCount} refs` : undefined,
    };
  }

  if (toolName === 'npmSearch') {
    const paths: string[] = [];
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      const name = str(data.name ?? data.packageName);
      const version = str(data.version);
      if (name) paths.push(version ? `${name}@${version}` : name);
    }
    return { queryCount, paths: paths.slice(0, 3) };
  }

  if (toolName === 'ghSearchPullRequests' || toolName === 'ghSearchIssues' || toolName === 'ghSearchCommits') {
    let count = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      if (Array.isArray(data.items)) count += data.items.length;
      else if (Array.isArray(data.prs)) count += data.prs.length;
      else if (Array.isArray(data.issues)) count += data.issues.length;
      else if (Array.isArray(data.commits)) count += data.commits.length;
    }
    return { queryCount, summary: count > 0 ? `${count} items` : undefined };
  }

  // Generic fallback: count results
  return { queryCount };
}

// ─── renderCall / renderResult builders ──────────────────────────────────────

export function buildOctocodeSingleRenderCall(
  toolName: string,
  args: unknown,
  theme?: PiTheme,
): RenderCallReturn {
  const summary = buildToolCallSummary(toolName, args);
  return buildToolView({
    name: toolName,
    state: 'request',
    segments: summary ? [{ text: summary, token: 'dim' }] : [],
  }, theme);
}

/** Build one operation/reasoning block per Octocode MCP query. */
export function buildOctocodeRenderCall(
  toolName: string,
  args: unknown,
  theme?: PiTheme,
): RenderCallReturn {
  return buildQueryCallBlocks(
    args,
    theme,
    (singleArgs) => buildOctocodeSingleRenderCall(toolName, singleArgs, theme),
  );
}

/** First non-empty, trimmed line of a result's text content (its error message or summary). */
/** Max visible cells of the inline `→ result` preview on a collapsed row. */
const RESULT_PREVIEW_MAX = 100;

function firstResultTextLine(result: ToolCallResult): string {
  const text = (result.content as Array<{ type: string; text: string }> | undefined)
    ?.find?.((p) => p?.type === 'text')?.text ?? '';
  return text.split('\n').map((line) => line.trim()).find(Boolean) ?? '';
}

export interface QueryResultRenderRow {
  index: number;
  status: 'success' | 'failed' | 'not-run';
  summary: string;
}

/** Extract only canonical query-envelope rows; provider `results[]` arrays do not qualify. */
export function extractQueryResultRows(result: ToolCallResult): QueryResultRenderRow[] {
  const details = result.details && typeof result.details === 'object'
    ? result.details as Record<string, unknown>
    : {};
  const values = Array.isArray(details['results']) ? details['results'] : [];
  const rows = values.flatMap((value): QueryResultRenderRow[] => {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return [];
    const row = value as Record<string, unknown>;
    const status = row['status'];
    if (typeof row['index'] !== 'number' || !['success', 'failed', 'not-run'].includes(String(status))) return [];
    return [{
      index: row['index'],
      status: status as QueryResultRenderRow['status'],
      summary: str(row['summary']) || (status === 'not-run' ? 'not run' : String(status)),
    }];
  });
  if (rows.length > 0) return rows;

  const text = (result.content as Array<{ type?: string; text?: string }> | undefined)
    ?.find?.((part) => part?.type === 'text')?.text ?? '';
  return text.split('\n').flatMap((line): QueryResultRenderRow[] => {
    const match = line.trim().match(/^(?:[✓✗○–]\s*)?\[(\d+)\]\s+(success|failed|not-run):\s*(.*)$/i);
    if (!match) return [];
    return [{
      index: Number(match[1]),
      status: match[2]!.toLowerCase() as QueryResultRenderRow['status'],
      summary: match[3]!.trim() || (match[2]!.toLowerCase() === 'not-run' ? 'not run' : match[2]!),
    }];
  });
}

function renderQueryResultRows(
  toolName: string,
  rows: QueryResultRenderRow[],
  theme?: PiTheme,
  queryRunType?: 'sequential' | 'parallel',
): RenderCallReturn {
  return makeCachedRenderer((width) => [
    ...(queryRunType
      ? buildToolView({
          name: toolName,
          state: 'neutral',
          segments: [
            { text: `${rows.length} queries`, token: 'count' },
            { text: queryRunType, token: queryRunType === 'parallel' ? 'link' : 'muted' },
          ],
        }, theme).render(width)
      : []),
    ...rows.flatMap((row) => buildToolView({
      name: toolName,
      state: row.status === 'success' ? 'success' : row.status === 'failed' ? 'error' : 'neutral',
      segments: [
        { text: `[${row.index}]`, token: 'dim' },
        { text: row.summary, token: row.status === 'success' ? 'success' : row.status === 'failed' ? 'error' : 'muted' },
      ],
    }, theme).render(width)),
  ]);
}

export function buildQueryResultRows(
  toolName: string,
  result: ToolCallResult,
  theme?: PiTheme,
): RenderCallReturn | undefined {
  const rows = extractQueryResultRows(result);
  const details = result.details && typeof result.details === 'object' ? result.details as Record<string, unknown> : {};
  const queryRunType = details['queryRunType'] === 'parallel' || details['queryRunType'] === 'sequential'
    ? details['queryRunType'] as 'parallel' | 'sequential'
    : undefined;
  return rows.length > 0 ? renderQueryResultRows(toolName, rows, theme, queryRunType) : undefined;
}

function buildProviderQueryResultRows(
  toolName: string,
  result: ToolCallResult,
  theme?: PiTheme,
): RenderCallReturn | undefined {
  const details = result.details && typeof result.details === 'object'
    ? result.details as Record<string, unknown>
    : {};
  const values = Array.isArray(details['results']) ? details['results'] : [];
  if (values.length < 2) return undefined;

  const rows = values.map((value, index): QueryResultRenderRow => {
    const record = value && typeof value === 'object' && !Array.isArray(value)
      ? value as Record<string, unknown>
      : {};
    const data = record['data'] && typeof record['data'] === 'object' && !Array.isArray(record['data'])
      ? record['data'] as Record<string, unknown>
      : {};
    const error = str(record['error'] ?? data['error']);
    const failed = Boolean(error) || ['error', 'failed'].includes(str(record['status']).toLowerCase());
    const stats = buildResultStats(toolName, { results: [value] });
    const summary = error || [
      stats.summary,
      stats.paths?.join(', '),
      stats.previews?.join(' | '),
    ].filter(Boolean).join(' · ') || 'ok';
    return {
      index: typeof record['index'] === 'number' ? record['index'] : index,
      status: failed ? 'failed' : 'success',
      summary,
    };
  });
  return renderQueryResultRows(toolName, rows, theme);
}

/** Build the renderResult component for any octocode tool. */
export function buildOctocodeRenderResult(
  toolName: string,
  result: ToolCallResult,
  opts: { expanded?: boolean; isPartial?: boolean },
  theme?: PiTheme,
  context?: RenderContext,
): RenderCallReturn {
  if (opts.isPartial) {
    return buildToolView(() => ({ name: toolName, state: 'running', status: CLI_STATUS_TEXT.running }), theme);
  }

  const queryRows = buildQueryResultRows(toolName, result, theme);
  if (queryRows && extractQueryResultRows(result).length > 1) return queryRows;
  const providerRows = buildProviderQueryResultRows(toolName, result, theme);
  if (providerRows) return providerRows;

  // Pi ignores isError in the returned ToolCallResult value and instead sets a
  // system-level context.isError when execute() throws or the call is rejected
  // (e.g. schema validation). Honor both so an error row never renders as a
  // misleading success/empty row.
  const isError = Boolean(result.isError) || Boolean(context?.isError);
  // On error, surface the actual failure text — execute() error results carry
  // the message in the first text content line — so the row explains WHY it
  // failed instead of showing a bare error glyph with no data.
  if (isError) {
    const errText = firstResultTextLine(result);
    const segments: InlineSegment[] = errText
      ? [{ text: truncatePlainToWidth(errText, 200), token: 'error' }]
      : [];
    if (!opts.expanded) return buildToolView({ name: toolName, state: 'error', segments }, theme);
    const text = (result.content as Array<{ type: string; text: string }>)?.find?.((p) => p.type === 'text')?.text ?? '';
    const allLines = text.split('\n');
    const shown = allLines.slice(0, 25);
    return buildToolView({
      name: toolName,
      state: 'error',
      segments,
      body: [
        { text: 'response:', token: 'muted' },
        ...shown.map((line): ToolViewLine => ({ text: line, token: 'error' })),
      ],
      hint: allLines.length > shown.length ? `${allLines.length - shown.length} more lines hidden in this view` : undefined,
    }, theme);
  }

  const stats = buildResultStats(toolName, result.details);

  // Summary (counts) stays muted; paths get the dedicated `path` colour so a
  // glance separates "what happened" from "which files". Painted as separate SGR
  // spans — safe under pi-tui width measurement (OSC 8 hyperlinks are not, so
  // clickable links are intentionally omitted from TUI rows).
  const summarySeg = stats.summary ?? '';
  const pathSeg = stats.paths && stats.paths.length > 0 ? stats.paths.join(', ') : '';

  const previewSeg = stats.previews && stats.previews.length > 0 ? stats.previews.join(' | ') : '';
  const segments: InlineSegment[] = [];
  if (summarySeg) segments.push({ text: summarySeg, token: 'count' });
  if (pathSeg) segments.push({ text: pathSeg, token: 'path' });
  if (previewSeg) segments.push({ text: `“${previewSeg}”`, token: 'dim' });
  // Every result row carries the result: when the tool reported no structured
  // preview, show the first line of its response (`→ …`) so the operator reads
  // the outcome inline instead of expanding the row (ctrl+o still shows all).
  if (!previewSeg) {
    const firstLine = firstResultTextLine(result);
    if (firstLine) segments.push({ text: `→ ${truncatePlainToWidth(firstLine, RESULT_PREVIEW_MAX)}`, token: 'dim' });
  }
  if (!opts.expanded) return buildToolView({ name: toolName, state: 'success', segments }, theme);
  const text = (result.content as Array<{ type: string; text: string }>)?.find?.((p) => p.type === 'text')?.text ?? '';
  const allLines = text.split('\n');
  const shown = allLines.slice(0, 25);
  return buildToolView({
    name: toolName,
    state: 'success',
    segments,
    body: [
      { text: 'response:', token: 'muted' },
      ...shown.map((line): ToolViewLine => ({ text: line, token: 'dim' })),
    ],
    hint: allLines.length > shown.length ? `${allLines.length - shown.length} more lines hidden in this view` : undefined,
  }, theme);
}
