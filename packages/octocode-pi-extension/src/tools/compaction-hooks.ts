import type { PiContext, PiInstance, SessionBeforeCompactEvent, SessionCompactEvent, NotifyFn } from '../types.js';
import { activePlanScope, getPlan, getPlanCoordination, getPlanReviewState } from './active-plan.js';
import { getCurrentPlanReadModel, renderPlanContext } from './plan-read-model.js';
import { emitCompactionCheckpoint, type CompactionCheckpointDetails } from './custom-messages.js';
import { writeCompactionArtifact } from './compaction-artifacts.js';
import { clearAllReadStates } from './file-state.js';
import { contentDigest, type ContextSegmentV1 } from '@octocodeai/octocode-awareness';
import { createSessionArtifactContext, writeRehydrationLedger } from './session-artifacts.js';
import { listPendingInteractionIds } from './interaction-broker.js';
import { clearPendingRehydration, runAndRecordRehydration } from './rehydration-orchestrator.js';
import { captureCurrentContextSources } from './context-source-registry.js';
import { redactCompactionText } from './compaction-redaction.js';
import { openPersistentAwareness } from './storage-policy.js';
import { appendSessionAuditEntry } from './session-audit.js';

export interface CompactionRehydrationCapture {
  segments: ContextSegmentV1[];
  contents: Record<string, string>;
}

let rehydrationSegmentsProvider: ((ctx: PiContext) => CompactionRehydrationCapture) | undefined;
export function setCompactionRehydrationSegmentsProvider(provider?: (ctx: PiContext) => CompactionRehydrationCapture): void {
  rehydrationSegmentsProvider = provider;
}

export function mergeCompactionRehydrationCaptures(
  fixed: CompactionRehydrationCapture,
  dynamic: CompactionRehydrationCapture,
): CompactionRehydrationCapture {
  const segments = new Map(fixed.segments.map((segment) => [segment.id, segment]));
  const contents = { ...fixed.contents };
  for (const segment of dynamic.segments) {
    if (segments.has(segment.id)) continue;
    segments.set(segment.id, segment);
    const content = dynamic.contents[segment.id];
    if (content !== undefined) contents[segment.id] = content;
  }
  return { segments: [...segments.values()], contents };
}

const SPLIT_TURN_COMPACTION_HEADER = '**Turn Context (split turn):**';
const CUSTOM_COMPACTION_SUMMARY_LIMIT = 12_000;
const CUSTOM_COMPACTION_SECTION_LIMIT = 4_000;

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function asString(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim().length > 0 ? value : undefined;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function asStringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const strings = value.filter((item): item is string => typeof item === 'string' && item.length > 0);
  return strings.length > 0 ? strings : undefined;
}

function truncateText(text: string, limit = CUSTOM_COMPACTION_SECTION_LIMIT): string {
  const redacted = redactCompactionText(text);
  if (redacted.length <= limit) return redacted;
  return `${redacted.slice(0, limit)}\n…[truncated ${redacted.length - limit} chars]`;
}

function extractTextContent(content: unknown): string {
  if (typeof content === 'string') return content;
  if (!Array.isArray(content)) return '';
  return content
    .map((part) => {
      if (!isRecord(part)) return '';
      if (part.type === 'text' && typeof part.text === 'string') return part.text;
      // Model reasoning is not user-visible output and must never be copied into
      // durable compaction artifacts or the next-turn summary.
      if (part.type === 'thinking') return '[model reasoning omitted]';
      if (part.type === 'toolCall') {
        const name = typeof part.name === 'string' ? part.name : 'tool';
        return `[tool call] ${name} (arguments omitted)`;
      }
      if (part.type === 'image') return '[image omitted]';
      return '';
    })
    .filter(Boolean)
    .join('\n');
}

function describeMessage(message: unknown): string {
  if (!isRecord(message)) return '';
  const role = typeof message.role === 'string' ? message.role : 'message';
  const content = extractTextContent(message.content);
  if (content) return `### ${role}\n${truncateText(content)}`;
  if (role === 'toolResult') return `### ${role}\n(metadata omitted)`;
  return `### ${role}\n${truncateText(JSON.stringify(message))}`;
}

function summarizeMessages(messages: unknown[], title: string, maxItems: number): string {
  if (messages.length === 0) return `## ${title}\n(none)`;
  const omitted = Math.max(0, messages.length - maxItems);
  const selected = messages.slice(-maxItems).map(describeMessage).filter(Boolean);
  return [
    `## ${title}`,
    omitted > 0 ? `Omitted ${omitted} older message(s); retained the most recent ${selected.length}.` : undefined,
    ...selected,
  ].filter(Boolean).join('\n\n');
}

