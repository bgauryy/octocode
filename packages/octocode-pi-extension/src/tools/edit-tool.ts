import { truncateToWidth } from '../tui/width.js';
import { paint, ANSI_RESET } from '../tui/palette.js';
import { constants } from 'node:fs';
import { access, readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { CLI_STATUS_TEXT, cliStatusGlyph, cliStatusToken, cliToolTitle, ansiForToken } from '../tui/cli-design.js';
import type { ToolCallResult, PiTheme, RenderCallReturn } from '../types.js';
import { makeComponentRenderer, wrapText } from './render-helpers.js';
import { assertPathAllowed } from './path-guard.js';
import { atomicWriteUtf8, withFileMutationQueue, recordFileReadStateFromContent, checkReadState, resolveFilePath, type ReadStateCheck } from './file-state.js';
import { peerWipNotice, markOwnWrite } from './peer-wip.js';


const require = createRequire(import.meta.url);

type MatchMode = 'exact' | 'normalized' | 'lineRange';

export interface EditQuery {
  path: string;
  edits: EditOperation[];
  requireRecentRead?: boolean;
}

export interface EditOperation {
  oldText?: string;
  newText: string;
  replaceAll?: boolean;
  reasoning: string;
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
  reasoning: string;
  // Removed text fragments (the oldText segments), split by line.
  removedLines: string[];
  // Added text fragments (the newText), split by line.
  addedLines: string[];
}

export interface PreparedEdit {
  requestPath: string;
  absolutePath: string;
  edits: EditOperation[];
  requireRecentRead: boolean;
  rawContent: string;
  finalContent: string;
  result: AppliedEditResult;
  readState: ReadStateCheck;
  diff: string;
  patch: string;
}

interface EditReasoningEntry {
  editIndex: number;
  reasoning: string;
}

interface RenderableEditFile {
  path: string;
  edits?: Array<AppliedEditEvidence>;
}

// ReadStateCheck is imported from file-state.ts above (type re-used in PreparedEdit).

function normalizeToLF(text: string): string {
  return text.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
}

function detectLineEnding(text: string): '\n' | '\r\n' {
  // Only report CRLF when the file is UNIFORMLY CRLF (every LF is part of a CRLF).
  // A mixed file (some bare LF, some CRLF) reports '\n' so restoreLineEndings leaves
  // it untouched — otherwise editing one line would rewrite every originally-LF line
  // to CRLF (spurious whole-file churn).
  if (!text.includes('\r\n')) return '\n';
  // Strip CRLF pairs; if any bare LF remains, the file is mixed → treat as LF.
  return text.replace(/\r\n/g, '').includes('\n') ? '\n' : '\r\n';
}

function restoreLineEndings(text: string, ending: '\n' | '\r\n'): string {
  return ending === '\r\n' ? text.replace(/\n/g, '\r\n') : text;
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
    index = content.indexOf(needle, index + needle.length);
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
  if (!edit || typeof edit !== 'object') {
    throw new Error(`Edit tool input is invalid. edits[${index}] must be an object.`);
  }
  const item = edit as Record<string, unknown>;
  const matchMode = (item['matchMode'] ?? 'exact') as MatchMode;
  if (!['exact', 'normalized', 'lineRange'].includes(matchMode)) {
    throw new Error(`Edit tool input is invalid. edits[${index}].matchMode must be exact, normalized, or lineRange.`);
  }
  if (typeof item['newText'] !== 'string') {
    throw new Error(`Edit tool input is invalid. edits[${index}].newText must be a string.`);
  }
  if (item['oldText'] !== undefined && typeof item['oldText'] !== 'string') {
    throw new Error(`Edit tool input is invalid. edits[${index}].oldText must be a string.`);
  }
  if (matchMode !== 'lineRange' && (typeof item['oldText'] !== 'string' || item['oldText'].length === 0)) {
    throw new Error(`Edit tool input is invalid. edits[${index}].oldText must be a non-empty string unless matchMode:"lineRange" is used.`);
  }
  if (item['oldText'] === '' && matchMode !== 'lineRange') {
    throw new Error(`Edit tool input is invalid. edits[${index}].oldText must not be empty; omit oldText when using matchMode:"lineRange".`);
  }
  if (item['replaceAll'] !== undefined && typeof item['replaceAll'] !== 'boolean') {
    throw new Error(`Edit tool input is invalid. edits[${index}].replaceAll must be a boolean.`);
  }
  if (typeof item['reasoning'] !== 'string' || item['reasoning'].trim().length === 0) {
    throw new Error(`Edit tool input is invalid. edits[${index}].reasoning is required — provide a non-empty string explaining why this edit is necessary.`);
  }
  const operation: EditOperation = {
    oldText: matchMode === 'lineRange' && item['oldText'] === '' ? undefined : item['oldText'] as string | undefined,
    newText: item['newText'],
    replaceAll: item['replaceAll'] === true,
    reasoning: item['reasoning'] as string,
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
  return (edit.replaceAll ? occurrences : [occurrences[0]!]).map((start) => ({
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
  const oldLineCount = oldText.split('\n').length;
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
  return [{ editIndex, start, end, newText: normalizeToLF(edit.newText), mode: 'lineRange' }];
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
    const edit = edits[r.editIndex]!;
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
        reasoning: edit.reasoning.trim(),
        removedLines: toLines(removedText),
        addedLines: toLines(r.newText),
      });
    }
  }
  const editEvidence = [...editEvidenceMap.values()].sort((a, b) => a.editIndex - b.editIndex);

  return {
    baseContent: content,
    newContent,
    replacements: replacements.length,
    firstChangedLine: firstChangedLine(content, newContent),
    usedModes: [...new Set(replacements.map((replacement) => replacement.mode))],
    edits: editEvidence,
  };
}

interface DiffOp { type: 'same' | 'add' | 'remove'; line: string }

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

/**
 * Myers O((N+M)D) line diff. Returns a full edit script of same/add/remove ops.
 * Default: pure JS Myers (typically sub-ms for agent-sized edits).
 * Opt-in native engine path via OCTOCODE_EDIT_NATIVE_DIFF=1 (useful for release
 * napi builds / shared CLI callers; debug napi can be slower than JS).
 * Exported for unit/perf tests.
 */
export function diffOps(oldContent: string, newContent: string): DiffOp[] {
  if (process.env['OCTOCODE_EDIT_NATIVE_DIFF'] === '1') {
    const native = tryNativeDiffOps(oldContent, newContent);
    if (native) return native;
  }
  return diffOpsJs(oldContent, newContent);
}

type NativeLineDiffOp = { opType: string; line: string };
let nativeComputeLineDiff:
  | ((oldText: string, newText: string) => NativeLineDiffOp[])
  | null
  | undefined;

function tryNativeDiffOps(oldContent: string, newContent: string): DiffOp[] | null {
  if (nativeComputeLineDiff === undefined) {
    try {
      const eng = require('@octocodeai/octocode-engine') as {
        computeLineDiff?: (a: string, b: string) => NativeLineDiffOp[];
      };
      nativeComputeLineDiff =
        typeof eng.computeLineDiff === 'function' ? eng.computeLineDiff.bind(eng) : null;
    } catch {
      nativeComputeLineDiff = null;
    }
  }
  if (!nativeComputeLineDiff) return null;
  try {
    const ops = nativeComputeLineDiff(oldContent, newContent);
    return ops.map((op) => ({
      type: op.opType as DiffOp['type'],
      line: op.line,
    }));
  } catch {
    return null;
  }
}

/** Pure JS Myers — used when the native addon is unavailable. */
export function diffOpsJs(oldContent: string, newContent: string): DiffOp[] {
  const a = oldContent.split('\n');
  const b = newContent.split('\n');
  const n = a.length;
  const m = b.length;
  if (n === 0 && m === 0) return [];
  if (n === 0) return b.map((line) => ({ type: 'add' as const, line }));
  if (m === 0) return a.map((line) => ({ type: 'remove' as const, line }));

  const max = n + m;
  const offset = max;
  const v = new Int32Array(2 * max + 1);
  v.fill(-1);
  v[offset + 1] = 0;
  const trace: Int32Array[] = [];

  let dFound = -1;
  outer: for (let d = 0; d <= max; d++) {
    for (let k = -d; k <= d; k += 2) {
      let x: number;
      if (k === -d || (k !== d && v[offset + k - 1]! < v[offset + k + 1]!)) {
        x = v[offset + k + 1]!;
      } else {
        x = v[offset + k - 1]! + 1;
      }
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) {
        x++;
        y++;
      }
      v[offset + k] = x;
      if (x >= n && y >= m) {
        trace.push(Int32Array.from(v));
        dFound = d;
        break outer;
      }
    }
    trace.push(Int32Array.from(v));
  }

  if (dFound < 0) {
    return [
      ...a.map((line) => ({ type: 'remove' as const, line })),
      ...b.map((line) => ({ type: 'add' as const, line })),
    ];
  }

  const opsRev: DiffOp[] = [];
  let x = n;
  let y = m;
  for (let d = dFound; d > 0; d--) {
    const vPrev = trace[d - 1]!;
    const k = x - y;
    let prevK: number;
    if (k === -d || (k !== d && vPrev[offset + k - 1]! < vPrev[offset + k + 1]!)) {
      prevK = k + 1;
    } else {
      prevK = k - 1;
    }
    const prevX = vPrev[offset + prevK]!;
    const prevY = prevX - prevK;
    while (x > prevX && y > prevY) {
      x--;
      y--;
      opsRev.push({ type: 'same', line: a[x]! });
    }
    if (x === prevX) {
      y--;
      opsRev.push({ type: 'add', line: b[y]! });
    } else {
      x--;
      opsRev.push({ type: 'remove', line: a[x]! });
    }
  }
  while (x > 0 && y > 0) {
    x--;
    y--;
    opsRev.push({ type: 'same', line: a[x]! });
  }
  while (x > 0) {
    x--;
    opsRev.push({ type: 'remove', line: a[x]! });
  }
  while (y > 0) {
    y--;
    opsRev.push({ type: 'add', line: b[y]! });
  }
  opsRev.reverse();
  return opsRev;
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
): { diff: string; patch: string; ops: DiffOp[] } {
  if (diffTooLarge(oldContent, newContent)) {
    return {
      diff: '(diff omitted: file too large — see the per-edit changes in details)',
      patch: `--- ${filePath}\n+++ ${filePath}\n@@ patch omitted: file too large @@\n`,
      ops: [],
    };
  }
  const ops = diffOps(oldContent, newContent);
  const diff = ops
    .filter((op) => op.type !== 'same')
    .map((op) => `${op.type === 'add' ? '+' : '-'} ${op.line}`)
    .join('\n');

  let start = 0;
  while (start < ops.length && ops[start]!.type === 'same') start++;
  let end = ops.length;
  while (end > start && ops[end - 1]!.type === 'same') end--;
  const hunkOps = ops.slice(start, end);
  const oldCount = hunkOps.filter((op) => op.type !== 'add').length;
  const newCount = hunkOps.filter((op) => op.type !== 'remove').length;
  const oldStart = start + 1;
  const newStart = start + 1;
  const lines = [`--- ${filePath}`, `+++ ${filePath}`, `@@ -${oldStart},${oldCount} +${newStart},${newCount} @@`];
  for (const op of hunkOps) {
    if (op.type === 'same') lines.push(` ${op.line}`);
    else lines.push(`${op.type === 'add' ? '+' : '-'}${op.line}`);
  }
  return { diff, patch: `${lines.join('\n')}\n`, ops };
}



