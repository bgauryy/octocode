import {
  effectiveCapabilityDecision,
  parseAgentEventEnvelopeV1,
  parseAuthorizationReceiptV1,
  parseInteractionAnswerV1,
  parseInteractionRequestV1,
  type AgentEventEnvelopeV1,
  type AuthorizationReceiptV1,
  type CapabilityDecisionReceiptV1,
  type InboundDecision,
  type InteractionAnswerV1,
  type InteractionRequestV1,
} from '../continuity-contracts.js';
import { CoordinationPlanGraph } from './coordination-plan-graph.js';
import { id, now, required } from './coordination-shared.js';

export interface OutboxEventV1<T = unknown> extends AgentEventEnvelopeV1<T> { sequence: number }
export interface StoredInteractionV1 {
  request: InteractionRequestV1;
  status: InteractionRequestV1['status'];
  answer?: InteractionAnswerV1;
  resolvedAt?: string;
}

interface OutboxRow {
  sequence: number;
  event_id: string;
  workspace_path: string;
  event_type: string;
  aggregate_kind: string | null;
  aggregate_id: string | null;
  aggregate_revision: string | null;
  actor_json: string;
  provenance_json: string;
  payload_json: string;
  session_id: string | null;
  correlation_id: string | null;
  created_at: string;
  expires_at: string | null;
}

function eventFromRow(row: OutboxRow): OutboxEventV1 {
  return {
    sequence: row.sequence,
    version: 1,
    eventId: row.event_id,
    workspace: row.workspace_path,
    ...(row.session_id ? { sessionId: row.session_id } : {}),
    ...(row.correlation_id ? { correlationId: row.correlation_id } : {}),
    type: row.event_type,
    actor: JSON.parse(row.actor_json) as AgentEventEnvelopeV1['actor'],
    provenance: JSON.parse(row.provenance_json) as AgentEventEnvelopeV1['provenance'],
    ...(row.aggregate_kind && row.aggregate_id ? { aggregate: {
      kind: row.aggregate_kind,
      id: row.aggregate_id,
      ...(row.aggregate_revision ? { revision: row.aggregate_revision } : {}),
    } } : {}),
    createdAt: row.created_at,
    ...(row.expires_at ? { expiresAt: row.expires_at } : {}),
    payload: JSON.parse(row.payload_json) as unknown,
  };
}

export class AwarenessStore extends CoordinationPlanGraph {
  appendEvent(input: AgentEventEnvelopeV1): OutboxEventV1 {
    const event = parseAgentEventEnvelopeV1(input);
    if (event.workspace !== this.workspace) throw new Error('event workspace does not match the opened Awareness store');
    return this.writeTransaction(() => ({ ...event, sequence: this.insertOutboxEvent(event) }));
  }

  listEvents(params: { consumerId: string; limit?: number }): OutboxEventV1[] {
    const consumerId = required(params.consumerId, 'consumer-id');
    const cursor = this.db.prepare('SELECT sequence FROM event_consumers WHERE workspace_path = ? AND consumer_id = ?')
      .get(this.workspace, consumerId) as { sequence: number } | undefined;
    const limit = Math.min(Math.max(params.limit ?? 100, 1), 1000);
    const rows = this.db.prepare('SELECT * FROM event_outbox WHERE workspace_path = ? AND sequence > ? ORDER BY sequence ASC LIMIT ?')
      .all(this.workspace, cursor?.sequence ?? 0, limit) as unknown as OutboxRow[];
    return rows.map(eventFromRow);
  }

