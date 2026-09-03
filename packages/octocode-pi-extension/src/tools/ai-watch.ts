/**
 * ai-watch — F9: Aider-style watch mode.
 *
 * Edit a source file in your own editor and leave a comment ending with `AI!`
 * (e.g. `// rename this function AI!`); the watcher picks it up and injects a
 * prompt into the running agent via pi.sendUserMessage — `steer` while a turn
 * is active, `followUp` otherwise.
 *
 * ── index.ts wiring (this module NEVER edits index.ts / tool files itself) ──
 *
 *   import {
 *     registerAiWatch, markOwnWrite, markBashActivity, stopWatch,
 *   } from './tools/ai-watch.js';
 *
 *   1. registerAiWatch(pi, { cwd: () => ctx.cwd ?? process.cwd(), isTurnActive })
 *      at activation. Registers the `/octocode-watch on|off|status` command and
 *      auto-starts when env OCTOCODE_WATCH=1. If no isTurnActive getter is
 *      passed it self-tracks turn activity from agent_start/agent_end.
 *
 *   2. markOwnWrite(absolutePath) — our own Edit/Write tools cause fs events
 *      that must not loop back into the agent. Because edit-tool.ts and
 *      write-tool.ts are not edited by this feature, wire it from index.ts via
 *      pi.on('tool_execution_start', …): when event.toolName is a file-writing
 *      tool ('edit', 'write', and the octocode multi-file variants), resolve
 *      every target path (resolveFilePath(p, ctx.cwd) from file-state.ts) and
 *      call markOwnWrite(abs) for each — BEFORE the write lands, so the fs
 *      event arrives inside the 5s suppression window. Calling it again from
 *      'tool_execution_end' is harmless and extends cover for slow writes.
 *
 *   3. markBashActivity() — call from the same tool_execution_start AND
 *      tool_execution_end hooks when toolName is the bash/shell tool: bash can
 *      touch arbitrary paths, so a global suppression window is opened.
 *
 *   4. stopWatch() from pi.on('session_shutdown') so no watcher, debounce
 *      timer, or poll interval outlives the session.
 */
import { watch as fsWatch, statSync, type FSWatcher } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import path from 'node:path';
import type { AutocompleteItem, PiCommandContext, PiInstance } from '../types.js';

// ─── Tunables ─────────────────────────────────────────────────────────────────

export const OWN_WRITE_SUPPRESS_MS = 5_000;
export const BASH_SUPPRESS_MS = 5_000;
export const DEBOUNCE_MS = 300;
export const POLL_INTERVAL_MS = 2_000;
const MAX_SCAN_BYTES = 1_500_000;
const MAX_OWN_WRITE_ENTRIES = 500;
const MAX_FIRED_KEYS = 1_000;
const MAX_PROMPT_MARKERS = 50;
const MAX_MARKER_TEXT_CHARS = 200;
export const MAX_AI_WATCH_PROMPT_CHARS = 16_000;

let ownWriteSuppressMs = OWN_WRITE_SUPPRESS_MS;
let bashSuppressMs = BASH_SUPPRESS_MS;
let debounceMs = DEBOUNCE_MS;

// ─── Marker extraction ────────────────────────────────────────────────────────

export interface AiMarker {
  /** 1-based line number. */
  line: number;
  /** Instruction text of the comment (between the comment opener and `AI!`). */
  text: string;
}

/**
 * One comment line ending with the `AI!` marker. `\bAI!` enforces a word
 * boundary before the marker so `OPENAI!` never matches. Supported openers
 * (loose, line-based): `//`, `#`, `/*`, `<!--`, and a block-comment
 * continuation `*`. The marker must terminate the comment (an optional
 * comment terminator (star-slash or `-->`) and trailing whitespace may follow).
 */
const MARKER_RE = /(?:\/\/+|#+|\/\*+|<!--|^[ \t]*\*(?!\/))\s?(.*?)[ \t]*\bAI!(?:[ \t]*(?:\*+\/|-->))?[ \t]*$/;

export function extractAiMarkers(content: string, filePath: string): AiMarker[] {
  void filePath; // reserved for per-language comment-style selection
  const markers: AiMarker[] = [];
  const lines = content.split(/\r\n|\r|\n/);
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (!line.includes('AI!')) continue; // fast path
    const match = MARKER_RE.exec(line);
    if (!match) continue;
    markers.push({ line: i + 1, text: (match[1] ?? '').trim() });
  }
  return markers;
}

