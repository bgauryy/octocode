import { truncateToWidth } from '../tui/width.js';
import { paint, ANSI_RESET } from '../tui/palette.js';
import { constants } from 'node:fs';
import { isUtf8 } from 'node:buffer';
import { access } from 'node:fs/promises';
import { computeLineDiff, computeLineDiffAsync, generateDiffArtifacts as nativeDiffArtifactsSync, generateDiffArtifactsAsync as nativeDiffArtifacts } from '@octocodeai/octocode-extension-rust';
import { CLI_STATUS_TEXT, cliStatusGlyph, cliStatusToken, cliToolTitle, ansiForToken } from '../tui/cli-design.js';
import type { ToolCallResult, PiTheme, RenderCallReturn } from '../types.js';
import { makeComponentRenderer } from './render-helpers.js';
import { withFileMutationQueue, checkReadState, type ReadStateCheck } from './file-state.js';
import { assertFileContentSize, replaceNativeFile } from './native-files.js';
import { countMutationLines, createCommittedMutationReceipt, finishFileMutation } from './file-mutation-receipt.js';
import { normalizeToLF, restoreEditedText, assertWellFormedText } from './file-text.js';
import { prepareFileMutationTarget, assertFileMutationTargetCurrent, rethrowFileMutationConflict, type FileMutationTarget } from './file-mutation-target.js';


type MatchMode = 'exact' | 'normalized' | 'lineRange';

export interface EditQuery {
  path: string;
  edits: EditOperation[];
  requireRecentRead?: boolean;
}

export interface EditOperation {
  oldText?: string;
  newText: string;
  /** Line-array alternative to newText for lineRange mode (VS Code LineReplacement pattern).
   *  Each element is one line without \n. Joined with \n; trailing \n added when range had one.
   *  Eliminates trailing-newline ambiguity. Mutually exclusive with newText when set. */
  newLines?: string[];
  replaceAll?: boolean;
  reasoning?: string;
  matchMode?: MatchMode;
  startLine?: number;
  endLine?: number;
}

interface MatchedReplacement {
  editIndex: number;
  start: number;
  end: number;
  newText: string;
  mode: MatchMode;
}

interface AppliedEditResult {
  changes: MatchedReplacement[];
  baseContent: string;
  newContent: string;
  replacements: number;
  firstChangedLine?: number;
  usedModes: MatchMode[];
  edits: AppliedEditEvidence[];
}

interface AppliedEditEvidence {
  editIndex: number;
  // 1-based line range in the ORIGINAL (pre-edit) file.
  startLine: number;
  endLine: number;
  mode: MatchMode;
  // Removed text fragments (the oldText segments), split by line.
  removedLines: string[];
  // Added text fragments (the newText), split by line.
  addedLines: string[];
}

export interface PreparedEdit {
  target: FileMutationTarget;
  requestPath: string;
  absolutePath: string;
  edits: EditOperation[];
  finalContent: string;
  result: AppliedEditResult;
  readState: ReadStateCheck;
  diff: string;
  patch: string;
}

interface RenderableEditFile {
  path: string;
  edits?: Array<AppliedEditEvidence>;
}

function stripBom(text: string): { bom: string; text: string } {
  return text.startsWith('\uFEFF') ? { bom: '\uFEFF', text: text.slice(1) } : { bom: '', text };
}


function findOccurrences(content: string, needle: string): number[] {
  if (needle.length === 0) return [];
  const indices: number[] = [];
  let index = content.indexOf(needle);
  while (index !== -1) {
    indices.push(index);
    index = content.indexOf(needle, index + 1);
  }
  return indices;
}

function firstChangedLine(oldContent: string, newContent: string): number | undefined {
  if (oldContent === newContent) return undefined;
  const oldLines = oldContent.split('\n');
  const newLines = newContent.split('\n');
  const max = Math.max(oldLines.length, newLines.length);
  for (let i = 0; i < max; i++) {
    if (oldLines[i] !== newLines[i]) return i + 1;
  }
  return undefined;
}