  acknowledgeEvent(params: { consumerId: string; eventId: string; decision: InboundDecision }): { sequence: number; decision: InboundDecision; duplicate: boolean } {
    return this.writeTransaction(() => {
      const consumerId = required(params.consumerId, 'consumer-id');
      const eventId = required(params.eventId, 'event-id');
      if (!(['accept', 'hold', 'refuse'] as const).includes(params.decision)) throw new Error('decision is invalid');
      const event = this.db.prepare('SELECT sequence FROM event_outbox WHERE workspace_path = ? AND event_id = ?')
        .get(this.workspace, eventId) as { sequence: number } | undefined;
      if (!event) throw new Error(`unknown event: ${eventId}`);
      const prior = this.db.prepare('SELECT decision FROM event_acknowledgements WHERE event_id = ? AND consumer_id = ?')
        .get(eventId, consumerId) as { decision: InboundDecision } | undefined;
      if (prior) {
        if (prior.decision !== params.decision) throw new Error('event already acknowledged with another decision');
        return { sequence: event.sequence, decision: prior.decision, duplicate: true };
      }
      const cursor = this.db.prepare('SELECT sequence FROM event_consumers WHERE workspace_path = ? AND consumer_id = ?')
        .get(this.workspace, consumerId) as { sequence: number } | undefined;
      const next = this.db.prepare('SELECT MIN(sequence) AS sequence FROM event_outbox WHERE workspace_path = ? AND sequence > ?')
        .get(this.workspace, cursor?.sequence ?? 0) as { sequence: number | null };
      if (next.sequence !== event.sequence) throw new Error(`event acknowledgement must be ordered; next sequence is ${next.sequence ?? 'none'}`);
      const stamp = now();
      this.db.prepare('INSERT INTO event_acknowledgements(event_id, consumer_id, decision, decided_at) VALUES (?, ?, ?, ?)')
        .run(eventId, consumerId, params.decision, stamp);
      this.db.prepare(`INSERT INTO event_consumers(workspace_path, consumer_id, sequence, updated_at) VALUES (?, ?, ?, ?)
        ON CONFLICT(workspace_path, consumer_id) DO UPDATE SET sequence = excluded.sequence, updated_at = excluded.updated_at`)
        .run(this.workspace, consumerId, event.sequence, stamp);
      return { sequence: event.sequence, decision: params.decision, duplicate: false };
    });
  }

  getConsumerCursor(consumerId: string): number {
    return (this.db.prepare('SELECT sequence FROM event_consumers WHERE workspace_path = ? AND consumer_id = ?')
      .get(this.workspace, required(consumerId, 'consumer-id')) as { sequence: number } | undefined)?.sequence ?? 0;
  }

  pruneEvents(params: { throughSequence: number; dryRun?: boolean }): { matched: number; deleted: number; slowestCursor: number } {
    if (!Number.isInteger(params.throughSequence) || params.throughSequence < 0) throw new Error('throughSequence must be a non-negative integer');
    const slowest = this.db.prepare('SELECT MIN(sequence) AS sequence FROM event_consumers WHERE workspace_path = ?')
      .get(this.workspace) as { sequence: number | null };
    const slowestCursor = slowest.sequence ?? 0;
    if (params.throughSequence > slowestCursor) throw new Error(`cannot prune beyond slowest consumer cursor ${slowestCursor}`);
    const matched = (this.db.prepare('SELECT COUNT(*) AS count FROM event_outbox WHERE workspace_path = ? AND sequence <= ?')
      .get(this.workspace, params.throughSequence) as { count: number }).count;
    const dryRun = params.dryRun !== false;
    const deleted = dryRun ? 0 : (this.db.prepare('DELETE FROM event_outbox WHERE workspace_path = ? AND sequence <= ?')
      .run(this.workspace, params.throughSequence) as { changes: number }).changes;
    return { matched, deleted, slowestCursor };
  }