// ─── Loop guards ──────────────────────────────────────────────────────────────

/** path → suppression expiry (epoch ms). */
const ownWrites = new Map<string, number>();
let bashSuppressedUntil = 0;
/** FIFO set of `${path}\u0000${markerSetHash}` keys that already fired. */
const firedKeys = new Set<string>();

function pruneOwnWrites(now: number): void {
  for (const [p, until] of ownWrites) {
    if (until <= now) ownWrites.delete(p);
  }
  while (ownWrites.size > MAX_OWN_WRITE_ENTRIES) {
    const oldest = ownWrites.keys().next().value;
    if (oldest === undefined) break;
    ownWrites.delete(oldest);
  }
}

/**
 * Suppress watch events for `filePath` for the next ~5s. MUST be called for
 * every path our own Edit/Write tools are about to write (see wiring notes in
 * the header) — otherwise our own edits would re-enter the agent as `AI!`
 * scan candidates.
 */
export function markOwnWrite(filePath: string): void {
  const now = Date.now();
  pruneOwnWrites(now);
  ownWrites.set(path.resolve(filePath), now + ownWriteSuppressMs);
}

/**
 * Open a global suppression window (all paths) while / just after a bash tool
 * runs — shell commands can touch arbitrary files.
 */
export function markBashActivity(): void {
  bashSuppressedUntil = Date.now() + bashSuppressMs;
}

function isOwnWriteSuppressed(absPath: string): boolean {
  const until = ownWrites.get(absPath);
  if (until === undefined) return false;
  if (until <= Date.now()) {
    ownWrites.delete(absPath);
    return false;
  }
  return true;
}

function isBashSuppressed(): boolean {
  return Date.now() < bashSuppressedUntil;
}

function markerSetHash(markers: AiMarker[]): string {
  return createHash('sha256')
    .update(JSON.stringify(markers.map((m) => [m.line, m.text])))
    .digest('hex');
}

function rememberFired(key: string): void {
  firedKeys.add(key);
  while (firedKeys.size > MAX_FIRED_KEYS) {
    const oldest = firedKeys.values().next().value;
    if (oldest === undefined) break;
    firedKeys.delete(oldest);
  }
}

// ─── Module wiring state ──────────────────────────────────────────────────────

export interface AiWatchDeps {
  /** Project root to watch. Defaults to process.cwd(). */
  cwd?: string | (() => string);
  /** Whether an agent turn is currently streaming (drives steer vs followUp). */
  isTurnActive?: () => boolean;
  /** Env for the OCTOCODE_WATCH auto-start check. Defaults to process.env. */
  env?: NodeJS.ProcessEnv;
  /**
   * Called after every watch-state transition (startWatch / stopWatch).
   * Receives `'watch: fs'` / `'watch: poll'` when active, `undefined` when off.
   * Used by the host to paint / clear a persistent status chip.
   */
  setStatus?: (text: string | undefined) => void;
}

let piRef: PiInstance | undefined;
let depsRef: AiWatchDeps = {};
let internalTurnActive = false;

function resolveCwd(): string {
  const cwd = depsRef.cwd;
  if (typeof cwd === 'function') return cwd();
  return cwd ?? process.cwd();
}

function turnActive(): boolean {
  if (depsRef.isTurnActive) {
    try {
      return depsRef.isTurnActive() === true;
    } catch {
      return internalTurnActive;
    }
  }
  return internalTurnActive;
}

// ─── Scanning ─────────────────────────────────────────────────────────────────

export type ScanReason =
  | 'fired'
  | 'not-configured'
  | 'bash-suppressed'
  | 'own-write-suppressed'
  | 'unreadable'
  | 'too-large'
  | 'no-markers'
  | 'deduped';

export interface ScanOutcome {
  fired: boolean;
  reason: ScanReason;
  markers: AiMarker[];
}

