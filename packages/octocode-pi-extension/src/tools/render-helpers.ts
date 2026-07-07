/**
 * Shared rendering utilities for Octocode Pi extension tool renderers.
 *
 * Centralises:
 *  - ANSI-aware line truncation (replaces 3 copies across the codebase)
 *  - Per-tool call-summary extraction (smart param display instead of raw JSON)
 *  - Per-tool result-stats extraction (counts, paths, match totals)
 *  - A tiny `makeRenderer` factory for the Component interface
 */

import type { PiTheme, RenderCallReturn, ToolCallResult } from '../types.js';

// ─── ANSI-safe width helpers ──────────────────────────────────────────────────

/** Matches CSI sequences (ESC [ … m) and 2-char ESC sequences. */
export const ANSI_ESC_RE = /\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])/g;

export function visibleWidth(str: string): number {
  return str.replace(ANSI_ESC_RE, '').length;
}

/**
 * Truncate `str` so its *visible* width (ANSI codes excluded) ≤ `maxWidth`.
 * Appends an ellipsis and an SGR reset so open colour sequences don't bleed.
 */
export function truncateToWidth(
  str: string,
  maxWidth: number,
  ellipsis = '\u2026',
): string {
  if (maxWidth <= 0) return '';
  if (visibleWidth(str) <= maxWidth) return str;
  const ellipsisLen = visibleWidth(ellipsis);
  const target = maxWidth - ellipsisLen;
  if (target <= 0) return ellipsis.slice(0, maxWidth);

  let visible = 0;
  let i = 0;
  while (i < str.length) {
    const esc = ANSI_ESC_RE.exec(str.slice(i));
    if (esc && esc.index === 0) {
      i += esc[0].length;
      ANSI_ESC_RE.lastIndex = 0;
      continue;
    }
    ANSI_ESC_RE.lastIndex = 0;
    if (visible >= target) break;
    visible++;
    i++;
  }
  return str.slice(0, i) + ellipsis + '\x1b[0m';
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
    const safeWord = word.length > maxWidth ? word.slice(0, maxWidth) : word;
    if (!current) {
      current = safeWord;
    } else {
      const candidate = `${current} ${safeWord}`;
      if (candidate.length <= maxWidth) {
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
export function makeRenderer(lines: (width: number) => string[]): RenderCallReturn {
  return {
    render: (width = 80) => lines(width).map((line) => truncateToWidth(line, width)),
    invalidate() { /* no-op */ },
  };
}

export function singleLineRenderer(rawLine: string): RenderCallReturn {
  return makeRenderer((w) => [truncateToWidth(rawLine, w)]);
}

// ─── Tool-call summary (replaces raw JSON dump in renderCall) ─────────────────

type QueryLike = Record<string, unknown>;

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
  if (p.length <= maxLen) return p;
  // keep last portion
  const short = '…' + p.slice(-(maxLen - 1));
  return short;
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
  const more = queries.length > 1 ? ` +${queries.length - 1}` : '';

  // ── GitHub tools ─────────────────────────────────────────────────────────
  if (toolName.startsWith('gh')) {
    const repo = [str(q.owner), str(q.repo)].filter(Boolean).join('/');

    if (toolName === 'ghSearchCode') {
      const kw = arr(q.keywords).join(' ');
      const lang = str(q.language);
      const fn = str(q.filename);
      const parts = [
        kw ? `"${kw}"` : '',
        fn ? `file:${fn}` : '',
        lang ? `lang:${lang}` : '',
        repo ? `in ${repo}` : '',
      ].filter(Boolean).join(' ');
      return (parts + more).trim();
    }

    if (toolName === 'ghSearchRepos') {
      const kw = arr(q.keywords).join(' ');
      const lang = str(q.language);
      return ([kw ? `"${kw}"` : '', lang ? `lang:${lang}` : ''].filter(Boolean).join(' ') + more).trim();
    }

    if (toolName === 'ghGetFileContent') {
      const p = str(q.path);
      const matchStr = str(q.matchString);
      const start = q.startLine != null ? `:${q.startLine}` : '';
      const end = q.endLine != null ? `-${q.endLine}` : '';
      const anchor = matchStr ? ` /${matchStr.slice(0, 20)}/` : start + end;
      return (`${repo}${p ? `:${p}` : ''}${anchor}` + more).trim();
    }

    if (toolName === 'ghViewRepoStructure') {
      const p = str(q.path);
      return (`${repo}${p && p !== '.' ? `/${p}` : ''}` + more).trim();
    }

    if (toolName === 'ghHistoryResearch') {
      const type = str(q.type) || 'prs';
      const prNum = q.prNumber != null ? `#${q.prNumber}` : '';
      return (`${repo} ${type}${prNum}` + more).trim();
    }

    if (toolName === 'ghCloneRepo') {
      const sp = str(q.sparsePath);
      return (`${repo}${sp ? `/${sp}` : ''}` + more).trim();
    }

    return (repo + more).trim();
  }

  // ── Local tools ───────────────────────────────────────────────────────────
  if (toolName.startsWith('local') || toolName === 'lspGetSemantics') {
    if (toolName === 'localSearchCode') {
      const kw = str(q.keywords);
      const p = str(q.path);
      const mode = str(q.mode);
      const modeTag = mode && mode !== 'paginated' ? `[${mode}] ` : '';
      return (`${modeTag}${kw ? `"${kw}"` : ''}${p ? ` in ${shortPath(p)}` : ''}` + more).trim();
    }

    if (toolName === 'localGetFileContent') {
      const p = str(q.path);
      const start = q.startLine != null ? `:${q.startLine}` : '';
      const end = q.endLine != null ? `-${q.endLine}` : '';
      const matchStr = str(q.matchString);
      const anchor = matchStr ? ` /${matchStr.slice(0, 20)}/` : start + end;
      return (shortPath(p) + anchor + more).trim();
    }

    if (toolName === 'localViewStructure') {
      const p = str(q.path);
      const depth = q.maxDepth != null ? ` depth:${q.maxDepth}` : '';
      return (shortPath(p) + depth + more).trim();
    }

    if (toolName === 'localFindFiles') {
      const p = str(q.path);
      const names = arr(q.names).join(', ');
      const pat = str(q.pathPattern);
      return (`${shortPath(p)}${names ? ` [${names}]` : ''}${pat ? ` ${pat}` : ''}` + more).trim();
    }

    if (toolName === 'localBinaryInspect') {
      const p = str(q.path);
      const mode = str(q.mode);
      return (`${basename(p)}${mode ? ` (${mode})` : ''}` + more).trim();
    }

    if (toolName === 'lspGetSemantics') {
      const sym = str(q.symbolName);
      const type = str(q.type) || 'definition';
      const uri = str(q.uri);
      const file = uri ? basename(uri.replace(/\?.*$/, '')) : '';
      const line = q.lineHint != null ? `:${q.lineHint}` : '';
      return (`${type}${sym ? ` "${sym}"` : ''}${file ? ` in ${file}${line}` : ''}` + more).trim();
    }

    // localBinaryInspect fallthrough
    const p = str(q.path);
    return (shortPath(p) + more).trim();
  }

  // ── npm ──────────────────────────────────────────────────────────────────
  if (toolName === 'npmSearch') {
    const pkg = str(q.packageName);
    return (pkg + more).trim();
  }

  // ── fallback: pick the 3 most informative string values ──────────────────
  const SKIP_KEYS = new Set(['id', 'reasoning', 'researchGoal', 'mainResearchGoal', 'resolveedPath']);
  const parts = Object.entries(q)
    .filter(([k]) => !SKIP_KEYS.has(k))
    .map(([, v]) => {
      const s = String(v ?? '');
      return s.length > 40 ? s.slice(0, 40) + '…' : s;
    })
    .filter(Boolean)
    .slice(0, 3);
  return (parts.join(' ') + more).trim();
}

// ─── Result stats (replaces generic "N items" in renderResult) ────────────────

export interface ResultStats {
  /** Total query count that produced results */
  queryCount?: number;
  /** Human-readable match/result total */
  summary?: string;
  /** Short file/repo paths to show inline */
  paths?: string[];
  /** Whether any result had an error */
  hasError?: boolean;
}

/**
 * Extract meaningful result stats from a tool's `details` object.
 * The structured output from octocode tools is typically:
 *   `{ results: [{ id, data: { ... tool-specific ... } }] }`
 */
export function buildResultStats(toolName: string, details: unknown): ResultStats {
  if (!details || typeof details !== 'object') return {};
  const d = details as Record<string, unknown>;

  const results = Array.isArray(d.results) ? (d.results as Record<string, unknown>[]) : [];
  const queryCount = results.length > 0 ? results.length : undefined;

  // Per-tool structured extraction
  if (toolName === 'ghSearchCode' || toolName === 'ghSearchRepos') {
    // data.items[] is the search result list; data.totalCount is the GH API total
    let total = 0;
    let repos: string[] = [];
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      if (typeof data.totalCount === 'number') total += data.totalCount;
      else if (Array.isArray(data.items)) total += data.items.length;
      if (toolName === 'ghSearchRepos' && Array.isArray(data.items)) {
        for (const item of (data.items as Record<string, unknown>[]).slice(0, 3)) {
          const name = str(item.fullName ?? item.name);
          if (name) repos.push(name);
        }
      }
    }
    return {
      queryCount,
      summary: total > 0 ? `${total} results` : undefined,
      paths: repos.length > 0 ? repos : undefined,
    };
  }

  if (toolName === 'ghGetFileContent') {
    const paths: string[] = [];
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      const p = str(data.path ?? data.filePath);
      if (p) paths.push(basename(p));
    }
    return { queryCount, paths: paths.slice(0, 4) };
  }

  if (toolName === 'ghViewRepoStructure') {
    let entryCount = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      if (typeof data.totalEntries === 'number') entryCount += data.totalEntries;
      else if (Array.isArray(data.files)) entryCount += data.files.length;
    }
    return { queryCount, summary: entryCount > 0 ? `${entryCount} entries` : undefined };
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

  if (toolName === 'localSearchCode') {
    let matchCount = 0;
    let fileCount = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      if (typeof data.totalMatches === 'number') matchCount += data.totalMatches;
      if (typeof data.totalFiles === 'number') fileCount += data.totalFiles;
      else if (Array.isArray(data.matches)) matchCount += data.matches.length;
    }
    const parts = [
      matchCount > 0 ? `${matchCount} matches` : '',
      fileCount > 0 ? `${fileCount} files` : '',
    ].filter(Boolean);
    return { queryCount, summary: parts.join(', ') || undefined };
  }

  if (toolName === 'localGetFileContent') {
    const paths: string[] = [];
    let lines = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      const p = str(data.path ?? data.resolvedPath);
      if (p) paths.push(basename(p));
      if (typeof data.totalLines === 'number') lines += data.totalLines;
    }
    return {
      queryCount,
      paths: paths.slice(0, 4),
      summary: lines > 0 ? `${lines} lines` : undefined,
    };
  }

  if (toolName === 'localViewStructure') {
    let entryCount = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      if (typeof data.totalEntries === 'number') entryCount += data.totalEntries;
      else if (Array.isArray(data.files)) entryCount += data.files.length;
    }
    return { queryCount, summary: entryCount > 0 ? `${entryCount} entries` : undefined };
  }

  if (toolName === 'localFindFiles') {
    let fileCount = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      if (Array.isArray(data.entries)) fileCount += data.entries.length;
      else if (typeof data.totalEntries === 'number') fileCount += data.totalEntries;
    }
    return { queryCount, summary: fileCount > 0 ? `${fileCount} files` : undefined };
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

  if (toolName === 'ghHistoryResearch') {
    let count = 0;
    for (const r of results) {
      const data = (r.data ?? {}) as Record<string, unknown>;
      if (Array.isArray(data.items)) count += data.items.length;
      else if (Array.isArray(data.prs)) count += data.prs.length;
      else if (Array.isArray(data.commits)) count += data.commits.length;
    }
    return { queryCount, summary: count > 0 ? `${count} items` : undefined };
  }

  // Generic fallback: count results
  return { queryCount };
}