  createInteraction(input: InteractionRequestV1): StoredInteractionV1 {
    const request = parseInteractionRequestV1(input);
    if (request.workspace !== this.workspace) throw new Error('interaction workspace does not match the opened Awareness store');
    this.db.exec('BEGIN');
    try {
      this.db.prepare(`INSERT INTO pending_interactions(
        interaction_id, workspace_path, session_id, correlation_id, kind, request_json, status, created_at, expires_at
      ) VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?)`)
        .run(request.interactionId, request.workspace, request.sessionId, request.correlationId, request.kind, JSON.stringify(request), request.createdAt, request.expiresAt ?? null);
      this.insertOutboxEvent({
        version: 1,
        eventId: `evt_${request.interactionId}_requested`,
        workspace: request.workspace,
        sessionId: request.sessionId,
        correlationId: request.correlationId,
        type: 'question.requested',
        actor: { kind: 'system', id: 'octocode-harness' },
        provenance: { source: 'harness', trust: 'authority' },
        aggregate: { kind: 'interaction', id: request.interactionId },
        createdAt: request.createdAt,
        payload: request,
      });
      this.db.exec('COMMIT');
      return { request, status: 'pending' };
    } catch (error) {
      try { this.db.exec('ROLLBACK'); } catch { /* best effort */ }
      throw error;
    }
  }

  getInteraction(interactionId: string): StoredInteractionV1 {
    const row = this.db.prepare('SELECT * FROM pending_interactions WHERE workspace_path = ? AND interaction_id = ?')
      .get(this.workspace, required(interactionId, 'interaction-id')) as {
        request_json: string; answer_json: string | null; status: InteractionRequestV1['status']; resolved_at: string | null;
      } | undefined;
    if (!row) throw new Error(`unknown interaction: ${interactionId}`);
    return {
      request: JSON.parse(row.request_json) as InteractionRequestV1,
      status: row.status,
      ...(row.answer_json ? { answer: JSON.parse(row.answer_json) as InteractionAnswerV1 } : {}),
      ...(row.resolved_at ? { resolvedAt: row.resolved_at } : {}),
    };
  }

  listPendingInteractions(params: { sessionId?: string; limit?: number } = {}): StoredInteractionV1[] {
    const clauses = ["workspace_path = ?", "status = 'pending'", '(expires_at IS NULL OR expires_at > ?)'];
    const values: Array<string | number> = [this.workspace, now()];
    if (params.sessionId?.trim()) { clauses.push('session_id = ?'); values.push(params.sessionId.trim()); }
    const limit = Math.min(Math.max(params.limit ?? 100, 1), 500);
    const rows = this.db.prepare(`SELECT request_json, answer_json, status, resolved_at FROM pending_interactions WHERE ${clauses.join(' AND ')} ORDER BY created_at ASC LIMIT ?`)
      .all(...values, limit) as Array<{ request_json: string; answer_json: string | null; status: InteractionRequestV1['status']; resolved_at: string | null }>;
    return rows.map((row) => ({
      request: JSON.parse(row.request_json) as InteractionRequestV1,
      status: row.status,
      ...(row.answer_json ? { answer: JSON.parse(row.answer_json) as InteractionAnswerV1 } : {}),
      ...(row.resolved_at ? { resolvedAt: row.resolved_at } : {}),
    }));
  }

