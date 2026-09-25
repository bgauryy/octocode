import { readSchemaObjects, assertSchemaObjects, type SchemaObject } from '../runtime/agent-store-schema.js';
import { join, dirname } from 'node:path';
import { agentDbPath } from '../contracts/paths.js';
import { DatabaseSync, withSqliteBusyRetry, SQLITE_BUSY_DEADLINE_MS } from '../runtime/sqlite.js';
import { preparePrivateSqlitePath, hardenSqliteFiles } from '../runtime/permissions.js';
import { journalModeForSqliteVersion } from '../runtime/sqlite-version.js';
import { normalizeWorkspacePath } from '../runtime/workspace.js';
import { parseInteractionRequestV1, parseInteractionAnswerV1, parseAuthorizationReceiptV1, type AgentEventEnvelopeV1, type InteractionRequestV1, type InteractionAnswerV1, type AuthorizationReceiptV1 } from '../runtime/continuity-contracts.js';

export interface OutboxEventV1<T = unknown> extends AgentEventEnvelopeV1<T> { sequence: number }
export interface StoredInteractionV1 { request: InteractionRequestV1; status: InteractionRequestV1['status']; answer?: InteractionAnswerV1; resolvedAt?: string }
const APPLICATION_ID = 0x4f435449;
const SCHEMA_SQL = `
          CREATE TABLE IF NOT EXISTS interactions (id TEXT PRIMARY KEY, workspace TEXT NOT NULL, session TEXT NOT NULL, value TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS interactions_workspace ON interactions(workspace, session);
          CREATE TABLE IF NOT EXISTS receipts (id TEXT PRIMARY KEY, workspace TEXT NOT NULL, value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS events (sequence INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT UNIQUE NOT NULL, workspace TEXT NOT NULL, value TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS events_workspace ON events(workspace, sequence);
          CREATE TABLE IF NOT EXISTS cursors (workspace TEXT NOT NULL, consumer TEXT NOT NULL, sequence INTEGER NOT NULL, PRIMARY KEY(workspace, consumer));
          CREATE TABLE IF NOT EXISTS acknowledgements (workspace TEXT NOT NULL, consumer TEXT NOT NULL, event TEXT NOT NULL, decision TEXT NOT NULL, PRIMARY KEY(workspace, consumer, event));
`;
let canonicalSchema: SchemaObject[] | undefined;
function expectedSchema(): SchemaObject[] {
  if (canonicalSchema) return canonicalSchema;
  const db = new DatabaseSync(':memory:');
  try { db.exec(SCHEMA_SQL); canonicalSchema = readSchemaObjects(db); return canonicalSchema; } finally { db.close(); }
}


