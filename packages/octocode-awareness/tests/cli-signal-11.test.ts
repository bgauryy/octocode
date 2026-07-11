/**
 * cli.test.ts — subprocess-based CLI contract tests for out/octocode-awareness.js.
 *
 * These tests exercise the compiled CLI binary end-to-end via spawnSync,
 * verifying the exact JSON output shapes that hook scripts and pi-extension depend on.
 */
import { describe, it, expect } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/octocode-awareness.js');
const NODE = process.execPath;
// ─── Helpers ─────────────────────────────────────────────────────────────────
function mktemp(): string {
    return mkdtempSync(join(tmpdir(), 'oc-cli-test-'));
}
interface RunResult {
    status: number;
    stdout: string;
    stderr: string;
    parsed: Record<string, unknown> | null;
}
function run(dbPath: string, args: string[], opts: {
    cwd?: string;
} = {}): RunResult {
    const result = spawnSync(NODE, [SCRIPT, '--db', dbPath, ...args], {
        cwd: opts.cwd ?? process.cwd(),
        encoding: 'utf8',
        // repo inject / heavy CLI paths can exceed 10s on cold machines
        timeout: 30000,
    });
    let parsed: Record<string, unknown> | null = null;
    try {
        parsed = JSON.parse(result.stdout) as Record<string, unknown>;
    }
    catch { /* non-JSON */ }
    return { status: result.status ?? 1, stdout: result.stdout, stderr: result.stderr, parsed };
}
function ok(dbPath: string, args: string[], opts: {
    cwd?: string;
} = {}): Record<string, unknown> {
    const r = run(dbPath, args, opts);
    expect(r.status, `expected exit 0 for ${args[0]}: stderr=${r.stderr} stdout=${r.stdout}`).toBe(0);
    expect(r.parsed?.['ok'], `expected ok:true for ${args[0]}: ${r.stdout}`).not.toBe(false);
    return r.parsed!;
}

// ─── signal ──────────────────────────────────────────────────────────────────

describe('signal', () => {
  it('preserves publish kind while still allowing kind filters', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      ok(db, [
        'signal', 'publish',
        '--agent-id', 'agent-a',
        '--to-agent', 'agent-b',
        '--kind', 'question',
        '--subject', 'Kind check',
        '--workspace', dir,
      ]);

      const listed = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-b',
        '--kind', 'question',
        '--workspace', dir,
      ]);
      expect(listed['count']).toBe(1);
      const signal = (listed['signals'] as Record<string, unknown>[])[0]!;
      expect(signal['kind']).toBe('question');
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('accepts signal list --format hook for host briefing shape', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      ok(db, [
        'memory', 'record',
        '--agent-id', 'agent-a',
        '--workspace', dir,
        '--task-context', 'hook briefing memory',
        '--observation', 'selective reminder should surface in hook format',
        '--importance', '8',
        '--label', 'GOTCHA',
      ]);

      const briefing = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-b',
        '--workspace', dir,
        '--format', 'hook',
        '--compact',
      ]);
      expect(briefing).not.toHaveProperty('error');
      expect(
        briefing['additionalContext'] != null || Number(briefing['count'] ?? 0) === 0,
      ).toBe(true);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('summarizes signal list bodies unless --include-bodies', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    const longBody = `Long signal body ${'x'.repeat(220)}`;
    try {
      ok(db, [
        'signal', 'publish',
        '--agent-id', 'agent-a',
        '--to-agent', 'agent-b',
        '--kind', 'fyi',
        '--subject', 'Body trim',
        '--body', longBody,
        '--workspace', dir,
      ]);

      const summarized = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-b',
        '--workspace', dir,
      ]);
      expect(summarized['bodies']).toBe('summarized');
      const shortBody = String((summarized['signals'] as Record<string, unknown>[])[0]!['body'] ?? '');
      expect(shortBody.length).toBeLessThanOrEqual(160);
      expect(shortBody.endsWith('...')).toBe(true);

      const full = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-b',
        '--workspace', dir,
        '--include-bodies',
        '--compact',
      ]);
      expect(full).not.toHaveProperty('bodies');
      expect(String((full['signals'] as Record<string, unknown>[])[0]!['body'])).toBe(longBody);
    } finally { rmSync(dir, { recursive: true }); }
  });

  it('round-trips reply, ack, and resolve through the CLI', () => {
    const dir = mktemp();
    const db = join(dir, 'test.sqlite3');
    try {
      const published = ok(db, [
        'signal', 'publish',
        '--agent-id', 'agent-a',
        '--to-agent', 'agent-b',
        '--kind', 'question',
        '--subject', 'Need answer',
        '--workspace', dir,
      ]);
      const parentId = published['signal_id'] as string;
      const threadId = published['thread_id'] as string;

      const reply = ok(db, [
        'signal', 'reply',
        '--agent-id', 'agent-b',
        '--to-agent', 'agent-a',
        '--subject', 'Answer',
        '--body', 'Done',
        '--in-reply-to', parentId,
        '--workspace', dir,
      ]);
      const replyId = reply['signal_id'] as string;
      expect(reply['thread_id']).toBe(threadId);
      expect(replyId).not.toBe(parentId);

      const acked = ok(db, [
        'signal', 'ack',
        '--agent-id', 'agent-a',
        '--signal-id', replyId,
        '--workspace', dir,
      ]);
      expect(acked['acknowledged']).toBe(1);
      expect(acked['signal_ids']).toEqual([replyId]);

      const resolved = ok(db, [
        'signal', 'resolve',
        '--agent-id', 'agent-a',
        '--thread-id', threadId,
        '--workspace', dir,
      ]);
      expect(resolved['resolved']).toBe(2);
      expect(resolved['signal_ids']).toEqual(expect.arrayContaining([parentId, replyId]));

      const listed = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-a',
        '--thread-id', threadId,
        '--all',
        '--workspace', dir,
      ]);
      expect(listed['count']).toBe(2);
      expect((listed['signals'] as Record<string, unknown>[]).every((s) => s['status'] === 'resolved')).toBe(true);

      ok(db, [
        'signal', 'publish',
        '--agent-id', 'agent-b',
        '--to-agent', 'agent-a',
        '--kind', 'fyi',
        '--subject', 'Unrelated signal',
        '--workspace', dir,
      ]);
      const selected = ok(db, [
        'signal', 'list',
        '--agent-id', 'agent-a',
        '--signal-id', replyId,
        '--all',
        '--workspace', dir,
      ]);
      expect(selected['count']).toBe(1);
      expect((selected['signals'] as Record<string, unknown>[])[0]!['signal_id']).toBe(replyId);
    } finally { rmSync(dir, { recursive: true }); }
  });
});