// ─── renderCall / renderResult builders ──────────────────────────────────────

/** Build the renderCall component for any octocode tool. */
export function buildOctocodeRenderCall(
  toolName: string,
  args: unknown,
  theme?: PiTheme,
): RenderCallReturn {
  const summary = buildToolCallSummary(toolName, args);
  const nameStr = theme?.fg('toolTitle', theme.bold(toolName)) ?? toolName;
  const summaryStr = summary
    ? (theme?.fg('dim', summary) ?? summary)
    : '';
  const rawLine = summaryStr ? `${nameStr} ${summaryStr}` : nameStr;
  return singleLineRenderer(rawLine);
}

/** Build the renderResult component for any octocode tool. */
export function buildOctocodeRenderResult(
  toolName: string,
  result: ToolCallResult,
  opts: { expanded?: boolean; isPartial?: boolean },
  theme?: PiTheme,
): RenderCallReturn {
  if (opts.isPartial) {
    const running = theme?.fg('warning', `${toolName} running…`) ?? `${toolName} running…`;
    return singleLineRenderer(running);
  }

  const ok = !result.isError;
  const stats = buildResultStats(toolName, result.details);

  // Build header: ✓/✗ toolName · stat-summary
  const icon = theme?.fg(ok ? 'success' : 'error', ok ? '✓' : '✗') ?? (ok ? '✓' : '✗');
  const nameStr = theme?.fg('toolTitle', toolName) ?? toolName;

  const statParts: string[] = [];
  if (stats.summary) statParts.push(stats.summary);
  else if (stats.queryCount !== undefined && stats.queryCount > 1)
    statParts.push(`${stats.queryCount} queries`);
  if (stats.paths && stats.paths.length > 0)
    statParts.push(stats.paths.join(', '));

  const statStr = statParts.length > 0
    ? (theme?.fg('dim', ` · ${statParts.join(' · ')}`) ?? ` · ${statParts.join(' · ')}`)
    : '';

  const header = `${icon} ${nameStr}${statStr}`;

  if (!opts.expanded) {
    const hint = theme?.fg('dim', ' · expand for full output') ?? ' · expand for full output';
    return singleLineRenderer(`${header}${hint}`);
  }

  // Expanded: show up to 25 lines of text content + truncation notice
  const text = (result.content as Array<{ type: string; text: string }>)
    ?.find?.((p) => p.type === 'text')?.text ?? '';
  const MAX_LINES = 25;
  const allLines = text.split('\n');
  const shownLines = allLines.slice(0, MAX_LINES);
  const omitted = allLines.length - shownLines.length;

  return makeRenderer((width) => {
    const out: string[] = [truncateToWidth(header, width)];
    for (const line of shownLines) {
      out.push(truncateToWidth(theme?.fg('dim', line) ?? line, width));
    }
    if (omitted > 0) {
      out.push(
        truncateToWidth(
          theme?.fg('muted', `… ${omitted} more line${omitted === 1 ? '' : 's'} hidden (full output available to agent)`) ??
            `… ${omitted} more lines hidden`,
          width,
        ),
      );
    }
    return out;
  });
}