function normalizeForFuzzyMatch(text: string): string {
  return text
    .normalize('NFKC')
    .replace(/[\u2018\u2019\u201A\u201B]/g, "'")
    .replace(/[\u201C\u201D\u201E\u201F]/g, '"')
    .replace(/[\u2010\u2011\u2012\u2013\u2014\u2015\u2212]/g, '-')
    .replace(/[\u00A0\u2002-\u200A\u202F\u205F\u3000]/g, ' ')
    // Unescape template-literal backtick escapes so oldText with `x` matches file with \`x\`.
    .replace(/\\`/g, '`')
    .split('\n')
    .map((line) => line.trim().replace(/[ \t]+/g, ' '))
    .join('\n');
}

function lineSpans(content: string): Array<{ start: number; end: number; line: string }> {
  const lines = content.split('\n');
  let cursor = 0;
  return lines.map((line, index) => {
    const hasNewline = index < lines.length - 1;
    const end = cursor + line.length + (hasNewline ? 1 : 0);
    const span = { start: cursor, end, line: hasNewline ? `${line}\n` : line };
    cursor = end;
    return span;
  });
}

function previewLine(line: string): string {
  const visible = line
    .replace(/^ +/u, (spaces) => '·'.repeat(spaces.length))
    .replace(/^\t+/u, (tabs) => '→'.repeat(tabs.length));
  return visible.length > 160 ? `${visible.slice(0, 160)}…` : visible;
}

function getSearchAnchor(oldText: string): string | null {
  const lines = normalizeToLF(oldText)
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .sort((a, b) => b.length - a.length);
  return lines[0] ?? null;
}

function buildNearbyLineHints(content: string, oldText: string): string[] {
  const anchor = getSearchAnchor(oldText);
  if (!anchor) return [];
  const candidates = [anchor, anchor.slice(0, 80), anchor.slice(0, 40)]
    .map((candidate) => candidate.trim())
    .filter((candidate, index, all) => candidate.length >= 8 && all.indexOf(candidate) === index);
  const lines = content.split('\n');
  const hints: string[] = [];
  for (const candidate of candidates) {
    for (let i = 0; i < lines.length; i++) {
      if (!lines[i]!.includes(candidate)) continue;
      hints.push(`line ${i + 1}: ${previewLine(lines[i]!)}`);
      if (hints.length >= 5) return hints;
    }
  }
  return hints;
}

function notFoundError(filePath: string, editIndex: number, totalEdits: number, oldText: string, content: string): Error {
  const target = totalEdits === 1 ? 'oldText' : `edits[${editIndex}].oldText`;
  const anchor = getSearchAnchor(oldText);
  const hints = buildNearbyLineHints(content, oldText);
  const details = [
    `Could not find ${target} in ${filePath}.`,
    'The custom Octocode edit tool matches current file text exactly unless matchMode is explicitly set.',
    'Likely causes: the file changed since it was last read, oldText came from a sibling/generated file, or whitespace/indentation differs.',
    anchor ? `Longest non-empty oldText line: ${JSON.stringify(previewLine(anchor))}` : undefined,
    hints.length > 0 ? `Current file lines containing a similar anchor:\n${hints.map((hint) => `  - ${hint}`).join('\n')}` : undefined,
    'Re-read the target range and retry with a smaller unique oldText copied from the current file.',
  ].filter((line): line is string => Boolean(line));
  return new Error(details.join('\n'));
}

function duplicateError(filePath: string, editIndex: number, totalEdits: number, occurrences: number): Error {
  const target = totalEdits === 1 ? 'oldText' : `edits[${editIndex}].oldText`;
  return new Error(
    `Found ${occurrences} occurrences of ${target} in ${filePath}. ` +
      'The text must be unique unless replaceAll:true is set for this edit. ' +
      'Provide more surrounding context or intentionally use replaceAll:true.',
  );
}

function assertIntegerLine(value: unknown, name: string, editIndex: number): number {
  if (!Number.isInteger(value) || Number(value) < 1) {
    throw new Error(`Edit tool input is invalid. edits[${editIndex}].${name} must be a positive integer.`);
  }
  return Number(value);
}

function validateOperation(edit: unknown, index: number): EditOperation {
  if (!edit || typeof edit !== 'object' || Array.isArray(edit)) {
    throw new Error(`Edit tool input is invalid. edits[${index}] must be an object.`);
  }
  const item = edit as Record<string, unknown>;
  const allowed = new Set(['oldText', 'newText', 'newLines', 'replaceAll', 'reasoning', 'matchMode', 'startLine', 'endLine']);
  const extra = Object.keys(item).filter(key => !allowed.has(key));
  if (extra.length) throw new Error(`Edit tool input is invalid. edits[${index}] has unknown fields: ${extra.join(', ')}.`);
  const matchMode = (item['matchMode'] ?? 'exact') as MatchMode;
  if (!['exact', 'normalized', 'lineRange'].includes(matchMode)) {
    throw new Error(`Edit tool input is invalid. edits[${index}].matchMode must be exact, normalized, or lineRange.`);
  }
  if (matchMode !== 'lineRange' && (item['startLine'] !== undefined || item['endLine'] !== undefined)) {
    throw new Error('startLine and endLine require matchMode:"lineRange".');
  }
  const hasNewLines = Array.isArray(item['newLines']);
  if (!hasNewLines && typeof item['newText'] !== 'string') {
    throw new Error(`Edit tool input is invalid. edits[${index}].newText must be a string (or supply newLines[] for lineRange).`);
  }
  if (hasNewLines && (item['newLines'] as unknown[]).some((l) => typeof l !== 'string')) {
    throw new Error(`Edit tool input is invalid. edits[${index}].newLines must be an array of strings.`);
  }
  if (hasNewLines && item['newText'] !== undefined) {
    throw new Error(`Edit tool input is invalid. edits[${index}]: newLines and newText are mutually exclusive.`);
  }
  if (hasNewLines && (item['matchMode'] ?? 'exact') !== 'lineRange') {
    throw new Error(`Edit tool input is invalid. edits[${index}].newLines requires matchMode:"lineRange".`);
  }
  if (item['oldText'] !== undefined && typeof item['oldText'] !== 'string') {
    throw new Error(`Edit tool input is invalid. edits[${index}].oldText must be a string.`);
  }
  if (!hasNewLines) assertWellFormedText(item['newText'] as string, 'newText');
  if (typeof item['oldText'] === 'string') assertWellFormedText(item['oldText'], 'oldText');
  if (matchMode !== 'lineRange' && (typeof item['oldText'] !== 'string' || item['oldText'].length === 0)) {
    throw new Error(`Edit tool input is invalid. edits[${index}].oldText must be a non-empty string unless matchMode:"lineRange" is used.`);
  }
  if (item['oldText'] === '' && matchMode !== 'lineRange') {
    throw new Error(`Edit tool input is invalid. edits[${index}].oldText must not be empty; omit oldText when using matchMode:"lineRange".`);
  }
  if (item['replaceAll'] !== undefined && typeof item['replaceAll'] !== 'boolean') {
    throw new Error(`Edit tool input is invalid. edits[${index}].replaceAll must be a boolean.`);
  }
  const operation: EditOperation = {
    oldText: matchMode === 'lineRange' && item['oldText'] === '' ? undefined : item['oldText'] as string | undefined,
    newText: (item['newText'] as string | undefined) ?? '',
    newLines: hasNewLines ? (item['newLines'] as string[]) : undefined,
    replaceAll: item['replaceAll'] === true,
    matchMode,
  };
  if (matchMode === 'lineRange') {
    operation.startLine = assertIntegerLine(item['startLine'], 'startLine', index);
    operation.endLine = assertIntegerLine(item['endLine'], 'endLine', index);
    if (operation.endLine < operation.startLine) {
      throw new Error(`Edit tool input is invalid. edits[${index}].endLine must be >= startLine.`);
    }
    if (operation.replaceAll) {
      throw new Error(`Edit tool input is invalid. edits[${index}].replaceAll cannot be used with matchMode:"lineRange".`);
    }
  }
  return operation;
}

export function validateEditQuery(item: Record<string, unknown>, index: number): EditQuery {
  if (item['requireRecentRead'] !== undefined && typeof item['requireRecentRead'] !== 'boolean') {
    throw new Error('requireRecentRead must be a boolean.');
  }
  if (typeof item['path'] !== 'string' || item['path'].trim().length === 0) {
    throw new Error(`Edit tool input is invalid. queries[${index}].path must be a non-empty string.`);
  }
  if (!Array.isArray(item['edits']) || (item['edits'] as unknown[]).length === 0) {
    throw new Error(`Edit tool input is invalid. queries[${index}].edits must contain at least one replacement.`);
  }
  return {
    path: item['path'],
    requireRecentRead: item['requireRecentRead'] === true,
    edits: (item['edits'] as unknown[]).map(validateOperation),
  };
}

function exactReplacements(content: string, edit: EditOperation, editIndex: number, totalEdits: number, filePath: string): MatchedReplacement[] {
  const oldText = normalizeToLF(edit.oldText ?? '');
  const occurrences = findOccurrences(content, oldText);
  if (occurrences.length === 0) throw notFoundError(filePath, editIndex, totalEdits, oldText, content);
  if (!edit.replaceAll && occurrences.length > 1) throw duplicateError(filePath, editIndex, totalEdits, occurrences.length);
  let end = -1;
  const selected = edit.replaceAll ? occurrences.filter(start => {
    if (start < end) return false;
    end = start + oldText.length;
    return true;
  }) : [occurrences[0]!];
  return selected.map((start) => ({
    editIndex,
    start,
    end: start + oldText.length,
    newText: normalizeToLF(edit.newText),
    mode: 'exact' as const,
  }));
}

function normalizedReplacements(content: string, spans: ReturnType<typeof lineSpans>, edit: EditOperation, editIndex: number, totalEdits: number, filePath: string): MatchedReplacement[] {
  const oldText = normalizeToLF(edit.oldText ?? '');
  const normalizedOld = normalizeForFuzzyMatch(oldText);
  const oldLineCount = oldText.split('\n').length - (oldText.endsWith('\n') ? 1 : 0);
  const matches: MatchedReplacement[] = [];
  for (let i = 0; i <= spans.length - oldLineCount; i++) {
    const candidateWithEnding = spans.slice(i, i + oldLineCount).map((span) => span.line).join('');
    const keepsTrailingNewline = oldText.endsWith('\n');
    const candidate = !keepsTrailingNewline && candidateWithEnding.endsWith('\n')
      ? candidateWithEnding.slice(0, -1)
      : candidateWithEnding;
    if (normalizeForFuzzyMatch(candidate) === normalizedOld) {
      const spanEnd = spans[i + oldLineCount - 1]!.end;
      matches.push({
        editIndex,
        start: spans[i]!.start,
        end: !keepsTrailingNewline && candidateWithEnding.endsWith('\n') ? spanEnd - 1 : spanEnd,
        newText: normalizeToLF(edit.newText),
        mode: 'normalized',
      });
    }
  }
  if (matches.length === 0) throw notFoundError(filePath, editIndex, totalEdits, oldText, content);
  if (!edit.replaceAll && matches.length > 1) throw duplicateError(filePath, editIndex, totalEdits, matches.length);
  return edit.replaceAll ? matches : [matches[0]!];
}

function lineRangeReplacement(content: string, spans: ReturnType<typeof lineSpans>, edit: EditOperation, editIndex: number, filePath: string): MatchedReplacement[] {
  const startLine = edit.startLine!;
  const endLine = edit.endLine!;
  if (startLine > spans.length || endLine > spans.length) {
    throw new Error(`edits[${editIndex}] line range ${startLine}-${endLine} is outside ${filePath} (${spans.length} lines).`);
  }
  const start = spans[startLine - 1]!.start;
  const end = spans[endLine - 1]!.end;
  const current = content.slice(start, end);
  if (edit.oldText !== undefined && normalizeToLF(edit.oldText) !== current) {
    throw new Error(`edits[${editIndex}] oldText does not match the requested line range in ${filePath}. Re-read the target range.`);
  }
  // newLines[] (VS Code LineReplacement pattern): each element is one line without \n.
  // Eliminates trailing-newline ambiguity entirely. Falls back to newText with auto-repair.
  let newText: string;
  if (edit.newLines !== undefined) {
    newText = edit.newLines.join('\n');
    if (current.endsWith('\n') && edit.newLines.length > 0) newText += '\n';
  } else {
    const normalizedNew = normalizeToLF(edit.newText);
    newText = current.endsWith('\n') && !normalizedNew.endsWith('\n')
      ? normalizedNew + '\n'
      : normalizedNew;
  }
  return [{ editIndex, start, end, newText, mode: 'lineRange' }];
}

export function applyCustomEditsToContent(content: string, edits: EditOperation[], filePath: string): AppliedEditResult {
  // Compute line spans once; shared by all per-mode helpers and the evidence builder.
  const spans = lineSpans(content);
  const replacements: MatchedReplacement[] = [];
  for (let editIndex = 0; editIndex < edits.length; editIndex++) {
    const edit = edits[editIndex]!;
    const mode = edit.matchMode ?? 'exact';
    if (mode === 'lineRange') replacements.push(...lineRangeReplacement(content, spans, edit, editIndex, filePath));
    else if (mode === 'normalized') replacements.push(...normalizedReplacements(content, spans, edit, editIndex, edits.length, filePath));
    else replacements.push(...exactReplacements(content, edit, editIndex, edits.length, filePath));
  }

  replacements.sort((a, b) => a.start - b.start || a.end - b.end);
  for (let i = 1; i < replacements.length; i++) {
    const previous = replacements[i - 1]!;
    const current = replacements[i]!;
    if (previous.end > current.start) {
      throw new Error(
        `edits[${previous.editIndex}] and edits[${current.editIndex}] overlap in ${filePath}. ` +
          'Merge them into one edit or target disjoint regions.',
      );
    }
  }

  let newContent = content;
  for (let i = replacements.length - 1; i >= 0; i--) {
    const replacement = replacements[i]!;
    newContent = `${newContent.slice(0, replacement.start)}${replacement.newText}${newContent.slice(replacement.end)}`;
  }

  if (newContent === content) {
    throw new Error(`No changes made to ${filePath}. The replacement produced identical content.`);
  }

  // Build per-edit evidence: group replacements by editIndex, compute original-file
  // line ranges from byte offsets, and derive removed/added line fragments.
  // Reuse the `spans` computed at the top of applyCustomEditsToContent.
  const spanLines = spans;
  const byteToLineRange = (start: number, end: number): { startLine: number; endLine: number } => {
    // O(log N) binary search: find the last span with span.start <= value.
    if (spanLines.length === 0) return { startLine: 1, endLine: 1 };
    const bisectLastLE = (value: number): number => {
      let lo = 0, hi = spanLines.length - 1;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1; // upper midpoint avoids infinite loop
        if (spanLines[mid]!.start <= value) lo = mid; else hi = mid - 1;
      }
      return lo;
    };
    const startLine = bisectLastLE(start) + 1;
    // end is an exclusive byte offset; find the span owning byte end-1.
    const endLine = Math.max(startLine, (end > 0 ? bisectLastLE(end - 1) : 0) + 1);
    return { startLine, endLine };
  };
  const editEvidenceMap = new Map<number, AppliedEditEvidence>();
  // Split text into display lines, stripping a single trailing newline so a line
  // that includes its own newline boundary doesn't produce a phantom empty line.
  // removedLines must reflect the ACTUAL bytes removed from the ORIGINAL file
  // (content.slice), not edit.oldText — which for normalized/lineRange matching
  // can differ from the original by normalization (e.g. ﬁ ligature, CRLF).
  const toLines = (text: string): string[] => {
    const stripped = text.endsWith('\n') ? text.slice(0, -1) : text;
    return stripped.split('\n');
  };
  for (const r of replacements) {
    const removedText = normalizeToLF(content.slice(r.start, r.end));
    const range = byteToLineRange(r.start, r.end);
    const existing = editEvidenceMap.get(r.editIndex);
    if (existing) {
      // Multiple occurrences (replaceAll): widen the line range + accumulate fragments.
      existing.startLine = Math.min(existing.startLine, range.startLine);
      existing.endLine = Math.max(existing.endLine, range.endLine);
      existing.removedLines.push(...toLines(removedText));
      existing.addedLines.push(...toLines(r.newText));
    } else {
      editEvidenceMap.set(r.editIndex, {
        editIndex: r.editIndex,
        startLine: range.startLine,
        endLine: range.endLine,
        mode: r.mode,
        removedLines: toLines(removedText),
        addedLines: toLines(r.newText),
      });
    }
  }
  const editEvidence = [...editEvidenceMap.values()].sort((a, b) => a.editIndex - b.editIndex);

  return {
    changes: replacements,
    baseContent: content,
    newContent,
    replacements: replacements.length,
    firstChangedLine: firstChangedLine(content, newContent),
    usedModes: [...new Set(replacements.map((replacement) => replacement.mode))],
    edits: editEvidence,
  };
}

