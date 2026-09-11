/**
 * Background job infrastructure.
 *
 * This module is pure infrastructure with no Pi-extension wiring.
 * bash-tool.ts imports from here and registers everything under the unified `bash` tool.
 */

import { randomUUID }        from 'node:crypto';
import { createWriteStream }  from 'node:fs';
import { readFileSync }       from 'node:fs';
import { tmpdir }             from 'node:os';
import { join }               from 'node:path';
import { spawn }              from 'node:child_process';

import type { RuntimeBackgroundJobState } from './runtime-store.js';

// ─── constants ────────────────────────────────────────────────────────────────

export const BASH_BG_DEFAULT_TIMEOUT_S = 1800;          // 30 min

const JOB_TTL_MS   = 4 * 60 * 60 * 1_000; // keep finished jobs 4 h
const MAX_JOBS     = 30;
const KILL_GRACE_MS = 5_000;
const ANSI_RE      = /\x1B\[[0-?]*[ -/]*[@-~]/g;

// ─── types ───────────────────────────────────────────────────────────────────

export type BgJobStatus = 'running' | 'succeeded' | 'failed' | 'timed_out' | 'killed';

export interface BgJob {
  id:             string;
  title:          string;
  command:        string;
  cwd:            string;
  startedAt:      number;
  endedAt?:       number;
  updatedAt:      number;
  status:         BgJobStatus;
  exitCode?:      number | null;
  logPath:        string;
  pid?:           number;
  timeoutSeconds: number;
}

export const BG_STATUS_ICON: Record<BgJobStatus, string> = {
  running:   '⟳',
  succeeded: '✓',
  failed:    '✗',
  killed:    '⊘',
  timed_out: '⏱',
};

// ─── helpers ──────────────────────────────────────────────────────────────────

export function sanitizeBgTitle(s: string): string {
  return [...s.replace(ANSI_RE, '')]
    .map(c => { const cp = c.codePointAt(0) ?? 0; return cp <= 0x1f || (cp >= 0x7f && cp <= 0x9f) ? ' ' : c; })
    .join('').replace(/\s+/g, ' ').trim();
}

export function formatBgElapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1_000));
  if (s < 60)  return `${s}s`;
  const m = Math.floor(s / 60), rs = s % 60;
  if (m < 60)  return rs > 0 ? `${m}m ${rs}s` : `${m}m`;
  const h = Math.floor(m / 60), rm = m % 60;
  return rm > 0 ? `${h}h ${rm}m` : `${h}h`;
}

export function projectBackgroundJobs(jobs: readonly BgJob[]): RuntimeBackgroundJobState[] {
  return jobs.map((job) => ({
    id: job.id,
    title: job.title,
    status: job.status,
    startedAt: job.startedAt,
    ...(job.endedAt !== undefined ? { endedAt: job.endedAt } : {}),
    updatedAt: job.updatedAt,
    ...(job.exitCode !== undefined ? { exitCode: job.exitCode } : {}),
  }));
}

// ─── JobManager ──────────────────────────────────────────────────────────────

export class JobManager {
  private jobs  = new Map<string, BgJob>();
  private procs = new Map<string, ReturnType<typeof spawn>>();

  constructor(private readonly onChange: () => void) {}

  async start(
    command:        string,
    cwd:            string,
    timeoutSeconds: number,
    title?:         string,
    onDone?:        (j: BgJob) => void,
  ): Promise<BgJob> {
    this.evict();
    const id      = randomUUID().slice(0, 8);
    const logPath = join(tmpdir(), `pi-bg-${id}.log`);
    const startedAt = Date.now();
    const job: BgJob = {
      id, command, cwd, logPath, timeoutSeconds,
      title:     sanitizeBgTitle(title ?? command.slice(0, 70).replace(/\n/g, ' ')),
      startedAt,
      updatedAt: startedAt,
      status:    'running',
    };
    this.jobs.set(id, job);
    this.onChange();

    const logStream = createWriteStream(logPath, { flags: 'a' });
    const child     = spawn('/bin/sh', ['-c', command], {
      cwd,
      detached: false,
      stdio:    ['ignore', 'pipe', 'pipe'],
    });
    job.pid = child.pid;
    this.procs.set(id, child);
    child.stdout?.pipe(logStream);
    child.stderr?.pipe(logStream);

    let timedOut = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    if (timeoutSeconds > 0) {
      timer = setTimeout(() => { timedOut = true; this._terminate(child); }, timeoutSeconds * 1_000);
    }

    child.on('close', code => {
      clearTimeout(timer);
      logStream.end();
      job.endedAt  = Date.now();
      job.updatedAt = job.endedAt;
      job.exitCode = code;
      if (job.status === 'running') {
        job.status = timedOut ? 'timed_out' : (code === 0 ? 'succeeded' : 'failed');
      }
      this.procs.delete(id);
      this.onChange();
      onDone?.(job);
    });

    return job;
  }

  get(id: string): BgJob | undefined { return this.jobs.get(id); }
  list():    BgJob[] { return [...this.jobs.values()]; }
  running(): BgJob[] { return [...this.jobs.values()].filter(j => j.status === 'running'); }

  output(id: string, offset = 0, lines = 200): { text: string; nextOffset: number } {
    const job = this.jobs.get(id);
    if (!job) return { text: `Job '${id}' not found.`, nextOffset: 0 };
    try {
      const raw  = readFileSync(job.logPath, 'utf8');
      const all  = raw.split('\n');
      const page = all.slice(offset, offset + lines);
      return { text: page.join('\n') || '(no output yet)', nextOffset: offset + page.length };
    } catch {
      return { text: '(log not yet available)', nextOffset: 0 };
    }
  }

  kill(id: string): void {
    const job   = this.jobs.get(id);
    const child = this.procs.get(id);
    if (!job || job.status !== 'running') return;
    job.status = 'killed';
    job.updatedAt = Date.now();
    if (child) this._terminate(child);
    this.onChange();
  }

  killAll(): void {
    for (const id of this.procs.keys()) this.kill(id);
  }

  dispose(): void {
    this.killAll();
    this.jobs.clear();
    this.onChange();
  }

  private evict(): void {
    if (this.jobs.size < MAX_JOBS) return;
    const now     = Date.now();
    const expired = [...this.jobs.values()]
      .filter(j => j.status !== 'running' && now - (j.endedAt ?? j.startedAt) > JOB_TTL_MS)
      .map(j => j.id);
    for (const id of expired) this.jobs.delete(id);
  }

  private _terminate(child: ReturnType<typeof spawn>): void {
    try { child.kill('SIGTERM'); } catch {}
    const t = setTimeout(() => { try { child.kill('SIGKILL'); } catch {} }, KILL_GRACE_MS);
    child.once('close', () => clearTimeout(t));
  }
}