// ─── Memory tool renderers ────────────────────────────────────────────────────
//
// Memory tools have flat param shapes (no queries[]) so they need dedicated
// builders separate from the octocode direct-tool helpers above.

/** Extract a human-readable one-liner from a memory tool's call params. */
export function buildMemoryRenderCall(
  toolName: string,
  args: unknown,
  theme?: PiTheme,
): RenderCallReturn {
  const a = (args ?? {}) as Record<string, unknown>;
  const s = (v: unknown, max = 60): string => {
    const r = typeof v === 'string' ? v.trim() : '';
    return r.length > max ? r.slice(0, max - 1) + '\u2026' : r;
  };

  let summary = '';

  switch (toolName) {
    case 'memory_recall': {
      const q = s(a.query, 70);
      const label = s(a.label, 12);
      summary = [q, label ? `[${label}]` : ''].filter(Boolean).join(' ');
      break;
    }
    case 'memory_record': {
      const label = s(a.label, 12);
      const imp = typeof a.importance === 'number' ? `\u00b7${a.importance}` : '';
      const ctx = s(a.task_context, 55);
      summary = [label ? `[${label}${imp}]` : '', ctx].filter(Boolean).join(' ');
      break;
    }
    case 'memory_reflect': {
      const task = s(a.task, 55);
      const outcome = s(a.outcome, 8);
      summary = [task, outcome ? `(${outcome})` : ''].filter(Boolean).join(' ');
      break;
    }
    case 'memory_verify': {
      if (a.allPending) summary = 'allPending';
      else if (Array.isArray(a.task_ids)) summary = `${a.task_ids.length} task${a.task_ids.length === 1 ? '' : 's'}`;
      else if (typeof a.task_id === 'string') summary = a.task_id.slice(0, 20);
      if (a.status) summary += ` \u2192 ${s(a.status, 10)}`;
      break;
    }
    case 'memory_forget': {
      const parts: string[] = [];
      if (Array.isArray(a.tags) && a.tags.length) parts.push(`tags:[${(a.tags as string[]).join(', ')}]`);
      if (typeof a.max_importance === 'number') parts.push(`\u2264${a.max_importance}`);
      if (a.before) parts.push(`before:${s(a.before, 12)}`);
      if (a.dry_run) parts.push('dry_run');
      summary = parts.join(' ');
      break;
    }
    case 'memory_digest': {
      const parts: string[] = [];
      if (a.dry_run) parts.push('dry_run');
      if (a.export_doc) parts.push('export_doc');
      summary = parts.join(' ');
      break;
    }
    case 'memory_notify': {
      const kind = s(a.kind, 12);
      const subject = s(a.subject, 55);
      summary = [kind ? `[${kind}]` : '', subject].filter(Boolean).join(' ');
      break;
    }
    case 'memory_refine_get': {
      const state = s(a.state, 12);
      summary = state ? `state:${state}` : '';
      break;
    }
    // memory_workspace_status, memory_audit_unverified: no meaningful params
    default:
      break;
  }

  const nameStr = theme?.fg('toolTitle', theme.bold(toolName)) ?? toolName;
  const summaryStr = summary ? (theme?.fg('dim', summary) ?? summary) : '';
  const rawLine = summaryStr ? `${nameStr} ${summaryStr}` : nameStr;
  return singleLineRenderer(rawLine);
}

