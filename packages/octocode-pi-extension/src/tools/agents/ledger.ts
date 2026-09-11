/**
 * Agent ledger: ledger event listeners, push/touch/notify primitives, entry
 * serialisation, and ledger query functions.
 *
 * Depends on: types (← leaf), registry (← types).
 * No imports from agent-tools or other agents sub-modules.
 */
import fs from 'node:fs';
import type {
  WorkerLedgerEntry,
  WorkerLedgerEventType,
  WorkerMessageActivity,
  WorkerWorktreeState,
} from '../../types.js';
import {
  type AgentRecord,
  type AgentStatus,
  MAX_STORED_EVENTS,
  MAX_LEDGER_EVENTS,
} from './types.js';
import { agents, getLedgerHidden, isProcessAlive } from './registry.js';

// ─── Ledger listeners ──────────────────────────────────────────────────────

// Ledger listeners: notified on every ledger event (spawned/status/tool/handback/…).
// Normalized-status flips also funnel through here — refreshNormalizedResult pushes a
// 'handback' ledger event whenever the normalized status changes, so subscribing to
// pushLedgerEvent covers all worker state transitions.
export const ledgerListeners = new Set<(entry: WorkerLedgerEntry, type: WorkerLedgerEventType) => void>();

/** Subscribe to worker ledger events. Returns an unsubscribe function. */
export function registerWorkerLedgerListener(
  cb: (entry: WorkerLedgerEntry, type: WorkerLedgerEventType) => void,
): () => void {
  ledgerListeners.add(cb);
  return () => { ledgerListeners.delete(cb); };
}

// ─── Core primitives ──────────────────────────────────────────────────────────

export function pushCapped<T>(items: T[], item: T): void {
  items.push(item);
  if (items.length > MAX_STORED_EVENTS) items.splice(0, items.length - MAX_STORED_EVENTS);
}

export function touch(record: AgentRecord, status?: AgentStatus): void {
  record.updatedAt = Date.now();
  if (status) record.status = status;
  // Every touch is driven by an inbound RPC event (tool call, output delta, turn
  // boundary, …) — i.e. proof the worker is alive and progressing. Fan it out to
  // any blocking waiter so it can reset its silence watchdog. A throwing listener
  // must never break the event pipeline.
  if (record.activityListeners.size > 0) {
    for (const listener of record.activityListeners) {
      try { listener(); } catch { /* listener errors are isolated */ }
    }
  }
}

/**
 * Mark that a turn has been queued to the worker but has not started yet. Bumps
 * pendingMessages (drives the 'queued' display state and keeps `wait` blocking)
 * and refreshes the timestamp without faking a 'running' status.
 */
export function enqueueWorkerTurn(record: AgentRecord): void {
  record.pendingMessages += 1;
  touch(record);
}

export function notifyWaiters(record: AgentRecord): void {
  for (const waiter of record.waiters) waiter();
  record.waiters.clear();
}

export function pushLedgerEvent(
  record: AgentRecord,
  type: WorkerLedgerEventType,
  message?: string,
  details?: unknown,
): void {
  pushCapped(record.ledgerEvents, {
    type,
    timestamp: Date.now(),
    message,
    details,
  });
  if (record.ledgerEvents.length > MAX_LEDGER_EVENTS) {
    record.ledgerEvents.splice(0, record.ledgerEvents.length - MAX_LEDGER_EVENTS);
  }
  if (ledgerListeners.size > 0) {
    const entry = toWorkerLedgerEntry(record);
    for (const listener of ledgerListeners) {
      // A throwing listener must never break the ledger (or the worker pipeline).
      try { listener(entry, type); } catch { /* listener errors are isolated */ }
    }
  }
}

// ─── Message activity ───────────────────────────────────────────────────────

export function previewMessage(message: string): string {
  const oneLine = message.replace(/\s+/g, ' ').trim();
  return oneLine.length > 72 ? `${oneLine.slice(0, 71)}\u2026` : oneLine;
}