function extractFileOps(preparation: Record<string, unknown>): { readFiles: string[]; modifiedFiles: string[] } {
  const fileOps = isRecord(preparation.fileOps) ? preparation.fileOps : {};
  const fromSetLike = (value: unknown): string[] => {
    if (Array.isArray(value)) return value.filter((item): item is string => typeof item === 'string');
    if (value instanceof Set) return [...value].filter((item): item is string => typeof item === 'string');
    if (isRecord(value)) return Object.keys(value);
    return [];
  };
  // Pi's FileOperations is {read, written, edited}: files created via the
  // write tool count as modified, and modified files are excluded from the
  // read list (mirrors Pi's own computeFileLists).
  const modifiedFiles = [
    ...new Set([...fromSetLike(fileOps.edited), ...fromSetLike(fileOps.written), ...fromSetLike(fileOps.modifiedFiles)]),
  ].sort();
  const modifiedSet = new Set(modifiedFiles);
  const readFiles = [...new Set([...fromSetLike(fileOps.read), ...fromSetLike(fileOps.readFiles)])]
    .filter((file) => !modifiedSet.has(file))
    .sort();
  return { readFiles, modifiedFiles };
}

function formatFileList(title: string, files: string[]): string {
  if (files.length === 0) return `## ${title}\n(none)`;
  return [`## ${title}`, ...files.slice(0, 80).map((file) => `- ${file}`), files.length > 80 ? `- …and ${files.length - 80} more` : undefined]
    .filter(Boolean)
    .join('\n');
}

function buildDeterministicCompaction(
  preparation: Record<string, unknown>,
  reason: string,
  customInstructions: unknown,
  continuationContext?: string,
): {
  summary: string;
  firstKeptEntryId: string;
  tokensBefore: number;
  details: { readFiles: string[]; modifiedFiles: string[]; fallback: string };
} | null {
  const firstKeptEntryId = asString(preparation.firstKeptEntryId);
  const tokensBefore = asNumber(preparation.tokensBefore);
  if (!firstKeptEntryId || tokensBefore === undefined) return null;

  const messagesToSummarize = asArray(preparation.messagesToSummarize);
  const turnPrefixMessages = asArray(preparation.turnPrefixMessages);
  const previousSummary = asString(preparation.previousSummary);
  const { readFiles, modifiedFiles } = extractFileOps(preparation);
  const focus = asString(customInstructions);

  // Keep recovery-critical structure at the front and budget each lossy history
  // section independently. A final whole-document slice alone can delete the
  // resume policy, authoritative plan/doc pointers, file list, or Pi's required
  // split-turn marker when tool history is large.
  const summary = [
    '## Octocode deterministic compaction checkpoint',
    `Reason: ${reason}`,
    `Tokens before compaction: ${tokensBefore}`,
    focus ? `Focus instructions: ${redactCompactionText(focus)}` : undefined,
    '## Resume instructions\nRe-orient from retained recent messages and the current authoritative sources below. If an active authorized plan remains, resume its next runnable step until the overall request meets acceptance, a real blocker or approval gate is reached, or the user asks to pause. Do not stop merely because one substep passes. If no active work remains, stop and wait for the user.',
    continuationContext?.trim()
      ? `## Active plan and authoritative references\n${truncateText(continuationContext, 2_500)}`
      : '## Active plan and authoritative references\n(none)',
    truncateText(formatFileList('Read files', readFiles), 1_200),
    truncateText(formatFileList('Modified files', modifiedFiles), 1_200),
    turnPrefixMessages.length > 0
      ? [
          '---',
          SPLIT_TURN_COMPACTION_HEADER,
          truncateText(summarizeMessages(turnPrefixMessages, 'Split-turn prefix checkpoint', 6), 2_500),
        ].join('\n\n')
      : undefined,
    previousSummary ? `## Previous summary\n${truncateText(previousSummary, 1_000)}` : undefined,
    truncateText(summarizeMessages(messagesToSummarize, 'Discarded history checkpoint', 4), 1_500),
  ].filter(Boolean).join('\n\n');

  return {
    summary: truncateText(summary, CUSTOM_COMPACTION_SUMMARY_LIMIT),
    firstKeptEntryId,
    tokensBefore,
    details: { readFiles, modifiedFiles, fallback: 'octocode-deterministic-compaction' },
  };
}

// ─── Compaction checkpoint card (one per compaction event) ───────────────────
//
// session_compact can be observed more than once for the same compaction
// (multiple registrations across reloads, replayed events); the card must be
// idempotent per compaction.
//
// Dedupe strategy (in priority order):
//   1. Stable string id  — entry.id is a server-assigned identifier stable
//      across retries; tracked in a resettable Set<string> so
//      resetCompactionCheckpointDedupe() can clear() on session boundaries.
//   2. Object identity   — Pi hands us the same compactionEntry object for
//      the same compaction when no id is present; tracked in a `let` WeakSet
//      so reset can reassign a fresh instance (WeakSet has no .clear()).
//   3. Fallback string   — non-object entries keyed by reason:String(entry).