interface DiffOp { type: 'same' | 'add' | 'remove'; line: string }
const evidenceDiffs = new WeakMap<AppliedEditEvidence, DiffOp[]>();

// Myers line diff is O((N+M)·D). Agent edits are almost always small D on large
// files, so this stays fast where the old LCS DP (O(N·M) time+memory) stalled
// (~230ms at 3k lines) and forced MAX_DIFF_LINES=6000 omit. Cap only for
// pathological total size / memory, not for ordinary mid-size source files.
const MAX_DIFF_LINES = 200_000;

function diffTooLarge(oldContent: string, newContent: string): boolean {
  // Cheap length pre-check avoids splitting huge buffers just to count lines.
  if (oldContent.length + newContent.length > 32 * 1024 * 1024) return true;
  return oldContent.split('\n').length + newContent.split('\n').length > MAX_DIFF_LINES;
}

/** Native diff for synchronous rendering; preparation uses the async worker. */
export function diffOps(oldContent: string, newContent: string): DiffOp[] {
  return computeLineDiff(oldContent, newContent).map((op) => ({ type: op.opType, line: op.line }));
}

/**
 * NO_COLOR (https://no-color.org) opt-out for the raw diff SGR codes. Only the
 * explicit env flag disables them — not TTY detection — because `coloredDiff`
 * and the Changes: block are deliberately colored even when the session is
 * piped (details.diff carries the plain twin for uncolored consumers).
 */