/** Parse JSON text content from a memory tool result and return a stat string. */
function parseMemoryStat(toolName: string, text: string): string {
  try {
    const data = JSON.parse(text) as Record<string, unknown>;

    switch (toolName) {
      case 'memory_recall': {
        const count = typeof data.count === 'number' ? data.count
          : Array.isArray(data.memories) ? (data.memories as unknown[]).length : 0;
        return `${count} memor${count === 1 ? 'y' : 'ies'}`;
      }
      case 'memory_record': {
        if (data.skipped) return 'skipped (similar exists)';
        const label = typeof data.label === 'string' ? ` [${data.label}]` : '';
        return `recorded${label}`;
      }
      case 'memory_reflect': {
        const outcome = typeof data.outcome === 'string' ? ` (${data.outcome})` : '';
        return `reflected${outcome}`;
      }
      case 'file_lock':
      case 'memory_file_lock': {
        if (data.type === 'lock') {
          const files = Array.isArray(data.files) ? data.files.length : 0;
          const expires = typeof data.expiresAt === 'string' ? ` until ${data.expiresAt}` : '';
          return `locked ${files} file${files === 1 ? '' : 's'}${expires}`;
        }
        if (data.type === 'status') {
          const locks = Array.isArray(data.locks) ? data.locks.length : 0;
          return `${locks} lock${locks === 1 ? '' : 's'}`;
        }
        if (data.type === 'renew') return `${data.locks_renewed ?? 0} renewed`;
        if (data.type === 'release') return `${data.locks_released ?? 0} released`;
        return '';
      }
      case 'memory_workspace_status': {
        const locks = Array.isArray(data.locks) ? data.locks.length : 0;
        const agents = Array.isArray(data.agents) ? data.agents.length : 0;
        const pending = typeof data.pending_tasks === 'number' ? data.pending_tasks : 0;
        const parts = [
          locks > 0 ? `${locks} lock${locks === 1 ? '' : 's'}` : '',
          agents > 0 ? `${agents} agent${agents === 1 ? '' : 's'}` : '',
          pending > 0 ? `${pending} pending` : '',
        ].filter(Boolean);
        return parts.length > 0 ? parts.join(', ') : 'no activity';
      }
      case 'memory_refine_get': {
        const count = Array.isArray(data.refinements) ? data.refinements.length
          : typeof data.count === 'number' ? data.count : 0;
        return `${count} refinement${count === 1 ? '' : 's'}`;
      }
      case 'memory_audit_unverified': {
        const count = typeof data.count === 'number' ? data.count
          : Array.isArray(data.pending) ? data.pending.length : 0;
        return `${count} pending task${count === 1 ? '' : 's'}`;
      }
      case 'memory_verify': {
        const count = typeof data.count === 'number' ? data.count
          : Array.isArray(data.results) ? data.results.length : 0;
        return `${count} verified`;
      }
      case 'memory_digest': {
        const archived = typeof data.archived === 'number' ? data.archived : 0;
        const pruned = typeof data.pruned === 'number' ? data.pruned : 0;
        const total = archived + pruned;
        return total > 0 ? `${total} cleaned (${archived} archived, ${pruned} pruned)` : 'nothing to clean';
      }
      case 'memory_forget': {
        const deleted = typeof data.deleted === 'number' ? data.deleted : 0;
        const previewed = typeof data.previewed === 'number' ? data.previewed : 0;
        if (data.dry_run) return `preview: ${previewed} would delete`;
        return `${deleted} deleted`;
      }
      case 'memory_notify':
        return 'posted';
      default:
        return '';
    }
  } catch {
    return '';
  }
}