const emittedCheckpointIds = new Set<string>();
let emittedCheckpointEntries = new WeakSet<object>();
let lastCheckpointFallbackKey: string | null = null;

function shouldEmitCheckpointCard(event: SessionCompactEvent): boolean {
  const entry = event.compactionEntry;
  if (entry !== null && entry !== undefined && typeof entry === 'object') {
    const rec = entry as Record<string, unknown>;
    const id = typeof rec.id === 'string' && rec.id.trim() ? rec.id : undefined;
    if (id !== undefined) {
      if (emittedCheckpointIds.has(id)) return false;
      emittedCheckpointIds.add(id);
      return true;
    }
    if (emittedCheckpointEntries.has(entry)) return false;
    emittedCheckpointEntries.add(entry);
    return true;
  }
  const key = `${event.reason}:${String(entry)}`;
  if (lastCheckpointFallbackKey === key) return false;
  lastCheckpointFallbackKey = key;
  return true;
}

function buildCheckpointDetails(event: SessionCompactEvent, ctx: PiContext): CompactionCheckpointDetails {
  const entry = isRecord(event.compactionEntry) ? event.compactionEntry : {};
  const entryDetails = isRecord(entry.details) ? entry.details : {};
  const tokensBefore = asNumber(entry.tokensBefore);
  const estimatedTokensAfter = asNumber(entry.estimatedTokensAfter);
  const summary = asString(entry.summary);
  const details: CompactionCheckpointDetails = {
    label: asString(entry.id) ?? `${event.reason} compaction`,
    reason: event.reason,
    fromExtension: event.fromExtension,
  };
  if (tokensBefore !== undefined) details.tokensBefore = tokensBefore;
  if (estimatedTokensAfter !== undefined) {
    details.estimatedTokensAfter = estimatedTokensAfter;
    if (tokensBefore !== undefined) {
      details.reclaimedTokens = Math.max(0, tokensBefore - estimatedTokensAfter);
      details.reclaimedPercent = tokensBefore > 0
        ? Math.round((details.reclaimedTokens / tokensBefore) * 100)
        : 0;
    }
  }
  const readFiles = asStringArray(entryDetails.readFiles);
  const modifiedFiles = asStringArray(entryDetails.modifiedFiles);
  if (readFiles) details.readFiles = readFiles;
  if (modifiedFiles) details.modifiedFiles = modifiedFiles;
  if (summary) details.summary = redactCompactionText(summary);
  const scope = activePlanScope(ctx);
  const plan = getPlan(scope);
  if (plan.length > 0) {
    // Capture the canonical persisted plan contract once. Both the model-facing
    // marker and durable latest.md serialize this exact versioned projection.
    details.continuation = {
      version: 1,
      plan: {
        review: getPlanReviewState(scope),
        coordination: getPlanCoordination(scope),
        steps: plan.map((step) => ({ ...step })),
      },
    };
  }
  return details;
}

export function resetCompactionCheckpointDedupe(): void {
  emittedCheckpointIds.clear();
  emittedCheckpointEntries = new WeakSet();
  lastCheckpointFallbackKey = null;
}

