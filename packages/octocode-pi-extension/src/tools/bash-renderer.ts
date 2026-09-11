import fs from 'node:fs';

import type { PiTheme, ToolCallResult } from '../types.js';
import { paint } from '../tui/palette.js';
import { truncateToWidth } from '../tui/width.js';
import { sanitizeBgTitle } from './bash-bg-tool.js';
import { buildToolView, makeComponentRenderer } from './render-helpers.js';

export const BASH_CONTEXT_MAX_CHARS = 4_000;
const BASH_COLLAPSED_LINES = 3;
const BASH_TOOL_DISPLAY_NAME = 'bash (Octocode)';

function smartBashView(text: string, maxChars: number): { lines: string[]; omittedChars: number } {
  if (text.length <= maxChars) return { lines: text.split('\n').filter((line) => line.length > 0), omittedChars: 0 };
  const markerReserve = 96;
  const retained = Math.max(2, maxChars - markerReserve);
  const headChars = Math.ceil(retained / 2);
  const tailChars = retained - headChars;
  const omittedChars = text.length - headChars - tailChars;
  const preview = [
    text.slice(0, headChars),
    `… ${omittedChars} chars hidden in UI only; complete bash output was delivered to the agent …`,
    text.slice(text.length - tailChars),
  ].join('\n');
  return { lines: preview.split('\n').filter((line) => line.length > 0), omittedChars };
}

/** Read a bounded head+tail window for the TUI without loading the full log. */
function readBashOutputForUi(file: string | undefined, maxChars: number): string {
  if (!file) return '';
  let fd: number | undefined;
  try {
    fd = fs.openSync(file, 'r');
    const size = fs.fstatSync(fd).size;
    const byteBudget = Math.max(512, maxChars);
    if (size <= byteBudget) {
      const bytes = Buffer.alloc(size);
      fs.readSync(fd, bytes, 0, size, 0);
      return bytes.toString('utf8');
    }
    const headBytes = Math.floor(byteBudget / 4);
    const tailBytes = byteBudget - headBytes;
    const head = Buffer.alloc(headBytes);
    const tail = Buffer.alloc(tailBytes);
    fs.readSync(fd, head, 0, headBytes, 0);
    fs.readSync(fd, tail, 0, tailBytes, Math.max(0, size - tailBytes));
    return `${head.toString('utf8')}\n… ${size - byteBudget} bytes hidden in UI; full output remains in ${file} …\n${tail.toString('utf8')}`;
  } catch {
    return '';
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
  }
}

export function renderBashCall(args: unknown, theme?: PiTheme) {
  const envelope = args && typeof args === 'object' ? (args as Record<string, unknown>) : {};
  const queries = Array.isArray(envelope['queries']) ? envelope['queries'] as Record<string, unknown>[] : [];
  const input = queries[0] ?? {};
  const action = typeof input['action'] === 'string' ? input['action'] : undefined;
  if (action) {
    const jobId = typeof input['jobId'] === 'string' ? input['jobId'] : '';
    return buildToolView({ name: BASH_TOOL_DISPLAY_NAME, state: 'request', segments: [
      { text: `bg · ${action}`, token: 'dim' },
      ...(jobId ? [{ text: jobId, token: 'muted' as const }] : []),
    ] }, theme);
  }
  const isBg = Boolean(input['background']);
  const command = typeof input['command'] === 'string' ? input['command'] : '(missing command)';
  const title = typeof input['title'] === 'string' ? input['title'] : undefined;
  if (isBg) {
    return buildToolView({ name: BASH_TOOL_DISPLAY_NAME, state: 'request', segments: [
      { text: 'bg · start', token: 'dim' },
      { text: sanitizeBgTitle(title ?? command).slice(0, 55), token: 'muted' },
    ] }, theme);
  }
  return buildToolView({ name: BASH_TOOL_DISPLAY_NAME, state: 'request', segments: [{ text: command, token: 'dim' }] }, theme);
}