/** Build renderResult Component for any memory tool. */
export function buildMemoryRenderResult(
  toolName: string,
  result: ToolCallResult,
  opts: { expanded?: boolean; isPartial?: boolean },
  theme?: PiTheme,
): RenderCallReturn {
  if (opts.isPartial) {
    const msg = theme?.fg('warning', `${toolName}\u2026`) ?? `${toolName}\u2026`;
    return singleLineRenderer(msg);
  }

  const ok = !result.isError;
  const icon = theme?.fg(ok ? 'success' : 'error', ok ? '\u2713' : '\u2717') ?? (ok ? '\u2713' : '\u2717');
  const nameStr = theme?.fg('toolTitle', toolName) ?? toolName;

  const text = (result.content as Array<{ type: string; text: string }>)
    ?.find?.((p) => p.type === 'text')?.text ?? '';
  const stat = ok ? parseMemoryStat(toolName, text) : '';
  const statStr = stat ? (theme?.fg('dim', ` \u00b7 ${stat}`) ?? ` \u00b7 ${stat}`) : '';

  const header = `${icon} ${nameStr}${statStr}`;

  if (!opts.expanded) {
    return singleLineRenderer(header);
  }

  // Expanded: pretty-print JSON for readability, then show up to 20 lines
  const MAX_LINES = 20;
  let displayLines = text.split('\n');
  if (displayLines.length === 1 && text.startsWith('{')) {
    try {
      displayLines = JSON.stringify(JSON.parse(text), null, 2).split('\n');
    } catch { /* keep original */ }
  }
  const shownLines = displayLines.slice(0, MAX_LINES);
  const omitted = displayLines.length - shownLines.length;

  return makeRenderer((width) => {
    const out: string[] = [truncateToWidth(header, width)];
    for (const line of shownLines) {
      out.push(truncateToWidth(theme?.fg('dim', line) ?? line, width));
    }
    if (omitted > 0) {
      out.push(
        truncateToWidth(
          theme?.fg('muted', `\u2026 ${omitted} more line${omitted === 1 ? '' : 's'}`) ?? `\u2026 ${omitted} more lines`,
          width,
        ),
      );
    }
    return out;
  });
}