function buildPrompt(displayPath: string, markers: AiMarker[]): string {
  const shown = markers.slice(0, MAX_PROMPT_MARKERS);
  const lines = shown.map((marker) => {
    const text = marker.text || '(no instruction text)';
    const bounded = text.length > MAX_MARKER_TEXT_CHARS
      ? `${text.slice(0, MAX_MARKER_TEXT_CHARS)}…`
      : text;
    return `line ${marker.line}: ${bounded}`;
  });
  const omitted = markers.length - shown.length;
  const prompt = [
    `AI! markers found in ${displayPath}:`,
    ...lines,
    ...(omitted > 0
      ? ['', `${omitted} more AI! markers omitted from this bounded prompt; inspect ${displayPath} directly before editing.`]
      : []),
    '',
    'Address each AI! instruction above, then remove the AI! comment(s) when done.',
  ].join('\n');
  if (prompt.length <= MAX_AI_WATCH_PROMPT_CHARS) return prompt;
  return `${prompt.slice(0, MAX_AI_WATCH_PROMPT_CHARS - 1)}…`;
}

/**
 * Scan one file for AI! markers and deliver a prompt when new markers are
 * found. Full guard pipeline: bash window → own-write window → read →
 * extract → (path, marker-set hash) dedupe → sendUserMessage.
 */
async function scanFile(filePath: string): Promise<ScanOutcome> {
  const absPath = path.resolve(filePath);
  if (isBashSuppressed()) return { fired: false, reason: 'bash-suppressed', markers: [] };
  if (isOwnWriteSuppressed(absPath)) return { fired: false, reason: 'own-write-suppressed', markers: [] };

  let content: string;
  try {
    content = await readFile(absPath, 'utf8');
  } catch {
    return { fired: false, reason: 'unreadable', markers: [] };
  }
  if (content.length > MAX_SCAN_BYTES) return { fired: false, reason: 'too-large', markers: [] };
  if (content.includes('\u0000')) return { fired: false, reason: 'no-markers', markers: [] };

  const markers = extractAiMarkers(content, absPath);
  if (markers.length === 0) return { fired: false, reason: 'no-markers', markers: [] };

  const key = `${absPath}\u0000${markerSetHash(markers)}`;
  if (firedKeys.has(key)) return { fired: false, reason: 'deduped', markers };

  const pi = piRef;
  if (!pi) return { fired: false, reason: 'not-configured', markers };

  const cwd = resolveCwd();
  const rel = path.relative(cwd, absPath);
  const displayPath = rel && !rel.startsWith('..') && !path.isAbsolute(rel) ? rel : absPath;
  const deliverAs = turnActive() ? 'steer' : 'followUp';
  try {
    pi.sendUserMessage(buildPrompt(displayPath, markers), { deliverAs });
  } catch {
    return { fired: false, reason: 'not-configured', markers };
  }
  rememberFired(key);
  return { fired: true, reason: 'fired', markers };
}

// ─── Path filtering ───────────────────────────────────────────────────────────

const TEXTISH_EXTENSIONS = new Set([
  '.ts', '.tsx', '.mts', '.cts', '.js', '.jsx', '.mjs', '.cjs',
  '.py', '.rb', '.go', '.rs', '.java', '.kt', '.kts', '.scala',
  '.c', '.h', '.cc', '.cpp', '.hpp', '.cs', '.swift', '.m', '.mm',
  '.sh', '.bash', '.zsh', '.fish', '.php', '.pl', '.lua', '.sql', '.r',
  '.html', '.htm', '.css', '.scss', '.less', '.vue', '.svelte', '.astro',
  '.md', '.mdx', '.txt', '.json', '.jsonc', '.yaml', '.yml', '.toml',
  '.xml', '.ini', '.cfg', '.conf', '.gradle', '.proto', '.tf', '.zig',
]);

const SPECIAL_BASENAMES = new Set(['Dockerfile', 'Makefile', 'Rakefile', 'Justfile']);

const IGNORED_SEGMENTS = new Set([
  'node_modules', '.git', 'dist', 'build', 'out', 'coverage', 'target',
  'vendor', '.next', '.nuxt', '.cache', '.yarn', '.venv', '__pycache__',
  '.octocode', '.idea', '.vscode',
]);