const EDIT_TOOL_DISPLAY_NAME = 'edit (Octocode)';



function editReasoningEntries(edits: EditOperation[]): EditReasoningEntry[] {
  return edits.map((edit, index) => ({ editIndex: index, reasoning: edit.reasoning.trim() }));
}

function reasoningSuffix(editsByFile: Array<{ path: string; edits: EditOperation[] }>): string {
  const lines = editsByFile.flatMap((file) => editReasoningEntries(file.edits)
    .map((entry) => entry.reasoning));
  return lines.length > 0 ? `\nReasoning:\n${lines.map((line) => `- ${line}`).join('\n')}` : '';
}

function changesSuffix(prepared: PreparedEdit[]): string {
  const blocks = prepared.map((item) => `# ${item.requestPath}\n${colorDiffString(item.diff)}`);
  return `\nChanges:\n${blocks.join('\n')}`;
}

function renderEditReasoningItems(files: RenderableEditFile[], theme?: PiTheme): Array<{ text: string; truncate: boolean }> {
  const items: Array<{ text: string; truncate: boolean }> = [];
  for (const file of files) {
    for (const edit of file.edits ?? []) {
      const reasonText = edit.reasoning.trim();
      if (!reasonText) continue;
      items.push({
        text: paint(theme, 'muted', `  Reasoning: ${reasonText}`),
        truncate: true,
      });
    }
  }
  return items;
}

