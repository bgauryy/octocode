import { c, bold, dim } from '../../utils/colors.js';

export interface LocalMatch {
  path?: string;
  matchCount?: number;
  totalOccurrences?: number;
  totalMatchedLines?: number;
  totalMatchRows?: number;
  returnedMatchRows?: number;
  matches?: Array<{
    value?: string;
    line?: number;
    count?: number;
    metavars?: Record<string, string[]>;
  }>;
}

export interface LocalPagination {
  totalFiles?: number;
  page?: number;
  totalPages?: number;
}

export interface LocalSearchResult {
  results?: Array<{
    data?: {
      files?: LocalMatch[];
      pagination?: LocalPagination;
      // Common per-file scalars are hoisted here when identical across files
      // (e.g. structural/AST results share one matchCount). Must be merged back.
      shared?: {
        matchCount?: number;
        totalOccurrences?: number;
        totalMatchedLines?: number;
        totalMatchRows?: number;
      };
    };
  }>;
}

interface RenderOptions {
  valuesOnly?: boolean;
}

function formatMetavars(
  metavars: Record<string, string[]> | undefined
): string {
  if (!metavars) return '';
  const parts = Object.entries(metavars)
    .filter(([, values]) => values.length > 0)
    .slice(0, 3)
    .map(([name, values]) => {
      const preview = values.join(', ').replace(/\s+/g, ' ').slice(0, 80);
      return `$${name}=${preview}`;
    });
  return parts.length > 0 ? `[${parts.join('; ')}]` : '';
}

/**
 * Renders localSearchCode hits (text or structural/AST — same envelope shape)
 * into a compact `path (n matches)` + `Lnn: snippet` listing. Shared by the
 * `grep` text and structural modes.
 */
export function renderLocalResults(
  sc: LocalSearchResult,
  limit: number,
  contextLines = 0,
  options: RenderOptions = {}
): string {
  const data = sc?.results?.[0]?.data;
  const pagination = data?.pagination;
  const files = data?.files ?? [];
  const sharedCount =
    data?.shared?.totalMatchRows ??
    data?.shared?.totalOccurrences ??
    data?.shared?.totalMatchedLines ??
    data?.shared?.matchCount;
  const total = pagination?.totalFiles ?? files.length;
  const lines: string[] = [];
  const shown = files.slice(0, limit);
  for (const f of shown) {
    // Prefer the per-file count; fall back to a hoisted shared count, then to
    // the number of returned matches. In --files-only mode none of these exist
    // (the tool returns matching paths with no counts), so omit the suffix
    // rather than printing a misleading "(0 matches)".
    const count =
      f.totalMatchRows ??
      f.totalOccurrences ??
      f.totalMatchedLines ??
      f.matchCount ??
      sharedCount ??
      f.matches?.length;
    const countSuffix = count != null ? `  ${dim(`(${count} matches)`)}` : '';
    if (!options.valuesOnly) {
      lines.push(`  ${c('cyan', bold(f.path ?? ''))}${countSuffix}`);
    }
    (f.matches ?? []).slice(0, 5).forEach(m => {
      const metavars = formatMetavars(m.metavars);
      const metaSuffix = metavars ? ` ${dim(metavars)}` : '';
      if (options.valuesOnly) {
        const snippet = (m.value ?? '').trim();
        const prefix = m.count !== undefined ? `${m.count}x  ` : '';
        lines.push(`  ${prefix}${snippet}${metaSuffix}`);
        return;
      }
      const physical = (m.value ?? '').split('\n');
      // With a context window the value is `before… + matchLine + after…`.
      // Number each physical line so the gutter aligns with the text it labels
      // (only the match line gets the `Lnn:` colon; context lines are dimmed)
      // instead of stamping the match's line number on the before-context row.
      if (contextLines > 0 && physical.length > 1 && m.line != null) {
        // Before-context count is deterministic: line numbers are contiguous,
        // so it is min(contextLines, matchLine - 1).
        const before = Math.min(contextLines, m.line - 1);
        const startLine = m.line - before;
        physical.forEach((text, k) => {
          const ln = startLine + k;
          const body = text.replace(/\s+$/, '');
          if (ln === m.line) {
            lines.push(`    ${c('yellow', `L${ln}:`)} ${body}${metaSuffix}`);
          } else {
            lines.push(`    ${dim(`L${ln} `)} ${dim(body)}`);
          }
        });
      } else {
        const lineNum = m.line != null ? m.line : '?';
        const snippet = (m.value ?? '').trim();
        lines.push(
          `    ${c('yellow', `L${lineNum}:`)} ${snippet}${metaSuffix}`
        );
      }
    });
  }
  if (total > shown.length) {
    lines.push(`\n  ${dim(`… ${total - shown.length} more files`)}`);
  }
  if (pagination?.totalPages && pagination.totalPages > 1) {
    lines.push(
      `\n  ${dim(`Page ${pagination.page ?? 1}/${pagination.totalPages} — use --page <n> to navigate`)}`
    );
  }
  if (lines.length === 0) lines.push(`  ${dim('No matches found.')}`);
  return lines.join('\n');
}