function isWatchablePath(relPath: string): boolean {
  const normalized = relPath.split(path.sep).join('/');
  const segments = normalized.split('/');
  for (const segment of segments) {
    if (IGNORED_SEGMENTS.has(segment)) return false;
  }
  const base = segments[segments.length - 1] ?? '';
  if (base.endsWith('.tmp') || base.endsWith('~') || base.startsWith('.#')) return false;
  if (SPECIAL_BASENAMES.has(base)) return true;
  return TEXTISH_EXTENSIONS.has(path.extname(base).toLowerCase());
}

// ─── Watcher ──────────────────────────────────────────────────────────────────

export type WatchMode = 'fs' | 'poll';

let watchMode: WatchMode | undefined;
let watcher: FSWatcher | undefined;
let pollTimer: ReturnType<typeof setInterval> | undefined;
let pollInFlight = false;
const debounceTimers = new Map<string, ReturnType<typeof setTimeout>>();

function scheduleScan(absPath: string): void {
  const existing = debounceTimers.get(absPath);
  if (existing !== undefined) clearTimeout(existing);
  const timer = setTimeout(() => {
    debounceTimers.delete(absPath);
    if (watchMode === undefined) return; // stopped while debouncing — suppress late callback
    void scanFile(absPath).catch(() => undefined);
  }, debounceMs);
  (timer as { unref?: () => void }).unref?.();
  debounceTimers.set(absPath, timer);
}

function onFsEvent(cwd: string, filename: string): void {
  if (watchMode !== 'fs') return;
  if (!isWatchablePath(filename)) return;
  scheduleScan(path.resolve(cwd, filename));
}

function parsePorcelainPaths(stdout: string): string[] {
  const paths: string[] = [];
  for (const raw of stdout.split('\n')) {
    if (raw.length < 4) continue;
    let entry = raw.slice(3);
    const arrow = entry.indexOf(' -> ');
    if (arrow !== -1) entry = entry.slice(arrow + 4); // renames: keep the new path
    if (entry.startsWith('"') && entry.endsWith('"')) entry = entry.slice(1, -1);
    if (entry) paths.push(entry);
  }
  return paths;
}

function startPoll(cwd: string): void {
  pollTimer = setInterval(() => {
    if (watchMode !== 'poll' || pollInFlight) return;
    pollInFlight = true;
    execFile('git', ['status', '--porcelain'], { cwd, timeout: 5_000 }, (error, stdout) => {
      pollInFlight = false;
      if (watchMode !== 'poll' || error) return; // stopped or not a git repo — suppress late callback
      for (const rel of parsePorcelainPaths(stdout)) {
        if (!isWatchablePath(rel)) continue;
        const abs = path.resolve(cwd, rel);
        try {
          if (!statSync(abs).isFile()) continue;
        } catch {
          continue;
        }
        void scanFile(abs).catch(() => undefined);
      }
    });
  }, POLL_INTERVAL_MS);
  (pollTimer as { unref?: () => void }).unref?.();
}

/**
 * Start watching. Prefers fs.watch(cwd, {recursive:true}) (fine on darwin /
 * win32 / recent Linux); falls back to polling `git status --porcelain`
 * every 2s when recursive watch is unavailable. Idempotent.
 */
export function startWatch(): WatchMode | undefined {
  if (watchMode !== undefined) return watchMode;
  const cwd = resolveCwd();
  try {
    watcher = fsWatch(cwd, { recursive: true }, (_event, filename) => {
      if (typeof filename !== 'string' || filename.length === 0) return;
      onFsEvent(cwd, filename);
    });
    watcher.on('error', () => {
      // Recursive watch died at runtime (e.g. unsupported fs) — fall back to polling.
      try {
        watcher?.close();
      } catch {
        // ignore
      }
      watcher = undefined;
      if (watchMode === 'fs') {
        watchMode = 'poll';
        startPoll(cwd);
      }
    });
    watchMode = 'fs';
  } catch {
    watcher = undefined;
    watchMode = 'poll';
    startPoll(cwd);
  }
  depsRef.setStatus?.(watchMode !== undefined ? `watch: ${watchMode}` : undefined);
  return watchMode;
}

