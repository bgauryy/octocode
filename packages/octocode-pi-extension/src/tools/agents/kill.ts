/**
 * Agent kill helpers: process termination and prompt-file cleanup.
 *
 * Depends only on: types, registry, ledger, storage-policy, node builtins.
 * No imports from process, wait, or agent-tools.
 */
import fs from 'node:fs';
import path from 'node:path';
import type { AgentRecord } from './types.js';
import { revokeWorkerCapabilities } from '../worker-capabilities.js';
import {
  isProcessAlive,
  findAgentByIdOrPrefix,
} from './registry.js';
import {
  touch,
  pushLedgerEvent,
  notifyWaiters,
} from './ledger.js';

// ─── Prompt-file cleanup ──────────────────────────────────────────────────────

/** Remove all temp prompt files written for this worker. Best-effort. */
export function removePromptFiles(record: AgentRecord): void {
  for (const filePath of record.promptFiles) {
    try {
      fs.rmSync(path.dirname(filePath), { recursive: true, force: true });
    } catch {
      // best-effort cleanup only
    }
  }
  record.promptFiles = [];
}

// ─── Kill ─────────────────────────────────────────────────────────────────────

/**
 * Terminate a worker. Sends SIGTERM immediately, then schedules a SIGKILL after
 * `forceKillDelayMs` ms if the process has not exited.
 *
 * Invariants:
 * - pendingMessages zeroed BEFORE touch('killed') so isTerminal() becomes true
 *   and a pending wait resolves rather than hanging on a phantom 'queued' state.
 * - SIGTERM first, SIGKILL after delay — gives Pi a chance to flush/abort cleanly.
 * - notifyWaiters AFTER all state mutations so waiters see the final state.
 */
export function killAgent(record: AgentRecord, opts: { forceKillDelayMs?: number } = {}): void {
  revokeWorkerCapabilities(record.id);
  pushLedgerEvent(record, 'killed', 'kill requested');
  // The process is going away, so any queued turns will never emit agent_start
  // to decrement this. Strand them at zero here, or isTerminal() stays false
  // forever and a later `wait` blocks its full timeout on a dead worker while
  // the ledger advertises a phantom 'queued' state.
  record.pendingMessages = 0;
  touch(record, 'killed');
  try {
    record.process.stdin.end?.();
  } catch {
    // ignore stdin close errors
  }
  record.process.kill('SIGTERM');
  // NOTE: ChildProcess.killed only means "a signal was delivered", not "process
  // exited" — it is true immediately after SIGTERM above, so it cannot gate the
  // SIGKILL escalation. Gate on actual liveness (exitCode/signalCode still null).
  const forceKillDelayMs = opts.forceKillDelayMs ?? 5000;
  if (forceKillDelayMs <= 0) {
    if (isProcessAlive(record)) record.process.kill('SIGKILL');
  } else {
    setTimeout(() => {
      if (isProcessAlive(record)) record.process.kill('SIGKILL');
    }, forceKillDelayMs).unref?.();
  }
  removePromptFiles(record);
  notifyWaiters(record);
}

// ─── Public wrappers ─────────────────────────────────────────────────────────

/** Kill a worker by id or prefix (same path as agent kill). Returns false for unknown ids. */
export function killWorkerById(idOrPrefix: string): boolean {
  const record = findAgentByIdOrPrefix(idOrPrefix);
  if (!record) return false;
  killAgent(record);
  return true;
}