  answerInteraction(input: InteractionAnswerV1): StoredInteractionV1 {
    const answer = parseInteractionAnswerV1(input);
    const stored = this.getInteraction(answer.interactionId);
    if (stored.status !== 'pending') throw new Error(`interaction is ${stored.status}`);
    if (stored.request.sessionId !== answer.sessionId) throw new Error('interaction answer session mismatch');
    if (stored.request.correlationId !== answer.correlationId) throw new Error('interaction answer correlation mismatch');
    if (stored.request.expiresAt && Date.parse(stored.request.expiresAt) <= Date.parse(answer.createdAt)) {
      this.db.prepare("UPDATE pending_interactions SET status = 'expired', resolved_at = ? WHERE interaction_id = ?")
        .run(answer.createdAt, answer.interactionId);
      throw new Error('interaction expired');
    }
    const knownOptions = new Map(stored.request.options.map((option) => [option.id, option]));
    for (const optionId of answer.optionIds ?? []) {
      const option = knownOptions.get(optionId);
      if (!option) throw new Error(`unknown interaction option: ${optionId}`);
      if (option.disabledReason) throw new Error(`interaction option is disabled: ${optionId}`);
    }
    const status = answer.cancelled ? 'cancelled' : 'answered';
    this.db.exec('BEGIN');
    try {
      this.db.prepare('UPDATE pending_interactions SET answer_json = ?, status = ?, resolved_at = ? WHERE interaction_id = ? AND status = ?')
        .run(JSON.stringify(answer), status, answer.createdAt, answer.interactionId, 'pending');
      this.insertOutboxEvent({
        version: 1,
        eventId: `evt_${answer.interactionId}_${status}`,
        workspace: this.workspace,
        sessionId: answer.sessionId,
        correlationId: answer.correlationId,
        type: answer.cancelled ? 'question.cancelled' : 'question.answered',
        actor: answer.actor,
        provenance: answer.provenance,
        aggregate: { kind: 'interaction', id: answer.interactionId },
        createdAt: answer.createdAt,
        payload: answer,
      });
      this.db.exec('COMMIT');
    } catch (error) {
      try { this.db.exec('ROLLBACK'); } catch { /* best effort */ }
      throw error;
    }
    return this.getInteraction(answer.interactionId);
  }