function diffColorsDisabled(env: NodeJS.ProcessEnv = process.env): boolean {
  const flag = env['NO_COLOR'];
  return flag !== undefined && flag !== '' && flag !== '0' && flag.toLowerCase() !== 'false';
}

function colorDiffLine(line: string): string {
  if (diffColorsDisabled()) return line;
  // Raw SGR twin of TOKEN.diffAdd / diffRemove (cli-design owns the fallback map).
  if (line.startsWith('+ ')) return `${ansiForToken('diffAdd')}${line}${ANSI_RESET}`;
  if (line.startsWith('- ')) return `${ansiForToken('diffRemove')}${line}${ANSI_RESET}`;
  return line;
}

function colorDiffString(diff: string): string {
  return diff.split('\n').map(colorDiffLine).join('\n');
}

/** Build diff + unified patch from a single Myers pass (no double O(ND)). */
export function generateDiffArtifacts(
  filePath: string,
  oldContent: string,
  newContent: string,
): { diff: string; patch: string } {
  if (diffTooLarge(oldContent, newContent)) {
    return {
      diff: '(diff omitted: file too large — see the per-edit changes in details)',
      patch: `--- ${filePath}\n+++ ${filePath}\n@@ patch omitted: file too large @@\n`,
    };
  }
  return nativeDiffArtifactsSync(filePath, oldContent, newContent);
}