export function recordMessageActivity(
  record: AgentRecord,
  direction: WorkerMessageActivity['direction'],
  action: WorkerMessageActivity['action'],
  message: string,
  ledgerMessage: string,
): void {
  record.lastMessage = {
    direction,
    action,
    preview: previewMessage(message),
    timestamp: Date.now(),
  };
  pushLedgerEvent(record, 'message', ledgerMessage, record.lastMessage);
}

// ─── Ledger entry serialisation helpers (local) ───────────────────────────────

function getArgValue(args: string[], flag: string): string | undefined {
  const index = args.indexOf(flag);
  return index >= 0 ? args[index + 1] : undefined;
}

function getArgCsv(args: string[], flag: string): string[] | undefined {
  const value = getArgValue(args, flag);
  return value ? value.split(',').map((item) => item.trim()).filter(Boolean) : undefined;
}

function statHandbackArtifact(
  filePath: string,
): { path: string; exists: boolean; bytes?: number; modifiedAt?: string } {
  try {
    const stat = fs.statSync(filePath);
    if (!stat.isFile()) return { path: filePath, exists: false };
    return { path: filePath, exists: true, bytes: stat.size, modifiedAt: stat.mtime.toISOString() };
  } catch {
    return { path: filePath, exists: false };
  }
}

function worktreeSnapshot(worktree: WorkerWorktreeState | undefined): WorkerWorktreeState | undefined {
  return worktree ? { ...worktree } : undefined;
}

// ─── Ledger entry builders ─────────────────────────────────────────────────────

export function toWorkerLedgerEntry(record: AgentRecord): WorkerLedgerEntry {
  const normalized = record.normalizedResult;
  const activeTool = [...record.toolCalls].reverse().find((call) => call.status === 'running')?.toolName;
  const toolNames = [...new Set(record.toolCalls.map((call) => call.toolName).filter(Boolean))].slice(0, 4);
  return {
    agentId: record.id,
    name: record.name,
    status: record.status,
    startedAt: new Date(record.startedAt).toISOString(),
    updatedAt: new Date(record.updatedAt).toISOString(),
    model: getArgValue(record.args, '--model'),
    provider: getArgValue(record.args, '--provider'),
    task: record.task,
    planStep: record.planStep,
    cohortId: record.cohortId,
    thinking: getArgValue(record.args, '--thinking'),
    tools: getArgCsv(record.args, '--tools'),
    normalizedStatus: normalized?.status,
    result: normalized?.result,
    confidence: normalized?.confidence,
    evidence: normalized?.evidence,
    verification: normalized?.verification,
    next: normalized?.next,
    artifact: normalized?.artifact,
    deltaSummary: record.deltaSummary,
    handback: statHandbackArtifact(record.handbackPath),
    pendingMessages: record.pendingMessages,
    lastMessage: record.lastMessage,
    activeTool,
    toolCallCount: record.toolCalls.length,
    toolNames,
    worktree: worktreeSnapshot(record.worktree),
    recentEvents: record.ledgerEvents.slice(-10),
  };
}

// ─── Ledger query functions ─────────────────────────────────────────────────────

export function listWorkerLedgerEntries(): WorkerLedgerEntry[] {
  return [...agents.values()]
    .sort((a, b) => b.updatedAt - a.updatedAt)
    .map(toWorkerLedgerEntry);
}

/** Footer projection respects the user's explicit hide/clear choice. */
export function listVisibleWorkerLedgerEntries(): WorkerLedgerEntry[] {
  return getLedgerHidden() ? [] : listWorkerLedgerEntries();
}

/** A reusable idle worker still owns its assignment until its process exits. */
export function findLivePlanWorker(
  planScope: string,
  planId: string,
  planStep: string,
): string | undefined {
  return [...agents.values()].find(
    (record) =>
      record.planScope === planScope &&
      record.planId === planId &&
      record.planStep === planStep &&
      isProcessAlive(record),
  )?.id;
}