  createAuthorizationReceipt(input: AuthorizationReceiptV1): AuthorizationReceiptV1 {
    const receipt = parseAuthorizationReceiptV1(input);
    if (receipt.workspace !== this.workspace) throw new Error('authorization workspace does not match the opened Awareness store');
    const interaction = this.getInteraction(receipt.interactionId);
    if (interaction.status !== 'answered' || interaction.request.kind !== 'authorization') throw new Error('authorization requires an answered authorization interaction');
    if (interaction.request.sessionId !== receipt.sessionId) throw new Error('authorization session mismatch');
    this.db.exec('BEGIN');
    try {
      this.db.prepare(`INSERT INTO authorization_receipts(
        receipt_id, interaction_id, workspace_path, session_id, plan_id, revision, scope_json,
        actor_json, provenance_json, created_at, expires_at, consumed_at
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
        .run(receipt.receiptId, receipt.interactionId, receipt.workspace, receipt.sessionId, receipt.planId, receipt.revision,
          JSON.stringify(receipt.scope), JSON.stringify(receipt.actor), JSON.stringify(receipt.provenance), receipt.createdAt,
          receipt.expiresAt ?? null, receipt.consumedAt ?? null);
      this.insertOutboxEvent({
        version: 1,
        eventId: `evt_${receipt.receiptId}_created`,
        workspace: receipt.workspace,
        sessionId: receipt.sessionId,
        correlationId: receipt.interactionId,
        type: 'authorization.created',
        actor: receipt.actor,
        provenance: receipt.provenance,
        aggregate: { kind: 'authorization', id: receipt.receiptId, revision: receipt.revision },
        createdAt: receipt.createdAt,
        payload: { receiptId: receipt.receiptId, planId: receipt.planId, revision: receipt.revision, scope: receipt.scope },
      });
      this.db.exec('COMMIT');
      return receipt;
    } catch (error) {
      try { this.db.exec('ROLLBACK'); } catch { /* best effort */ }
      throw error;
    }
  }

  consumeAuthorizationReceipt(params: { receiptId: string; planId: string; revision: string; scope: string; consumedAt?: string }): AuthorizationReceiptV1 {
    const receiptId = required(params.receiptId, 'receipt-id');
    const consumedAt = params.consumedAt ?? now();
    this.db.exec('BEGIN IMMEDIATE');
    try {
      const row = this.db.prepare('SELECT * FROM authorization_receipts WHERE workspace_path = ? AND receipt_id = ?')
        .get(this.workspace, receiptId) as {
          receipt_id: string; interaction_id: string; workspace_path: string; session_id: string; plan_id: string; revision: string;
          scope_json: string; actor_json: string; provenance_json: string; created_at: string; expires_at: string | null; consumed_at: string | null;
        } | undefined;
      if (!row) throw new Error('authorization receipt not found');
      const receipt = parseAuthorizationReceiptV1({
        version: 1, receiptId: row.receipt_id, interactionId: row.interaction_id, workspace: row.workspace_path,
        sessionId: row.session_id, planId: row.plan_id, revision: row.revision, scope: JSON.parse(row.scope_json),
        actor: JSON.parse(row.actor_json), provenance: JSON.parse(row.provenance_json), createdAt: row.created_at,
        ...(row.expires_at ? { expiresAt: row.expires_at } : {}), ...(row.consumed_at ? { consumedAt: row.consumed_at } : {}),
      });
      if (receipt.planId !== params.planId || receipt.revision !== params.revision) throw new Error('authorization revision mismatch');
      if (!receipt.scope.includes(params.scope)) throw new Error('authorization scope mismatch');
      if (receipt.expiresAt && Date.parse(receipt.expiresAt) <= Date.parse(consumedAt)) throw new Error('authorization receipt expired');
      if (receipt.consumedAt) throw new Error('authorization receipt already consumed');
      const result = this.db.prepare(`UPDATE authorization_receipts SET consumed_at = ?
        WHERE workspace_path = ? AND receipt_id = ? AND consumed_at IS NULL`)
        .run(consumedAt, this.workspace, receipt.receiptId) as { changes: number };
      if (result.changes !== 1) throw new Error('authorization receipt already consumed');
      this.insertOutboxEvent({
        version: 1,
        eventId: `evt_${receipt.receiptId}_consumed`,
        workspace: receipt.workspace,
        sessionId: receipt.sessionId,
        correlationId: receipt.interactionId,
        type: 'authorization.consumed',
        actor: receipt.actor,
        provenance: receipt.provenance,
        aggregate: { kind: 'authorization', id: receipt.receiptId, revision: receipt.revision },
        createdAt: consumedAt,
        payload: { receiptId: receipt.receiptId, planId: receipt.planId, revision: receipt.revision, scope: params.scope },
      });
      this.db.exec('COMMIT');
      return { ...receipt, consumedAt };
    } catch (error) {
      try { this.db.exec('ROLLBACK'); } catch { /* best effort */ }
      throw error;
    }
  }

  recordCapabilityReceipt(input: CapabilityDecisionReceiptV1): CapabilityDecisionReceiptV1 {
    if (input.version !== 1 || !input.receiptId || !input.action || !input.resource || !input.createdAt) throw new Error('capability receipt is incomplete');
    const effective = effectiveCapabilityDecision(input.guards);
    if (effective !== input.effectiveDecision) throw new Error(`capability receipt decision must be ${effective}`);
    this.db.prepare('INSERT INTO capability_receipts(receipt_id, workspace_path, receipt_json, created_at) VALUES (?, ?, ?, ?) ON CONFLICT(receipt_id) DO NOTHING')
      .run(input.receiptId, this.workspace, JSON.stringify(input), input.createdAt);
    return input;
  }

  createHarnessEvent<T>(params: { type: string; aggregateKind: string; aggregateId: string; aggregateRevision?: string; payload: T; sessionId?: string; correlationId?: string }): AgentEventEnvelopeV1<T> {
    return {
      version: 1,
      eventId: id('evt'),
      workspace: this.workspace,
      ...(params.sessionId ? { sessionId: params.sessionId } : {}),
      ...(params.correlationId ? { correlationId: params.correlationId } : {}),
      type: params.type,
      actor: { kind: 'system', id: 'octocode-harness' },
      provenance: { source: 'harness', trust: 'authority' },
      aggregate: { kind: params.aggregateKind, id: params.aggregateId, ...(params.aggregateRevision ? { revision: params.aggregateRevision } : {}) },
      createdAt: now(),
      payload: params.payload,
    };
  }
}
