/**
 * TDD tests for bash-tool.ts security hardening.
 * H1: Expanded catastrophic command patterns.
 * H2: Write-target detection completeness.
 *
 * These tests are RED against the un-patched source because:
 * - assertBashCommandAllowed is not yet exported.
 * - The BLOCKED_COMMAND_PATTERNS list does not yet include shutdown/reboot/halt/poweroff.
 */
import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  assertBashCommandAllowed,
  extractBashWriteTargets,
} from '../src/tools/bash-tool.js';

const CWD = '/tmp/work';

// ─── H1: Catastrophic power-state commands ───────────────────────────────────

test('H1: blocks shutdown -h now', () => {
  assert.throws(
    () => assertBashCommandAllowed('shutdown -h now', CWD),
    /bash blocked|catastrophic/i,
  );
});

test('H1: blocks bare reboot', () => {
  assert.throws(
    () => assertBashCommandAllowed('reboot', CWD),
    /bash blocked|catastrophic/i,
  );
});

test('H1: blocks halt', () => {
  assert.throws(
    () => assertBashCommandAllowed('halt', CWD),
    /bash blocked|catastrophic/i,
  );
});

test('H1: blocks poweroff', () => {
  assert.throws(
    () => assertBashCommandAllowed('poweroff', CWD),
    /bash blocked|catastrophic/i,
  );
});

test('H1: blocks sudo reboot', () => {
  assert.throws(
    () => assertBashCommandAllowed('sudo reboot', CWD),
    /bash blocked|catastrophic/i,
  );
});

test('H1: blocks reboot after semicolon separator', () => {
  assert.throws(
    () => assertBashCommandAllowed('echo done; reboot', CWD),
    /bash blocked|catastrophic/i,
  );
});

test('H1: blocks shutdown after && separator', () => {
  assert.throws(
    () => assertBashCommandAllowed('make build && shutdown -h now', CWD),
    /bash blocked|catastrophic/i,
  );
});

// ─── H1: False-positive guard — these must NOT be blocked ────────────────────

test('H1: does NOT block echo that mentions shutdown in quoted string', () => {
  // 'shutdown' inside a single-quoted string — not a command invocation
  assert.doesNotThrow(() =>
    assertBashCommandAllowed("echo 'system shutdown notice sent'", CWD),
  );
});

test('H1: does NOT block git commit message mentioning reboot', () => {
  assert.doesNotThrow(() =>
    assertBashCommandAllowed('git commit -m "fix: reboot handling improved"', CWD),
  );
});

test('H1: does NOT block a shell variable name containing reboot', () => {
  // The word "rebooting" ends with word characters, so \b anchoring preserves it
  // only when the command itself matches; a variable like REBOOTING_FLAG is not affected
  assert.doesNotThrow(() =>
    assertBashCommandAllowed('echo $REBOOTING_FLAG', CWD),
  );
});

// ─── H2: Write-target detection ──────────────────────────────────────────────

test('H2: detects exec > redirect', () => {
  const targets = extractBashWriteTargets('exec > /tmp/out.log', CWD);
  assert.ok(
    targets.includes('/tmp/out.log'),
    `exec > not detected; got: ${JSON.stringify(targets)}`,
  );
});

test('H2: detects exec >> append redirect', () => {
  const targets = extractBashWriteTargets('exec >> /tmp/appended.log', CWD);
  assert.ok(targets.includes('/tmp/appended.log'));
});

test('H2: variable-expansion redirect: extractor records token (contract unchanged), assertBashCommandAllowed now rejects it (no longer fail-open)', () => {
  // extractBashWriteTargets still records the raw token for callers.
  const targets = extractBashWriteTargets('echo hi > $OUTFILE', CWD);
  assert.ok(targets.length > 0, 'extractor still records the raw token');
  // assertBashCommandAllowed rejects before the path guard runs.
  assert.throws(
    () => assertBashCommandAllowed('echo hi > $OUTFILE', CWD),
    /shell variable|command substitution|literal path/i,
    'ambiguous $VAR redirect must be rejected, not silently treated as relative to cwd'
  );
});

// ─── H2b: Shell-expansion rejection regressions ─────────────────────────────

test('H2b: ${VAR} redirect is rejected', () => {
  assert.throws(
    () => assertBashCommandAllowed('echo hi > ${OUTFILE}', CWD),
    /shell variable|command substitution|literal path/i,
  );
});

test('H2b: $(cmd) redirect is rejected', () => {
  assert.throws(
    () => assertBashCommandAllowed('echo hi > $(mktemp)', CWD),
    /shell variable|command substitution|literal path/i,
  );
});