export function registerCompactionHooks(pi: PiInstance, notify: NotifyFn): void {
  if (!pi.on) return;

  pi.on('session_shutdown', async () => {
    // Replacement shutdown can deliberately provide a stale context proxy.
    // The extension owns one active session, so cleanup must not dereference it.
    // clearCurrentContextSources() is intentionally ABSENT here: the no-ctx
    // clear-all races with a concurrently starting session that has already
    // registered its sources.  Ctx-specific clearing is owned by
    // disposeSessionResources in the extension's session_shutdown handler.
    clearPendingRehydration();
  });

  pi.on('session_before_compact', async (event: SessionBeforeCompactEvent, ctx: PiContext) => {
    try {
      // `/compact` is an explicit user action. Never second-guess it from brittle
      // assistant-text heuristics; Pi owns cancellation and failure presentation.
      const preparation = isRecord(event.preparation) ? event.preparation : undefined;
      if (!preparation) return;
      const turnPrefixMessages = asArray(preparation.turnPrefixMessages);
      const isSplitTurn = preparation.isSplitTurn === true || turnPrefixMessages.length > 0;
      if (!isSplitTurn) return;
      // The deterministic checkpoint is an EMERGENCY path only: on overflow the
      // provider summarization call can itself overflow/fail, so a fast local
      // checkpoint beats losing the compaction entirely. Manual and threshold
      // split-turn compactions keep Pi's LLM summarizer — it produces a far
      // richer summary, and replacing it unconditionally was a silent quality
      // regression on the most common compaction shape.
      if (event.reason !== 'overflow') return;

      const planScope = activePlanScope(ctx);
      const continuationContext = renderPlanContext(getCurrentPlanReadModel(ctx, planScope));
      const compaction = buildDeterministicCompaction(
        preparation,
        event.reason,
        event.customInstructions,
        continuationContext,
      );
      if (!compaction) return;

      notify(
        ctx,
        'Using Octocode deterministic split-turn compaction checkpoint (overflow path — provider summarization could overflow too).',
        'warning',
      );
      return { compaction };
    } catch {
      // On unexpected error let Pi use its default LLM summarizer path rather
      // than surfacing an extension exception to the user.
      return undefined;
    }
  });

  pi.on('session_compact', async (event: SessionCompactEvent, ctx: PiContext) => {
    try {
      // Pi emits this event exactly once after it has appended the successful
      // compaction and rebuilt context. `willRetry` means Pi will retry the
      // interrupted agent turn (overflow recovery); it does not mean compaction
      // itself is pending or that another session_compact event will follow.
      // The transcript the read-states were recorded against is gone; the edit
      // tool's stale-read gate must demand a fresh read, not trust pre-compaction
      // knowledge the model no longer has.
      clearAllReadStates();
      // Completed compaction → branded checkpoint card in the transcript. The
      // dedupe guard makes this idempotent even if the hook observes the same
      // compaction event twice. Content is one terse line (it enters the LLM
      // context); rich data rides in details for the renderer only.
      if (shouldEmitCheckpointCard(event)) {
        const details = buildCheckpointDetails(event, ctx);
        const artifact = writeCompactionArtifact(details, ctx.sessionManager, ctx.cwd);
        if (artifact) {
          details.artifactPath = artifact.path;
          details.latestArtifactPath = artifact.latestPath;
        }
        try {
          const artifactContext = createSessionArtifactContext(ctx);
          appendSessionAuditEntry(artifactContext, {
            event: 'compaction.completed',
            detail: {
              checkpoint: details.label,
              reason: details.reason,
              fromExtension: details.fromExtension,
              ...(details.tokensBefore === undefined ? {} : { tokensBefore: details.tokensBefore }),
              ...(details.reclaimedTokens === undefined ? {} : { reclaimedTokens: details.reclaimedTokens }),
            },
          });
          const planScope = activePlanScope(ctx);
          const review = getPlanReviewState(planScope);
          const planContent = renderPlanContext(getCurrentPlanReadModel(ctx, planScope));
          const fixedCapture = rehydrationSegmentsProvider?.(ctx) ?? { segments: [], contents: {} };
          const registeredCapture = captureCurrentContextSources(ctx);
          const capture = mergeCompactionRehydrationCaptures(fixedCapture, registeredCapture);
          const segmentMap = new Map(capture.segments.map((segment) => [segment.id, segment]));
          segmentMap.set('active-plan', {
            version: 1,
            id: 'active-plan',
            kind: 'plan',
            origin: 'plan-domain',
            authority: 'user',
            digest: contentDigest(planContent),
            scope: 'task',
            visibility: 'transcript',
            rehydrate: 'always',
            tokenBudget: 15_000,
          });
          let consumerCursors: Record<string, number> = {};
          try {
            const awareness = openPersistentAwareness({ workspace: ctx.cwd ?? process.cwd() });
            try { consumerCursors = { tui: awareness.getConsumerCursor('tui'), rpc: awareness.getConsumerCursor('rpc') }; }
            finally { awareness.close(); }
          } catch { /* continuity metadata is best-effort; plan checkpoint still persists */ }
          writeRehydrationLedger(artifactContext, {
            capturedAt: new Date().toISOString(),
            segments: [...segmentMap.values()],
            segmentContents: { ...capture.contents, 'active-plan': planContent },
            plan: { scope: planScope, branchSnapshotId: review.branchSnapshotId, generation: review.generation, ...(review.revision ? { revision: review.revision } : {}) },
            pendingInteractionIds: listPendingInteractionIds(ctx),
            consumerCursors,
          });
          details.rehydrationLedgerPath = artifactContext.resolve('compaction/rehydration-v1.json');
          runAndRecordRehydration(pi, ctx, 'compaction');
        } catch {
          notify(
            ctx,
            'Compaction succeeded, but Octocode could not stage smart-resume metadata. Continue from Pi’s summary and the current active plan.',
            'warning',
          );
        }
        emitCompactionCheckpoint(pi, details);
      }
    } catch {
      notify(
        ctx,
        'Octocode compaction checkpoint failed unexpectedly. Compaction itself succeeded; context continuity metadata may be incomplete.',
        'warning',
      );
    }
  });
}

export const __test__ = {
  buildDeterministicCompaction,
  extractTextContent,
};
