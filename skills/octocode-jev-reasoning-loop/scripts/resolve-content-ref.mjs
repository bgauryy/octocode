// Server-side resolution of `contentRef` evidence pointers into bounded,
// source-anchored `content`. Lets the host supply a POINTER (path + line span or
// regex) instead of inlining file bytes it already paid to read into its own
// context. The runner resolves refs before building the Jev packet, so the
// authored packet stays tiny while Jev still receives exact, anchored evidence.
//
// Contract (mirrors Jev EvidenceSchema bounds):
//   evidence item may carry EITHER `content` (inline, as today) OR `contentRef`:
//     contentRef: {
//       path:     string        // repo-relative (or within rootDir); no traversal
//       lines?:   "S" | "S-E"   // 1-based inclusive line span
//       regex?:   string        // first matching line anchors a window
//       window?:  [before,after] // lines around a regex hit (default [3,8])
//       maxChars?: number        // clamp, 1..4000 (Jev content cap); default 1200
//     }
//   Resolution fills `content` (bounded, redacted) and, when absent, `source`
//   ("path:LS-LE"). Supplying both content and contentRef is rejected.
//
// This mirrors what the native runtime SHOULD do at engine/src/jev.rs:261
// (reusing the localFetch read path); here it is JS so the shipped skill runner
// realizes and measures the saving today.

import { readFileSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';

const CONTENT_CAP = 4000; // Jev EvidenceSchema.content max
const DEFAULT_MAX = 1200;

const isObject = value => value !== null && typeof value === 'object' && !Array.isArray(value);

// Stand-in for the native `secrets` primitive; conservative, never widening scope.
function redact(text) {
  return text
    .replace(/\b(sk|gh[pousr]|xox[baprs]|apikey)[-_][A-Za-z0-9]{16,}\b/gi, '«redacted-token»')
    .replace(/-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----/g, '«redacted-key»');
}

function boundedRoot(rootDir, path) {
  if (typeof path !== 'string' || path.trim().length === 0) throw new Error('contentRef.path expected a nonempty string.');
  if (isAbsolute(path)) throw new Error(`contentRef.path expected a rootDir-relative path; received absolute ${JSON.stringify(path)}.`);
  const abs = resolve(rootDir, path);
  const rel = relative(rootDir, abs);
  if (rel === '' || rel.startsWith('..') || isAbsolute(rel)) throw new Error(`contentRef.path escapes rootDir: ${JSON.stringify(path)}.`);
  return abs;
}

function clampMax(maxChars) {
  if (maxChars === undefined) return DEFAULT_MAX;
  if (!Number.isInteger(maxChars) || maxChars < 1) throw new Error('contentRef.maxChars expected a positive integer.');
  return Math.min(maxChars, CONTENT_CAP);
}

// Resolve one contentRef -> { content, source, meta }
export function resolveRef(ref, rootDir) {
  if (!isObject(ref)) throw new Error('contentRef expected an object.');
  const abs = boundedRoot(rootDir, ref.path);
  const raw = readFileSync(abs, 'utf8');
  const lines = raw.split('\n');
  const hasLines = ref.lines !== undefined;
  const hasRegex = ref.regex !== undefined;
  if (hasLines === hasRegex) throw new Error('contentRef expected exactly one of `lines` or `regex`.');

  let startLine, endLine;
  if (hasLines) {
    const m = /^(\d+)(?:-(\d+))?$/.exec(String(ref.lines).trim());
    if (!m) throw new Error(`contentRef.lines expected "S" or "S-E"; received ${JSON.stringify(ref.lines)}.`);
    startLine = parseInt(m[1], 10);
    endLine = m[2] ? parseInt(m[2], 10) : startLine;
    if (startLine < 1 || endLine < startLine) throw new Error(`contentRef.lines out of order: ${JSON.stringify(ref.lines)}.`);
    if (startLine > lines.length) throw new Error(`contentRef.lines start ${startLine} exceeds ${ref.path} length ${lines.length}.`);
    endLine = Math.min(endLine, lines.length);
  } else {
    let re;
    try { re = new RegExp(ref.regex); } catch { throw new Error(`contentRef.regex is not a valid RegExp: ${JSON.stringify(ref.regex)}.`); }
    const idx = lines.findIndex(l => re.test(l));
    if (idx === -1) throw new Error(`contentRef.regex ${JSON.stringify(ref.regex)} matched nothing in ${ref.path}.`);
    const before = ref.window?.[0] ?? 3;
    const after = ref.window?.[1] ?? 8;
    startLine = Math.max(1, idx + 1 - before);
    endLine = Math.min(lines.length, idx + 1 + after);
  }

  const max = clampMax(ref.maxChars);
  let content = redact(lines.slice(startLine - 1, endLine).join('\n'));
  let truncated = false;
  if (content.length > max) { content = content.slice(0, max); truncated = true; }
  if (content.trim().length === 0) throw new Error(`contentRef resolved to empty content at ${ref.path}:${startLine}-${endLine}.`);
  const source = `${ref.path}:L${startLine}-L${endLine}${truncated ? ' (truncated)' : ''}`;
  return { content, source, meta: { startLine, endLine, truncated, fileChars: raw.length, injectedChars: content.length } };
}

function resolveEvidenceItem(item, rootDir, stats) {
  if (!isObject(item) || item.contentRef === undefined) return item;
  if (item.content !== undefined) throw new Error(`evidence ${item.id ?? '?'} supplied both content and contentRef.`);
  const { content, source, meta } = resolveRef(item.contentRef, rootDir);
  stats.refsResolved += 1;
  stats.fileCharsRead += meta.fileChars;
  stats.contentCharsInjected += meta.injectedChars;
  const { contentRef, ...rest } = item;
  return { ...rest, content, source: rest.source ?? source };
}

// Walk state.evidence[] and state.newEvidence, resolving any contentRef.
// Backward compatible: inputs without contentRef are returned unchanged.
export function resolveEvidenceRefs(input, { rootDir = process.cwd() } = {}) {
  const stats = { refsResolved: 0, fileCharsRead: 0, contentCharsInjected: 0 };
  if (!isObject(input) || !isObject(input.state)) return { input, stats };
  const clone = structuredClone(input);
  const state = clone.state;
  if (Array.isArray(state.evidence)) {
    state.evidence = state.evidence.map(item => resolveEvidenceItem(item, rootDir, stats));
  }
  if (isObject(state.newEvidence)) {
    state.newEvidence = resolveEvidenceItem(state.newEvidence, rootDir, stats);
  }
  return { input: clone, stats };
}