async function generateDiffArtifactsAsync(filePath: string, oldContent: string, newContent: string) {
  if (diffTooLarge(oldContent, newContent)) return generateDiffArtifacts(filePath, oldContent, newContent);
  return nativeDiffArtifacts(filePath, oldContent, newContent);
}

const EDIT_TOOL_DISPLAY_NAME = 'edit (Octocode)';

function changesSuffix(prepared: PreparedEdit[]): string {
  const blocks = prepared.map((item) => `# ${item.requestPath}\n${colorDiffString(item.diff)}`);
  return `\nChanges:\n${blocks.join('\n')}`;
}

function renderEditDiffItems(files: RenderableEditFile[], theme?: PiTheme): Array<{ text: string; truncate: boolean }> {
  const items: Array<{ text: string; truncate: boolean }> = [];
  for (const file of files) {
    items.push({ text: paint(theme, 'path', `  ${file.path}`), truncate: true });
    for (const edit of file.edits ?? []) {
      let ops = evidenceDiffs.get(edit);
      if (!ops) {
        ops = diffOps(edit.removedLines.join('\n'), edit.addedLines.join('\n')).filter(op => op.type !== 'same');
        evidenceDiffs.set(edit, ops);
      }
      for (const op of ops) {
        if (op.type === 'same') continue;
        const label = op.type === 'remove' ? '- ' : '+ ';
        const color = op.type === 'remove' ? 'diffRemove' : 'diffAdd';
        // 4-space indent is OUTSIDE theme.fg so the coloured substring
        // `<color>- text</color>` is preserved for test assertions and renderers
        // that match on the coloured part only.
        items.push({ text: `    ${paint(theme, color, `${label}${op.line}`)}`, truncate: true });
      }
    }
  }
  return items;
}

