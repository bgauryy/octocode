/**
 * ffmpeg-runtime — binary discovery + a hardened runner for the `media` tool.
 *
 * Everything here is Pi-agnostic and unit-testable. We NEVER route through a
 * shell: `spawn(bin, argv)` with an explicit string[] means user-supplied paths
 * and timestamps sit in fixed argv positions and cannot inject flags. Callers
 * build argv with the validated helpers in media-tool.ts.
 */

import { spawn } from 'node:child_process';
import { accessSync, constants } from 'node:fs';
import path from 'node:path';

/** Common install locations so we work even when PATH is thin (GUI launch, cron). */
const EXTRA_BIN_DIRS = ['/opt/homebrew/bin', '/usr/local/bin', '/usr/bin', '/bin'];

export interface FfmpegAvailability {
  ok: boolean;
  ffmpeg?: string;
  ffprobe?: string;
  reason?: string;
}

let cachedAvailability: FfmpegAvailability | undefined;

/** Resolve an executable by scanning PATH then well-known dirs. */
function findExecutable(name: string): string | undefined {
  const pathDirs = (process.env['PATH'] ?? '').split(path.delimiter).filter(Boolean);
  for (const dir of [...pathDirs, ...EXTRA_BIN_DIRS]) {
    const candidate = path.join(dir, name);
    try {
      accessSync(candidate, constants.X_OK);
      return candidate;
    } catch {
      // keep scanning
    }
  }
  return undefined;
}

/** Try to resolve a binary from the optional ffmpeg-static / ffprobe-static npm packages. */
function findStaticBinary(name: 'ffmpeg' | 'ffprobe'): string | undefined {
  try {
    if (name === 'ffmpeg') {
      // ffmpeg-static exports the binary path as its default export
      // eslint-disable-next-line @typescript-eslint/no-var-requires
      const p = require('ffmpeg-static') as string | null;
      if (typeof p === 'string' && p) return p;
    } else {
      // ffprobe-static exports { path: string }
      // eslint-disable-next-line @typescript-eslint/no-var-requires
      const pkg = require('ffprobe-static') as { path?: string } | null;
      if (pkg && typeof pkg.path === 'string' && pkg.path) return pkg.path;
    }
  } catch {
    // optional dep not installed — silently ignore
  }
  return undefined;
}

/** Detect ffmpeg + ffprobe once and cache. Never throws. */
export function detectFfmpeg(): FfmpegAvailability {
  if (cachedAvailability) return cachedAvailability;
  // 1. Prefer system PATH + well-known dirs
  const ffmpeg = findExecutable('ffmpeg') ?? findStaticBinary('ffmpeg');
  const ffprobe = findExecutable('ffprobe') ?? findStaticBinary('ffprobe');
  if (!ffmpeg || !ffprobe) {
    cachedAvailability = {
      ok: false,
      ffmpeg,
      ffprobe,
      reason:
        'ffmpeg/ffprobe not found on PATH or via ffmpeg-static. ' +
        'Install with `brew install ffmpeg` (macOS), ' +
        '`apt install ffmpeg` (Debian/Ubuntu), or see https://ffmpeg.org/download.html',
    };
  } else {
    cachedAvailability = { ok: true, ffmpeg, ffprobe };
  }
  return cachedAvailability;
}

/** Test seam: reset the availability cache. */
export function resetFfmpegDetectionForTests(value?: FfmpegAvailability): void {
  cachedAvailability = value;
}

export interface RunResult {
  code: number | null;
  signal: NodeJS.Signals | null;
  stdout: Buffer;
  stderr: string;
  aborted: boolean;
}

export interface RunOptions {
  cwd?: string;
  signal?: AbortSignal;
  timeoutMs?: number;
  /** Cap collected stdout (binary image bytes for image modes). */
  maxStdoutBytes?: number;
  /** Called with parsed `-progress` key/values when present on stdout. */
  onProgress?: (fields: Record<string, string>) => void;
}

const DEFAULT_TIMEOUT_MS = 120_000;
const DEFAULT_MAX_STDOUT = 32 * 1024 * 1024; // 32MB — image/gif output ceiling
export const MAX_FFPROBE_JSON_BYTES = 2 * 1024 * 1024;

/**
 * Spawn a binary with an explicit argv (no shell). Collects stdout as a Buffer
 * (some modes pipe an image to stdout), stderr as text. Honors AbortSignal and
 * a wall-clock timeout, escalating SIGTERM→SIGKILL like bash-tool.
 */