export const renderBashResult = Object.assign(
  function renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
    if (opts.isPartial) {
      return buildToolView(() => ({ name: BASH_TOOL_DISPLAY_NAME, state: 'running', status: 'running…' }), theme);
    }
    const ok = !result.isError;
    const details = result.details as {
      code?: number | null;
      outputPath?: string;
      totalChars?: number;
      stdoutChars?: number;
      stderrChars?: number;
      queryRunType?: string;
      results?: Array<{ index: number; status: string; result?: { code?: number | null; outputPath?: string; totalChars?: number } }>;
    } | undefined;
    const queryResults = Array.isArray(details?.results) && details.results.length > 1
      ? details.results
      : null;

    if (queryResults) {
      const qCount = queryResults.length;
      return makeComponentRenderer((_props, { width }) => {
        const lines = buildToolView({
          name: BASH_TOOL_DISPLAY_NAME,
          state: ok ? 'success' : 'error',
          segments: [
            { text: `${qCount} quer${qCount === 1 ? 'y' : 'ies'}`, token: 'count' },
            { text: details?.queryRunType ?? 'sequential', token: 'muted' },
          ],
        }, theme).render(width);
        for (const qr of queryResults) {
          const qOk = qr.status === 'success';
          const qCode = qr.result?.code;
          const maxChars = Math.max(512, Math.floor(BASH_CONTEXT_MAX_CHARS / qCount));
          const combined = readBashOutputForUi(qr.result?.outputPath, maxChars);
          const allQLines = combined.split('\n').filter((line) => line.length > 0);
          lines.push(...buildToolView({
            name: `[${qr.index}]`,
            state: qOk ? 'success' : 'error',
            segments: [
              { text: `exit ${qCode ?? 'null'}`, token: qOk ? 'dim' : 'error' },
              { text: `${allQLines.length} line${allQLines.length === 1 ? '' : 's'}`, token: 'count' },
            ],
          }, theme).render(width));
          const expandedView = smartBashView(combined, maxChars);
          const shown = opts.expanded ? expandedView.lines : allQLines.slice(-BASH_COLLAPSED_LINES);
          const hidden = opts.expanded ? 0 : allQLines.length - shown.length;
          if (hidden > 0) {
            lines.push(truncateToWidth(paint(theme, 'muted', `    … ${hidden} more line${hidden === 1 ? '' : 's'}`), width));
          }
          for (const line of shown) {
            lines.push(truncateToWidth(qOk ? paint(theme, 'dim', `    ${line}`) : paint(theme, 'error', `    ${line}`), width));
          }
        }
        if (!opts.expanded) lines.push(truncateToWidth(paint(theme, 'muted', '  ctrl+o to expand full output'), width));
        return lines;
      }, undefined);
    }

    const detailText = readBashOutputForUi(details?.outputPath, BASH_CONTEXT_MAX_CHARS);
    const text = detailText || result.content
      .filter((content) => content.type === 'text')
      .map((content) => content.text)
      .join('\n');
    const allLines = text.split('\n').filter((line) => line.length > 0);
    const code = details?.code;
    const expandedView = smartBashView(text, BASH_CONTEXT_MAX_CHARS);
    const shown = opts.expanded ? expandedView.lines : allLines.slice(-BASH_COLLAPSED_LINES);
    const hidden = opts.expanded ? 0 : allLines.length - shown.length;
    return buildToolView({
      name: BASH_TOOL_DISPLAY_NAME,
      state: ok ? 'success' : 'error',
      segments: [
        { text: `exit ${code ?? 'null'}`, token: ok ? 'dim' : 'error' },
        { text: `${allLines.length} line${allLines.length === 1 ? '' : 's'}`, token: 'count' },
      ],
      body: shown.map((line) => ({ text: line, token: ok ? 'dim' : 'error' })),
      hint: hidden > 0 ? `${hidden} more line${hidden === 1 ? '' : 's'} hidden · ctrl+o expands` : undefined,
    }, theme);
  },
  { multiQueryAware: true },
);