function renderCollapsedEditDiffLines(header: string, files: RenderableEditFile[], theme?: PiTheme): RenderCallReturn {
  const diffItems = renderEditDiffItems(files, theme);
  const maxPreviewLines = 10;
  const previewItems = diffItems;
  const shown = previewItems.slice(0, maxPreviewLines);
  const omitted = previewItems.length - shown.length;
  return makeComponentRenderer((_props, { width: width }) => [
    truncateToWidth(header, width),
    ...shown.map((item) => truncateToWidth(item.text, width)),
    ...(omitted > 0 ? [truncateToWidth(paint(theme, 'muted', `  … ${omitted} more detail line${omitted === 1 ? '' : 's'} hidden; expand for full details`), width)] : []),
  ], undefined);
}

export async function prepareEdit(query: EditQuery, cwd: string, inheritedRequireRecentRead: boolean): Promise<PreparedEdit> {
  const target = await prepareFileMutationTarget(query.path, cwd, false, true);
  const absolutePath = target.canonicalPath;
  await access(absolutePath, constants.R_OK | constants.W_OK);
  // Position-only edits require freshness; supplied oldText verifies current content.
  const contentAnchored = query.edits.every((e) => {
    const mode = e.matchMode ?? 'exact';
    return mode !== 'lineRange' || e.oldText !== undefined;
  });
  const bytes = target.snapshot.content!;
  if (!isUtf8(bytes) || bytes.includes(0)) throw new Error(`Edit requires valid UTF-8 text, not binary data: ${query.path}`);
  const rawContent = bytes.toString('utf8');
  // Commit needs the token, not a second retained copy of the original file.
  delete target.snapshot.content;
  // requireRecentRead is implicitly forced when any edit uses lineRange without oldText,
  // because line numbers can silently shift without a verified read.
  const implicitlyRequired = !contentAnchored && !inheritedRequireRecentRead && query.requireRecentRead !== true;
  const requireRecentRead = inheritedRequireRecentRead || query.requireRecentRead === true || !contentAnchored;
  const readState = await checkReadState(
    absolutePath,
    requireRecentRead,
    {
      contentAnchored,
      currentDigest: target.snapshot.digest,
      implicitReason: implicitlyRequired
        ? 'a lineRange edit without oldText was used — line numbers require a verified read'
        : undefined,
    },
  );
  const { bom, text } = stripBom(rawContent);
  const normalizedContent = normalizeToLF(text);
  const result = applyCustomEditsToContent(normalizedContent, query.edits, query.path);
  const finalContent = bom + restoreEditedText(text, result.changes);
  assertFileContentSize(finalContent);
  const artifacts = await generateDiffArtifactsAsync(query.path, result.baseContent, result.newContent);
  for (const edit of result.edits) {
    const ops = await computeLineDiffAsync(edit.removedLines.join('\n'), edit.addedLines.join('\n'));
    evidenceDiffs.set(edit, ops.filter(op => op.opType !== 'same').map(op => ({ type: op.opType, line: op.line })));
  }
  return {
    target,
    requestPath: query.path,
    absolutePath,
    edits: query.edits,
    finalContent,
    result,
    readState,
    diff: artifacts.diff,
    patch: artifacts.patch,
  };
}

