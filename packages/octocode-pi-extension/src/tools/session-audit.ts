import fs from 'node:fs';
import {
  createSessionArtifactContext,
  type SessionArtifactContext,
  type SessionIdentityInput,
} from './session-artifacts.js';

export const SESSION_AUDIT_RELATIVE_PATH = 'audit.md';
export const SESSION_AUDIT_MAX_EVENTS = 200;
const MAX_EVENT_CHARS = 96;
const MAX_DETAIL_CHARS = 500;

const AUDIT_HEADER = `# Session audit

System-written lifecycle history. Agents may read this file but must never edit it.

| Time | Event | Detail |
| --- | --- | --- |
`;

export interface SessionAuditEntry {
  event: string;
  detail?: unknown;
  at?: string;
}

function inline(value: string, maxChars: number): string {
  return value
    .replace(/\r?\n|\r/g, ' ')
    .replace(/\|/g, '\\|')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, maxChars);
}

function detailText(value: unknown): string {
  if (value === undefined) return '';
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function auditRows(text: string): string[] {
  return text.split('\n').filter((line) => /^\| \d{4}-\d{2}-\d{2}T/.test(line));
}

export function initializeSessionAudit(ctx: SessionArtifactContext): string {
  const auditPath = ctx.resolve(SESSION_AUDIT_RELATIVE_PATH);
  if (!fs.existsSync(auditPath)) ctx.writeText(SESSION_AUDIT_RELATIVE_PATH, AUDIT_HEADER);
  const registered = ctx.inspect()?.producers.audit?.paths.includes(SESSION_AUDIT_RELATIVE_PATH) === true;
  if (!registered) ctx.registerProducer('audit', SESSION_AUDIT_RELATIVE_PATH);
  return auditPath;
}

/** Best-effort by contract: observability must never interrupt the owning lifecycle. */
export function appendSessionAuditEntry(ctx: SessionArtifactContext, entry: SessionAuditEntry): boolean {
  try {
    const auditPath = initializeSessionAudit(ctx);
    const current = fs.readFileSync(auditPath, 'utf8');
    const at = inline(entry.at ?? new Date().toISOString(), 40);
    const event = inline(entry.event, MAX_EVENT_CHARS) || 'unknown';
    const detail = inline(detailText(entry.detail), MAX_DETAIL_CHARS);
    const row = `| ${at} | ${event} | ${detail} |`;
    const rows = [...auditRows(current), row].slice(-SESSION_AUDIT_MAX_EVENTS);
    ctx.writeText(SESSION_AUDIT_RELATIVE_PATH, `${AUDIT_HEADER}${rows.join('\n')}${rows.length ? '\n' : ''}`);
    return true;
  } catch {
    return false;
  }
}

export function appendSessionAuditForContext(
  input: SessionIdentityInput | undefined,
  entry: SessionAuditEntry,
): boolean {
  if (!input) return false;
  try {
    return appendSessionAuditEntry(createSessionArtifactContext(input), entry);
  } catch {
    return false;
  }
}