/** Stop watching and clear every watcher/timer. Safe to call repeatedly. */
export function stopWatch(): void {
  watchMode = undefined;
  if (watcher !== undefined) {
    try {
      watcher.close();
    } catch {
      // ignore
    }
    watcher = undefined;
  }
  if (pollTimer !== undefined) {
    clearInterval(pollTimer);
    pollTimer = undefined;
  }
  pollInFlight = false;
  for (const timer of debounceTimers.values()) clearTimeout(timer);
  debounceTimers.clear();
  depsRef.setStatus?.(undefined);
}

/** Whether the watcher is currently running (fs.watch or poll). */
export function isWatchActive(): boolean {
  return watchMode !== undefined;
}

// ─── Registration ─────────────────────────────────────────────────────────────

const WATCH_ARGS: AutocompleteItem[] = [
  { value: 'on', label: 'on', description: 'Start watching for AI! comments' },
  { value: 'off', label: 'off', description: 'Stop watching' },
  { value: 'status', label: 'status', description: 'Show watch state' },
];

function statusLine(): string {
  return watchMode === undefined
    ? 'octocode-watch: off'
    : `octocode-watch: on (${watchMode === 'fs' ? 'fs.watch recursive' : 'git status poll'}) — watching ${resolveCwd()}`;
}

/**
 * Register the `/octocode-watch on|off|status` command and the OCTOCODE_WATCH=1
 * auto-start. Call once at activation; see the header for full index.ts wiring
 * (markOwnWrite / markBashActivity / stopWatch).
 */
export function registerAiWatch(pi: PiInstance, deps: AiWatchDeps = {}): void {
  piRef = pi;
  depsRef = deps;

  if (!deps.isTurnActive) {
    // Default turn tracking when the host does not supply a getter.
    try {
      pi.on('agent_start', async () => {
        internalTurnActive = true;
      });
      pi.on('agent_end', async () => {
        internalTurnActive = false;
      });
    } catch {
      // Host without an event bus — followUp delivery is always used.
    }
  }

  pi.registerCommand?.('octocode-watch', {
    description: 'Aider-style AI! comment watch mode: on | off | status',
    getArgumentCompletions: (prefix: string) =>
      WATCH_ARGS.filter((item) => item.value.startsWith(prefix.trim().toLowerCase())),
    handler: async (args: string, ctx: PiCommandContext) => {
      const arg = args.trim().toLowerCase() || 'status';
      if (arg === 'on') {
        const mode = startWatch();
        ctx.ui?.notify?.(
          mode !== undefined ? statusLine() : 'octocode-watch: failed to start',
          mode !== undefined ? 'info' : 'error',
        );
      } else if (arg === 'off') {
        stopWatch();
        ctx.ui?.notify?.('octocode-watch: off', 'info');
      } else {
        ctx.ui?.notify?.(statusLine(), 'info');
      }
    },
  });

  const env = deps.env ?? process.env;
  if (env['OCTOCODE_WATCH'] === '1') startWatch();
}

// ─── Test seam ────────────────────────────────────────────────────────────────

export const __test__ = {
  /** Drive detection directly, without a real fs.watch. */
  scanFile,
  extractAiMarkers,
  isWatchablePath,
  parsePorcelainPaths,
  /** Shrink guard windows so expiry can be tested with short real waits. Pass null to restore defaults. */
  setWindowsForTests(opts: { ownWriteMs?: number; bashMs?: number; debounceMs?: number } | null): void {
    ownWriteSuppressMs = opts?.ownWriteMs ?? OWN_WRITE_SUPPRESS_MS;
    bashSuppressMs = opts?.bashMs ?? BASH_SUPPRESS_MS;
    debounceMs = opts?.debounceMs ?? DEBOUNCE_MS;
  },
  isOwnWriteSuppressed(filePath: string): boolean {
    return isOwnWriteSuppressed(path.resolve(filePath));
  },
  isBashSuppressed,
  getWatchMode(): WatchMode | undefined {
    return watchMode;
  },
  /** Full reset: watcher, guards, dedupe cache, wiring refs, and windows. */
  reset(): void {
    stopWatch();
    ownWrites.clear();
    bashSuppressedUntil = 0;
    firedKeys.clear();
    piRef = undefined;
    depsRef = {};
    internalTurnActive = false;
    ownWriteSuppressMs = OWN_WRITE_SUPPRESS_MS;
    bashSuppressMs = BASH_SUPPRESS_MS;
    debounceMs = DEBOUNCE_MS;
  },
};
