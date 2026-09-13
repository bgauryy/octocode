/**
 * Worker output normalization: parse structured worker handbacks, extract confidence,
 * compute recovery risk, and extract progress summaries from worker output text.
 */
import type { NormalizedWorkerConfidence, NormalizedWorkerResult, WorkerRecoveryRisk } from './types.js';

const DELTA_PREFIX = /^\s*\[(STATUS|ACTION|FINDING|METRIC|PLAN|BLOCKED|DONE|EVIDENCE)\]/i;
const MAX_DELTA_SUMMARY_CHARS = 120;

function normalizeConfidence(value: string | undefined): NormalizedWorkerConfidence {
  const lower = String(value ?? '').trim().toLowerCase();
  if (lower === 'confirmed') return 'confirmed';
  if (lower === 'likely') return 'likely';
  return 'uncertain';
}

export function normalizeWorkerOutput(output: string): NormalizedWorkerResult {
  const rawPrefixes: Record<string, string[]> = {};
  const lines = output.split(/\r?\n/);
  for (const line of lines) {
    const match = line.match(/^\s*\[([A-Z][A-Z0-9_-]*)\]\s*(.*)$/);
    if (!match) continue;
    const prefix = match[1]!;
    (rawPrefixes[prefix] ??= []).push(match[2]!.trim());
  }

  const last = (prefix: string): string | undefined => rawPrefixes[prefix]?.at(-1);
  const evidence = rawPrefixes['EVIDENCE'] ?? [];
  const blocked = last('BLOCKED');
  const done = last('DONE');
  const failed = last('FAILED') ?? last('ERROR');
  // Cover every salient prefix the subagent SYSTEM_PROMPTs solicit — a worker
  // whose best output is [RISK]/[IMPACT]/[GAP] must still surface it in the
  // normalized handback instead of silently dropping to result:undefined.
  const result =
    last('RESULT')
    ?? last('VERDICT')
    ?? last('FINDING')
    ?? last('ROOT')
    ?? last('FIX')
    ?? last('PLAN')
    ?? last('ACTION')
    ?? last('IMPACT')
    ?? last('RISK')
    ?? last('GAP')
    ?? last('GAPS')
    ?? last('ASSUMPTION')
    ?? last('QUERY')
    ?? last('METRIC')
    ?? failed
    ?? blocked
    ?? done
    ?? undefined;
  const verification = last('VERIFICATION') ?? last('VERIFY') ?? undefined;
  const artifact = last('ARTIFACT') ?? last('HANDOFF') ?? undefined;
  const fallback = output.trim();

  return {
    status: failed ? 'failed' : blocked ? 'blocked' : done ? 'done' : 'unknown',
    result: result || (Object.keys(rawPrefixes).length === 0 && fallback ? fallback : undefined),
    evidence,
    verification,
    confidence: normalizeConfidence(last('CONFIDENCE')),
    next: last('NEXT') || blocked || done || undefined,
    artifact,
    rawPrefixes,
  };
}

export function evaluateWorkerRecoveryRisk(output: string): WorkerRecoveryRisk {
  const normalized = normalizeWorkerOutput(output);
  const statusOrActionCount = (normalized.rawPrefixes['STATUS']?.length ?? 0)
    + (normalized.rawPrefixes['ACTION']?.length ?? 0)
    + (normalized.rawPrefixes['FIX']?.length ?? 0);
  const evidenceCount = normalized.evidence.length;
  const hasVerification = Boolean(normalized.verification);
  const warnings: string[] = [];

  if (statusOrActionCount >= 4 && evidenceCount === 0 && !hasVerification) {
    warnings.push(
      `Possible recovery loop: ${statusOrActionCount} status/action updates without evidence or verification; re-diagnose before continuing.`,
    );
  }

  if (normalized.status === 'done' && evidenceCount === 0 && !hasVerification) {
    warnings.push('Worker claims done without evidence or verification; parent must independently verify acceptance.');
  }

  return { warnings, statusOrActionCount, evidenceCount, hasVerification };
}

/** Rolling progress note: latest structured worker line, else the last non-empty line. */
export function extractDeltaSummary(text: string): string | undefined {
  const lines = text.split('\n').map((l) => l.trim()).filter(Boolean);
  if (lines.length === 0) return undefined;
  const structured = [...lines].reverse().find((l) => DELTA_PREFIX.test(l));
  const chosen = (structured ?? lines[lines.length - 1]!).replace(/\s+/g, ' ');
  return chosen.length > MAX_DELTA_SUMMARY_CHARS ? `${chosen.slice(0, MAX_DELTA_SUMMARY_CHARS - 1)}\u2026` : chosen;
}

export function extractTextFromMessage(message: unknown): string {
  const content = (message as { content?: unknown })?.content;
  if (!Array.isArray(content)) return '';
  return content
    .map((part) => ((part as { type?: string; text?: string }).type === 'text' ? (part as { text?: string }).text ?? '' : ''))
    .filter(Boolean)
    .join('\n');
}

export function isAssistantOutputMessage(message: unknown): boolean {
  if (!message || typeof message !== 'object') return false;
  return (message as { role?: unknown }).role === 'assistant';
}