/** Commit one fully preflighted edit while preserving the lost-update guard. */
export async function commitPreparedEdit(prepared: PreparedEdit, signal?: AbortSignal): Promise<ToolCallResult> {
  if (signal?.aborted) throw new Error('Operation aborted');
  let committed: Awaited<ReturnType<typeof replaceNativeFile>>;
  let warnings: string[];
  try {
    ({ receipt: committed, warnings } = await withFileMutationQueue(prepared.absolutePath, async () => {
      if (signal?.aborted) throw new Error('Operation aborted');
      assertFileMutationTargetCurrent(prepared.target);
      const receipt = await replaceNativeFile(prepared.absolutePath, prepared.finalContent, prepared.target.snapshot.version, signal);
      const nextWarnings = [...receipt.warnings, ...await finishFileMutation(prepared.absolutePath, prepared.finalContent)];
      return { receipt, warnings: nextWarnings };
    }));
  } catch (error) {
    rethrowFileMutationConflict(error, prepared.target);
  }
  const replacements = prepared.result.replacements;
  const editCount = prepared.edits.length;
  const firstChangedLine = prepared.result.firstChangedLine;
  const lineSuffix = firstChangedLine ? ` First changed line: ${firstChangedLine}.` : '';
  const readStates = prepared.readState.state;
  const changes = changesSuffix([prepared]);
  const mutationMetrics = prepared.result.changes.reduce((total, change) => {
    const removed = prepared.result.baseContent.slice(change.start, change.end);
    return {
      bytesChanged: total.bytesChanged + Buffer.byteLength(removed, 'utf8') + Buffer.byteLength(change.newText, 'utf8'),
      linesAdded: total.linesAdded + countMutationLines(change.newText),
      linesDeleted: total.linesDeleted + countMutationLines(removed),
    };
  }, { bytesChanged: 0, linesAdded: 0, linesDeleted: 0 });
  return {
    content: [{
      type: 'text',
      text: `Successfully replaced ${replacements} occurrence(s) across ${editCount} edit(s) in 1 file(s).${lineSuffix} Read state: ${readStates}.${changes}${warnings.length ? `\n${warnings.join('\n')}` : ''}`,
    }],
    details: {
      operation: 'edit',
      committed: true,
      durable: committed.durable,
      ...(warnings.length ? { warnings } : {}),
      path: prepared.requestPath,
      replacements,
      firstChangedLine,
      files: [{
        path: prepared.requestPath,
        replacements: prepared.result.replacements,
        firstChangedLine: prepared.result.firstChangedLine,
        usedModes: prepared.result.usedModes,
        readState: prepared.readState,
        edits: prepared.result.edits,
        diff: prepared.diff,
        coloredDiff: colorDiffString(prepared.diff),
        patch: prepared.patch,
      }],
      diff: `# ${prepared.requestPath}\n${prepared.diff}`,
      patch: prepared.patch,
      mutation: createCommittedMutationReceipt({
        snapshot: prepared.target.snapshot,
        nextContent: prepared.finalContent,
        ...mutationMetrics,
      }),
    },
  };
}


