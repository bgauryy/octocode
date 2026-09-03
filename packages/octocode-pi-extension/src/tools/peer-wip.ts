/**
 * peer-wip — surface working-tree changes that existed BEFORE this session
 * (likely another agent's / the user's in-progress work) so edit/write can warn
 * before we co-mingle edits into a file we don't own.
 *
 * Captured once at session start from `git status --porcelain`. When edit/write
 * touches a path that was dirty at baseline (and we haven't written it yet), it
 * emits a one-line advisory. Once we write a file, it's marked as ours so the
 * warning fires at most once per file. Purely advisory — never blocks.
 */

import path from 'node:path';

const baselineDirty = new Set<string>(); // absolute paths dirty before this session
const ownWrites = new Set<string>(); // absolute paths this session has written

// ─── Status painter hook ────────────────────────────────────────────────────

/** Callback registered by the host to paint/clear the peer-WIP status chip. */
let statusPainter: ((count: number) => void) | undefined;

/**
 * Register a painter that is called whenever the peer-WIP count changes
 * (after the baseline is set and after each file is claimed by this session).
 * Pass `undefined` on session shutdown to detach.
 */
export function setPeerWipStatusPainter(fn: ((count: number) => void) | undefined): void {
  statusPainter = fn;
}

/**
 * Parse `git status --porcelain` output into repo-relative paths. Handles the
 * two-char status prefix, rename arrows (`R  old -> new` → the new path), and
 * quoted paths (spaces / non-ASCII).
 */
export function parseGitPorcelain(output: string): string[] {
  const paths: string[] = [];
  for (const rawLine of output.split('\n')) {
    if (!rawLine.trim()) continue;
    // Porcelain v1: "XY <path>"; the path starts at column 3.
    const body = rawLine.length > 3 ? rawLine.slice(3) : rawLine.trim();
    // Rename/copy: "old -> new" — the new path is the live one.
    const arrow = body.indexOf(' -> ');
    const rel = arrow >= 0 ? body.slice(arrow + 4) : body;
    const cleaned = unquote(rel.trim());
    if (cleaned) paths.push(cleaned);
  }
  return paths;
}

function unquote(s: string): string {
  if (s.length >= 2 && s.startsWith('"') && s.endsWith('"')) {
    return s.slice(1, -1).replace(/\\(["\\])/g, '$1');
  }
  return s;
}

/** Record the pre-session dirty set from porcelain output (resolved to absolute paths). */
export function setPeerWipBaseline(cwd: string, porcelain: string): void {
  baselineDirty.clear();
  ownWrites.clear();
  for (const rel of parseGitPorcelain(porcelain)) {
    baselineDirty.add(path.resolve(cwd, rel));
  }
  statusPainter?.(peerWipCount());
}

/** True when `absPath` was dirty before this session and we have not written it yet. */
export function isPeerWip(absPath: string): boolean {
  return baselineDirty.has(absPath) && !ownWrites.has(absPath);
}

/** Mark a path as written by this session (suppresses further peer-WIP warnings for it). */
export function markOwnWrite(absPath: string): void {
  ownWrites.add(absPath);
  statusPainter?.(peerWipCount());
}

/** Count of pre-session dirty files this session has not yet touched. */
export function peerWipCount(): number {
  let n = 0;
  for (const p of baselineDirty) if (!ownWrites.has(p)) n++;
  return n;
}

/** One-line advisory for a peer-WIP file, or '' when the path is not peer-WIP. */
export function peerWipNotice(absPath: string, requestPath: string): string {
  if (!isPeerWip(absPath)) return '';
  return `\n⚠ note: ${requestPath} was already modified in the working tree before this session ` +
    `(likely another agent's or your uncommitted work). Confirm you own this change before staging/committing.`;
}

/** Test seam: reset all peer-WIP state. */
export function resetPeerWipForTests(): void {
  baselineDirty.clear();
  ownWrites.clear();
  statusPainter = undefined;
}