function renderEditDiffItems(files: RenderableEditFile[], theme?: PiTheme): Array<{ text: string; truncate: boolean }> {
  const items: Array<{ text: string; truncate: boolean }> = [];
  for (const file of files) {
    items.push({ text: paint(theme, 'path', `  ${file.path}`), truncate: true });
    for (const edit of file.edits ?? []) {
      const ops = diffOps(edit.removedLines.join('\n'), edit.addedLines.join('\n'));
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
  const reasoningItems = renderEditReasoningItems(files, theme);
  const diffItems = renderEditDiffItems(files, theme);
  const maxPreviewLines = 10;
  const previewItems = [...reasoningItems, ...diffItems];
  const shown = previewItems.slice(0, maxPreviewLines);
  const omitted = previewItems.length - shown.length;
  return makeComponentRenderer((_props, { width: width }) => [
    truncateToWidth(header, width),
    ...shown.map((item) => truncateToWidth(item.text, width)),
    ...(omitted > 0 ? [truncateToWidth(paint(theme, 'muted', `  … ${omitted} more reasoning/diff line${omitted === 1 ? '' : 's'} hidden; expand for full details`), width)] : []),
  ], undefined);
}

export async function prepareEdit(query: EditQuery, cwd: string, inheritedRequireRecentRead: boolean): Promise<PreparedEdit> {
  const absolutePath = resolveFilePath(query.path, cwd);
  // Bound writes to home + ALLOWED_PATHS + cwd/tmp (same model as the native tools).
  assertPathAllowed(absolutePath, cwd, 'edit');
  await access(absolutePath, constants.R_OK | constants.W_OK);
  // Content-anchored when every edit matches by exact/normalized oldText (self-
  // verifying); a lineRange edit is position-anchored and needs strict freshness.
  // An edit is content-anchored when exact/normalized oldText matching is used, OR when
  // lineRange is used with an explicit oldText — lineRangeReplacement validates oldText
  // against the actual lines, so the edit is self-verifying even if the read is stale.
  const contentAnchored = query.edits.every((e) => {
    const mode = e.matchMode ?? 'exact';
    return mode !== 'lineRange' || e.oldText !== undefined;
  });
  const readState = await checkReadState(
    absolutePath,
    inheritedRequireRecentRead || query.requireRecentRead === true,
    { contentAnchored },
  );
  const rawContent = await readFile(absolutePath, 'utf8');
  const { bom, text } = stripBom(rawContent);
  const lineEnding = detectLineEnding(text);
  const normalizedContent = normalizeToLF(text);
  const result = applyCustomEditsToContent(normalizedContent, query.edits, query.path);
  const finalContent = bom + restoreLineEndings(result.newContent, lineEnding);
  const artifacts = generateDiffArtifacts(query.path, result.baseContent, result.newContent);
  return {
    requestPath: query.path,
    absolutePath,
    edits: query.edits,
    requireRecentRead: inheritedRequireRecentRead || query.requireRecentRead === true,
    rawContent,
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
  const peerNotice = peerWipNotice(prepared.absolutePath, prepared.requestPath);
  await withFileMutationQueue(prepared.absolutePath, async () => {
    if (signal?.aborted) throw new Error('Operation aborted');
    const currentRaw = await readFile(prepared.absolutePath, 'utf8');
    if (currentRaw !== prepared.rawContent) {
      throw new Error(
        `${prepared.requestPath} changed on disk after it was read for editing ` +
          `(concurrent edit or external write). Re-read the file and retry.`,
      );
    }
    if (signal?.aborted) throw new Error('Operation aborted');
    await atomicWriteUtf8(prepared.absolutePath, prepared.finalContent);
    await recordFileReadStateFromContent(prepared.absolutePath, prepared.finalContent);
    markOwnWrite(prepared.absolutePath);
  });
  if (signal?.aborted) throw new Error('Operation aborted');
  const replacements = prepared.result.replacements;
  const editCount = prepared.edits.length;
  const firstChangedLine = prepared.result.firstChangedLine;
  const lineSuffix = firstChangedLine ? ` First changed line: ${firstChangedLine}.` : '';
  const readStates = prepared.readState.state;
  const reasoning = reasoningSuffix([{ path: prepared.requestPath, edits: prepared.edits }]);
  const changes = changesSuffix([prepared]);
  return {
    content: [{
      type: 'text',
      text: `Successfully replaced ${replacements} occurrence(s) across ${editCount} edit(s) in 1 file(s).${lineSuffix} Read state: ${readStates}.${peerNotice}${reasoning}${changes}`,
    }],
    details: {
      operation: 'edit',
      path: prepared.requestPath,
      replacements,
      firstChangedLine,
      files: [{
        path: prepared.requestPath,
        replacements: prepared.result.replacements,
        firstChangedLine: prepared.result.firstChangedLine,
        usedModes: prepared.result.usedModes,
        readState: prepared.readState,
        reasoning: editReasoningEntries(prepared.edits),
        edits: prepared.result.edits,
        diff: prepared.diff,
        coloredDiff: colorDiffString(prepared.diff),
        patch: prepared.patch,
      }],
      diff: `# ${prepared.requestPath}\n${prepared.diff}`,
      patch: prepared.patch,
    },
  };
}


/**
 * Shared renderer for edit results — used by both the `edit` tool and the `file` tool
 * (which wraps edit operations). Accepts a `displayName` so callers can substitute
 * their own tool label (e.g. 'file (Octocode)') without changing the rendering logic.
 */
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

  // Per file → per edit:
  //   meta line  (truncatable — always short)
  //   reasoning  (word-wrapped so full text is visible without exceeding terminal width)
  //   diff lines (Myers: only genuinely changed lines)
  type StaticItem = { text: string; truncate: boolean };
  type DynamicItem = { fn: (width: number) => string[] };
  type Item = StaticItem | DynamicItem;
  const items: Item[] = [{ text: header, truncate: true }];
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

      const reasonText = edit.reasoning.trim();
      if (reasonText) {
        const indent = '      ';
        items.push({
          fn: (w) => {
            const availWidth = Math.max(w - indent.length, 10);
            return wrapText(reasonText, availWidth).map((line) =>
              truncateToWidth(`${indent}${paint(theme, 'muted', line)}`, w),
            );
          },
        });
      }

      items.push(...renderEditDiffItems([{ path: file.path, edits: [edit] }], theme).slice(1));
    }
  }
  return makeComponentRenderer((_props, { width: width }) => items.flatMap((item) =>
    'fn' in item
      ? item.fn(width)
      : [item.truncate ? truncateToWidth(item.text, width) : item.text],
  ), undefined);
}