/** Render edit evidence under the caller's tool label. */
export function renderEditResult(
  result: ToolCallResult,
  opts: { expanded?: boolean; isPartial?: boolean },
  theme: PiTheme | undefined,
  displayName: string = EDIT_TOOL_DISPLAY_NAME,
): RenderCallReturn {
  if (opts.isPartial) {
    const prog = paint(theme, 'brand', `${CLI_STATUS_TEXT.editing} ${displayName}`);
    return makeComponentRenderer((_props, { width: width }) => [truncateToWidth(prog, width)], undefined);
  }
  const ok = !result.isError;
  const details = result.details as {
    replacements?: number;
    firstChangedLine?: number;
    files?: RenderableEditFile[];
  } | undefined;
  const count = typeof details?.replacements === 'number'
    ? ` \xb7 ${details.replacements} replacement${details.replacements === 1 ? '' : 's'}`
    : '';
  const icon = paint(theme, cliStatusToken(ok), cliStatusGlyph(ok));
  const titleStr = cliToolTitle(theme, displayName);
  const header = `${icon} ${titleStr}${count}`;

  if (!opts.expanded) {
    const files = details?.files ?? [];
    const fileCount = files.length;
    const filesNote = fileCount > 0
      ? paint(theme, 'dim', ` \xb7 ${fileCount} file${fileCount === 1 ? '' : 's'}`)
      : '';
    return fileCount > 0
      ? renderCollapsedEditDiffLines(`${header}${filesNote}`, files, theme)
      : makeComponentRenderer((_props, { width: width }) => [truncateToWidth(`${header}${filesNote}`, width)], undefined);
  }

  // Per file → per edit metadata and changed lines.
  const items: Array<{ text: string; truncate: boolean }> = [{ text: header, truncate: true }];
  for (const file of details?.files ?? []) {
    items.push({
      text: paint(theme, 'path', `  ${file.path}`),
      truncate: true,
    });
    for (const edit of file.edits ?? []) {
      const range = edit.startLine === edit.endLine
        ? `line ${edit.startLine}`
        : `lines ${edit.startLine}–${edit.endLine}`;
      const metaStr = `    edit #${edit.editIndex + 1} \xb7 ${range} \xb7 ${edit.mode}`;
      items.push({ text: paint(theme, 'dim', metaStr), truncate: true });

      items.push(...renderEditDiffItems([{ path: file.path, edits: [edit] }], theme).slice(1));
    }
  }
  return makeComponentRenderer((_props, { width: width }) => items.map((item) =>
    item.truncate ? truncateToWidth(item.text, width) : item.text,
  ), undefined);
}