export function runBinary(bin: string, args: string[], opts: RunOptions = {}): Promise<RunResult> {
  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const maxStdout = opts.maxStdoutBytes ?? DEFAULT_MAX_STDOUT;

  return new Promise((resolve, reject) => {
    if (opts.signal?.aborted) {
      reject(new Error('Operation aborted'));
      return;
    }
    const child = spawn(bin, args, {
      cwd: opts.cwd,
      env: process.env,
      detached: process.platform !== 'win32',
    });

    const stdoutChunks: Buffer[] = [];
    let stdoutBytes = 0;
    let stderr = '';
    let settled = false;
    let aborted = false;
    let progressCarry = '';

    const kill = (sig: NodeJS.Signals) => {
      try {
        if (process.platform !== 'win32' && child.pid) process.kill(-child.pid, sig);
        else child.kill(sig);
      } catch {
        try { child.kill(sig); } catch { /* ignore */ }
      }
    };
    const terminate = () => {
      kill('SIGTERM');
      const escalate = setTimeout(() => { if (!settled) kill('SIGKILL'); }, 2000);
      escalate.unref?.();
    };

    const timer = timeoutMs > 0 ? setTimeout(() => {
      terminate();
      if (!settled) { settled = true; cleanup(); reject(new Error(`ffmpeg timed out after ${Math.round(timeoutMs / 1000)}s`)); }
    }, timeoutMs) : null;

    const onAbort = () => { aborted = true; terminate(); };
    opts.signal?.addEventListener('abort', onAbort, { once: true });

    const cleanup = () => {
      if (timer) clearTimeout(timer);
      opts.signal?.removeEventListener('abort', onAbort);
    };

    child.stdout?.on('data', (chunk: Buffer) => {
      if (opts.onProgress) {
        progressCarry += chunk.toString('utf8');
        const lines = progressCarry.split('\n');
        progressCarry = lines.pop() ?? '';
        const fields: Record<string, string> = {};
        for (const line of lines) {
          const eq = line.indexOf('=');
          if (eq > 0) fields[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
        }
        if (Object.keys(fields).length) opts.onProgress(fields);
      } else {
        stdoutBytes += chunk.length;
        if (stdoutBytes <= maxStdout) stdoutChunks.push(chunk);
      }
    });
    child.stderr?.on('data', (chunk: Buffer) => {
      if (stderr.length < 64 * 1024) stderr += chunk.toString('utf8');
    });

    child.on('error', (err) => {
      if (settled) return;
      settled = true; cleanup();
      reject(err);
    });
    child.on('close', (code, signal) => {
      if (settled) return;
      settled = true; cleanup();
      if (stdoutBytes > maxStdout) {
        reject(new Error(`ffmpeg output exceeded ${Math.round(maxStdout / (1024 * 1024))}MB limit`));
        return;
      }
      resolve({ code, signal, stdout: Buffer.concat(stdoutChunks), stderr, aborted });
    });
  });
}

/** Run ffmpeg. Rejects (with trimmed stderr) on non-zero exit. */
export async function runFfmpeg(args: string[], opts: RunOptions = {}): Promise<RunResult> {
  const det = detectFfmpeg();
  if (!det.ok || !det.ffmpeg) throw new Error(det.reason ?? 'ffmpeg unavailable');
  const res = await runBinary(det.ffmpeg, ['-hide_banner', '-nostdin', ...args], opts);
  if (res.aborted) throw new Error('Operation aborted');
  if (res.code !== 0) {
    const tail = res.stderr.trim().split('\n').slice(-4).join('\n') || `exited with code ${res.code}`;
    throw new Error(`ffmpeg failed: ${tail}`);
  }
  return res;
}

/** Run ffprobe and parse its JSON output. */
export async function runFfprobeJson(input: string, opts: RunOptions = {}): Promise<Record<string, unknown>> {
  const det = detectFfmpeg();
  if (!det.ok || !det.ffprobe) throw new Error(det.reason ?? 'ffprobe unavailable');
  const args = ['-hide_banner', '-v', 'error', '-print_format', 'json', '-show_format', '-show_streams', input];
  const res = await runBinary(det.ffprobe, args, {
    ...opts,
    maxStdoutBytes: Math.min(opts.maxStdoutBytes ?? MAX_FFPROBE_JSON_BYTES, MAX_FFPROBE_JSON_BYTES),
  });
  if (res.code !== 0) {
    const tail = res.stderr.trim().split('\n').slice(-3).join('\n') || `exited with code ${res.code}`;
    throw new Error(`ffprobe failed: ${tail}`);
  }
  try {
    return JSON.parse(res.stdout.toString('utf8')) as Record<string, unknown>;
  } catch {
    throw new Error('ffprobe returned unparseable JSON');
  }
}