/** Pi-owned approval storage. It never opens or migrates former coordination databases. */
export class InteractionStore {
  private readonly db: DatabaseSync;
  readonly workspace: string;
  constructor(workspace: string, dbPath = join(dirname(agentDbPath()), 'interactions.sqlite3')) {
    this.workspace = normalizeWorkspacePath(workspace, workspace)!;
    preparePrivateSqlitePath(dbPath);
    this.db = new DatabaseSync(dbPath);
    try {
      this.db.exec(`PRAGMA busy_timeout = ${SQLITE_BUSY_DEADLINE_MS}`);
      this.transaction(() => {
        const identity = (this.db.prepare('PRAGMA application_id').get() as { application_id: number }).application_id;
        const tables = this.db.prepare("SELECT name FROM sqlite_schema WHERE name NOT GLOB 'sqlite_*'").all();
        if (identity !== APPLICATION_ID && (identity !== 0 || tables.length)) throw new Error('refusing foreign interaction database');
        const version = (this.db.prepare('PRAGMA user_version').get() as { user_version: number }).user_version;
        if (identity === 0 && tables.length === 0 && version === 0) {
          this.db.exec(SCHEMA_SQL);
          this.db.exec(`PRAGMA application_id = ${APPLICATION_ID}; PRAGMA user_version = 1`);
        } else if (version !== 1) {
          throw new Error(`unsupported interaction database version: ${version}`);
        }
        assertSchemaObjects(readSchemaObjects(this.db), expectedSchema());
      });
      const version = (this.db.prepare('SELECT sqlite_version() AS version').get() as { version: string }).version;
      withSqliteBusyRetry(() => this.db.exec(`PRAGMA journal_mode = ${journalModeForSqliteVersion(version)}`));
      this.db.exec('PRAGMA synchronous = FULL');
      hardenSqliteFiles(dbPath);
    } catch (error) { this.db.close(); throw error; }
  }
  private transaction<T>(run: () => T): T {
    withSqliteBusyRetry(() => this.db.exec('BEGIN IMMEDIATE'));
    try { const result = run(); this.db.exec('COMMIT'); return result; }
    catch (error) { try { this.db.exec('ROLLBACK'); } catch { /* preserve original failure */ } throw error; }
  }
  private event(event: AgentEventEnvelopeV1): void {
    this.db.prepare('INSERT INTO events(id, workspace, value) VALUES (?, ?, ?)').run(event.eventId, this.workspace, JSON.stringify(event));
  }
  createInteraction(input: InteractionRequestV1): StoredInteractionV1 {
    const request = parseInteractionRequestV1(input);
    if (request.workspace !== this.workspace) throw new Error('interaction workspace mismatch');
    const value: StoredInteractionV1 = { request, status: 'pending' };
    this.db.prepare('INSERT INTO interactions(id, workspace, session, value) VALUES (?, ?, ?, ?)').run(request.interactionId, this.workspace, request.sessionId, JSON.stringify(value));
    return value;
  }
  getInteraction(id: string): StoredInteractionV1 {
    const row = this.db.prepare('SELECT value FROM interactions WHERE id = ? AND workspace = ?').get(id, this.workspace) as { value: string } | undefined;
    if (!row) throw new Error('interaction request was not found');
    return JSON.parse(row.value) as StoredInteractionV1;
  }
  listPendingInteractions({ sessionId, limit = 500 }: { sessionId?: string; limit?: number } = {}): StoredInteractionV1[] {
    const rows = this.db.prepare("SELECT value FROM interactions WHERE workspace = ? AND (? IS NULL OR session = ?) AND json_extract(value, '$.status') = 'pending' AND (json_extract(value, '$.request.expiresAt') IS NULL OR json_extract(value, '$.request.expiresAt') > ?) ORDER BY rowid LIMIT ?")
      .all(this.workspace, sessionId ?? null, sessionId ?? null, new Date().toISOString(), Math.min(500, Math.max(1, limit))) as Array<{ value: string }>;
    return rows.map(row => JSON.parse(row.value) as StoredInteractionV1);
  }
  answerInteraction(input: InteractionAnswerV1): StoredInteractionV1 {
    const answer = parseInteractionAnswerV1(input);
    return this.transaction(() => {
      const stored = this.getInteraction(answer.interactionId);
      if (stored.status !== 'pending') throw new Error(`interaction is ${stored.status}`);
      if (stored.request.sessionId !== answer.sessionId || stored.request.correlationId !== answer.correlationId) throw new Error('interaction answer session or correlation mismatch');
      if (stored.request.expiresAt && Date.parse(stored.request.expiresAt) <= Date.now()) throw new Error('interaction expired');
      for (const id of answer.optionIds ?? []) {
        const option = stored.request.options.find(option => option.id === id);
        if (!option || option.disabledReason) throw new Error('interaction option is unknown or disabled');
      }
      const status = answer.cancelled ? 'cancelled' : 'answered';
      const result: StoredInteractionV1 = { ...stored, status, answer, resolvedAt: answer.createdAt };
      this.db.prepare('UPDATE interactions SET value = ? WHERE id = ? AND workspace = ?').run(JSON.stringify(result), answer.interactionId, this.workspace);
      this.event({ version: 1, eventId: `interaction:${answer.interactionId}:${status}`, workspace: this.workspace, sessionId: answer.sessionId, correlationId: answer.correlationId, type: answer.cancelled ? 'question.cancelled' : 'question.answered', actor: answer.actor, provenance: answer.provenance, aggregate: { kind: 'interaction', id: answer.interactionId }, createdAt: answer.createdAt, payload: answer });
      return result;
    });
  }
  createAuthorizationReceipt(input: AuthorizationReceiptV1): AuthorizationReceiptV1 {
    const receipt = parseAuthorizationReceiptV1(input);
    return this.transaction(() => {
      if (receipt.workspace !== this.workspace) throw new Error('authorization workspace mismatch');
      const interaction = this.getInteraction(receipt.interactionId);
      if (interaction.status !== 'answered' || interaction.request.kind !== 'authorization' || interaction.request.sessionId !== receipt.sessionId) throw new Error('authorization requires an answered authorization interaction in this session');
      const issued = this.db.prepare("SELECT value FROM receipts WHERE workspace = ? AND json_extract(value, '$.interactionId') = ?").all(this.workspace, receipt.interactionId) as Array<{ value: string }>;
      if (issued.some(row => parseAuthorizationReceiptV1(JSON.parse(row.value)).scope.some(scope => receipt.scope.includes(scope)))) {
        throw new Error('authorization scope already issued for this interaction');
      }
      this.db.prepare('INSERT INTO receipts(id, workspace, value) VALUES (?, ?, ?)').run(receipt.receiptId, this.workspace, JSON.stringify(receipt));
      return receipt;
    });
  }
  consumeAuthorizationReceipt(params: { receiptId: string; planId: string; revision: string; scope: string }): AuthorizationReceiptV1 {
    return this.transaction(() => {
      const row = this.db.prepare('SELECT value FROM receipts WHERE id = ? AND workspace = ?').get(params.receiptId, this.workspace) as { value: string } | undefined;
      if (!row) throw new Error('authorization receipt not found');
      const receipt = parseAuthorizationReceiptV1(JSON.parse(row.value));
      if (receipt.planId !== params.planId || receipt.revision !== params.revision || !receipt.scope.includes(params.scope)) throw new Error('authorization revision or scope mismatch');
      if (receipt.consumedAt) throw new Error('authorization receipt already consumed');
      if (receipt.expiresAt && Date.parse(receipt.expiresAt) <= Date.now()) throw new Error('authorization receipt expired');
      const consumed = { ...receipt, consumedAt: new Date().toISOString() };
      this.db.prepare('UPDATE receipts SET value = ? WHERE id = ? AND workspace = ?').run(JSON.stringify(consumed), params.receiptId, this.workspace);
      return consumed;
    });
  }
  getConsumerCursor(consumerId: string): number { return (this.db.prepare('SELECT sequence FROM cursors WHERE workspace = ? AND consumer = ?').get(this.workspace, consumerId) as { sequence: number } | undefined)?.sequence ?? 0; }
  listEvents({ consumerId, limit = 100 }: { consumerId: string; limit?: number }): OutboxEventV1[] {
    const rows = this.db.prepare('SELECT sequence, value FROM events WHERE workspace = ? AND sequence > COALESCE((SELECT sequence FROM cursors WHERE workspace = ? AND consumer = ?), 0) ORDER BY sequence LIMIT ?').all(this.workspace, this.workspace, consumerId, Math.min(1000, Math.max(1, limit))) as Array<{ sequence: number; value: string }>;
    return rows.map(row => ({ ...JSON.parse(row.value), sequence: row.sequence }) as OutboxEventV1);
  }
  acknowledgeEvent({ consumerId, eventId, decision }: { consumerId: string; eventId: string; decision: 'accept' | 'hold' | 'refuse' }): void {
    this.transaction(() => {
      const prior = this.db.prepare('SELECT decision FROM acknowledgements WHERE workspace = ? AND consumer = ? AND event = ?').get(this.workspace, consumerId, eventId) as { decision: string } | undefined;
      if (prior) { if (prior.decision !== decision) throw new Error('event already acknowledged with another decision'); return; }
      const event = this.listEvents({ consumerId, limit: 1 })[0];
      if (!event || event.eventId !== eventId) throw new Error('event acknowledgement must be ordered');
      this.db.prepare('INSERT INTO acknowledgements VALUES (?, ?, ?, ?)').run(this.workspace, consumerId, eventId, decision);
      this.db.prepare('INSERT INTO cursors VALUES (?, ?, ?) ON CONFLICT(workspace, consumer) DO UPDATE SET sequence = excluded.sequence').run(this.workspace, consumerId, event.sequence);
    });
  }
  close(): void { this.db.close(); }
}
export function openInteractionStore(workspace: string): InteractionStore { return new InteractionStore(workspace); }