test('H2b: backtick redirect is rejected', () => {
  assert.throws(
    () => assertBashCommandAllowed('echo hi > `mktemp`', CWD),
    /shell variable|command substitution|literal path/i,
  );
});

test('H2b: tee with $VAR target is rejected', () => {
  assert.throws(
    () => assertBashCommandAllowed('echo hi | tee $LOGFILE', CWD),
    /shell variable|command substitution|literal path/i,
  );
});

test('H2b: cp with $VAR destination is rejected', () => {
  assert.throws(
    () => assertBashCommandAllowed('cp src.txt $DEST', CWD),
    /shell variable|command substitution|literal path/i,
  );
});

test('H2b: plain literal redirect within cwd is still allowed', () => {
  assert.doesNotThrow(() => assertBashCommandAllowed('echo hi > output.txt', CWD));
});

test('H2b: /dev/null redirect is always allowed (never blocked as ambiguous)', () => {
  assert.doesNotThrow(() => assertBashCommandAllowed('echo hi > /dev/null', CWD));
});

// ─── H3: In-place editor write targets ───────────────────────────────────────

test('H3: sed -i file is detected as a write target', () => {
  const targets = extractBashWriteTargets("sed -i 's/x/y/' /etc/hosts", CWD);
  assert.ok(targets.includes('/etc/hosts'), `sed -i target not detected; got ${JSON.stringify(targets)}`);
});

test('H3: sed -i.bak with multiple files detects all files', () => {
  const targets = extractBashWriteTargets("sed -i.bak 's/a/b/' /tmp/a.txt /tmp/b.txt", CWD);
  assert.ok(targets.includes('/tmp/a.txt') && targets.includes('/tmp/b.txt'));
});

test('H3: perl -i -pe file is detected', () => {
  const targets = extractBashWriteTargets("perl -i -pe 's/x/y/' /etc/passwd", CWD);
  assert.ok(targets.includes('/etc/passwd'));
});

test('H3: sed WITHOUT -i (read-only) records no write target', () => {
  const targets = extractBashWriteTargets("sed 's/x/y/' /etc/hosts", CWD);
  assert.equal(targets.includes('/etc/hosts'), false, 'read-only sed must not be flagged as a write');
});

test('H3: assertBashCommandAllowed blocks sed -i outside allowed roots', () => {
  assert.throws(() => assertBashCommandAllowed("sed -i 's/x/y/' /etc/hosts", CWD));
});

// ─── H4: Env-exfil not hidden by a concurrent sensitive classification ─────────
//
// Regression: `const sensitive = classify(...) ?? classifyEnvExfil(...)` silently
// skipped classifyEnvExfilCommand whenever classifySensitiveCommand returned non-null.
// A compound `git push && env` with git-write always-allowed would exfiltrate env vars.

import { allowAlways, resetApprovalStore } from '../src/tools/approval.js';
import { registerBashTool } from '../src/tools/bash-tool.js';
import { registerUniqueTool } from '../src/tools/octocode-tools.js';
import { Type } from 'typebox';
import type { ToolDefinition, ToolCallResult } from '../src/types.js';
import os from 'node:os';

function loadBashToolForH4(): ToolDefinition {
  let def: ToolDefinition | undefined;
  registerBashTool({ registerTool: (d: ToolDefinition) => { def = d; } }, Type as never, new Set<string>(), registerUniqueTool);
  assert.ok(def, 'bash tool registered');
  return def!;
}

function execBashH4(
  tool: object,
  id: string,
  params: Record<string, unknown>,
  ctx?: { cwd?: string },
): Promise<ToolCallResult> {
  const definition = tool as ToolDefinition;
  return definition.execute(id, { queries: [params] }, undefined, undefined, ctx);
}

test('H4: env-exfil approval fires even when classifySensitiveCommand also matches (no ?? hidden-gate regression)', async () => {
  resetApprovalStore();
  allowAlways('git-write'); // auto-approve git-write so only env-exfil would block
  const tool = loadBashToolForH4();
  // Before the fix: git push satisfied `??`, env-exfil was silently skipped.
  await assert.rejects(
    () => execBashH4(tool, 'h4', { command: 'git push && env', reasoning: 'hidden exfil regression' }, { cwd: os.tmpdir() }),
    /Expose inherited environment variables.*requires user approval.*non-interactive/i,
    'env-exfil must still prompt even when another approval class is auto-approved'
  );
});

test('H4: standalone env blocked non-interactively (baseline unchanged)', async () => {
  resetApprovalStore();
  const tool = loadBashToolForH4();
  await assert.rejects(
    () => execBashH4(tool, 'h4b', { command: 'env', reasoning: 'baseline' }, { cwd: os.tmpdir() }),
    /Expose inherited environment variables.*requires user approval.*non-interactive/i,
  );
});
