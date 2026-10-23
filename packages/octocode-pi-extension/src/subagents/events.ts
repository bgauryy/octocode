import type { Usage } from '@earendil-works/pi-ai';
import { NEEDS_APPROVAL } from '../permission/dialog.js';
import { AUDIT_NOTIFY_PREFIX } from '../permission/audit.js';
import { redactSecrets, visibleControls } from '../shared/sanitize.js';
import { Thought, thoughtGist } from '../shared/thought.js';
import { addUsage, contentText, emptyUsage, finiteNumber, isRecord, toolHint } from '../shared/util.js';
import { imagesOf } from './screenshots.js';

export interface WatchdogLimits {
  /**
   * Quiet time before a child counts as hung: no running tool, not settling after its answer, and no event at all. A
   * running tool (or the hold on background jobs after the answer) is busy, however long it stays silent: it ends on
   * its own deadline, and the user can stop it.
   */
  idleMs: number;
  /**
   * How long a busy child may stay busy past a call (or its answer) when that call declares no `timeout`; a call that
   * declares one gets that. Past it plus `idleMs`, a tool that never ends (or a hold that never settles) is hung.
   */
  busyMs: number;
}

/** What a child's compactions came to (Pi's `compaction_start` / `compaction_end`): counted, never dropped, so a report can say how often and how well. */
export interface Compactions {
  /** Compactions that produced a summary. */
  done: number;
  /** Compactions stopped before they finished (the child was aborted, or an extension cancelled it). */
  aborted: number;
  /** Compactions that failed; `error` is the latest message. */
  failed: number;
  error?: string;
  /** Context size in tokens before and after the latest finished compaction. */
  tokensBefore?: number;
  tokensAfter?: number;
}

/** A warning or error the child's extensions showed (`notify` with `notifyType` warning or error): the child has no screen, the parent keeps them. Whole, whatever its length. */
interface ChildNotice {
  level: 'warning' | 'error';
  text: string;
  /** How many times the child showed this exact notice (same level and text): repeats fold into one. */
  count: number;
}

/** `error: text`, or `warning (×3): text` for a notice shown three times. */
export const noticeText = (notice: ChildNotice): string => `${notice.level}${notice.count > 1 ? ` (×${notice.count})` : ''}: ${notice.text}`;

/**
 * The inline page of a run's notices: the first distinct notices in the order they came, at most `NOTICES_INLINE` and
 * `NOTICES_INLINE_CHARS` characters of text. The page holds whole notices only; the rest are counted in
 * `ChildNotices.more` and every one is in the run's saved log (`notices.txt`).
 */
export const NOTICES_INLINE = 5;
export const NOTICES_INLINE_CHARS = 4_000;

export interface ChildNotices {
  /** The inline page (see `NOTICES_INLINE`), counts included. */
  kept: ChildNotice[];
  /** Distinct notices that came after the inline page. */
  more: number;
  /** The latest error raised since the child's last model turn, whether or not it fits in `kept`: the reason an aborted child gives. A later turn means the child went on, so the error is not why it stopped. */
  lastError?: string;
  /** The file holding every notice, whole, one per line; set when the run ends with `more` above 0. */
  file?: string;
}

/** One step of a running child, for the parent's live row. */
export interface ChildProgress {
  /** A tool call it started (`→ bash ls`). */
  activity?: string | undefined;
  /** A step of its reasoning that is not a tool call: a thought's gist (`✻ …`) or its narration (`› …`) once a reply ends. */
  step?: string | undefined;
  /** The gist of the thinking it streams now; empty once that thinking ends. */
  thinking?: string | undefined;
  /** Its usage so far. */
  usage: Usage;
  /** The assistant reply that just ended (with its `provider`, `model` and `usage`): its model may have no pricing. */
  reply?: Record<string, unknown>;
  /** The latest screenshot a tool returned. */
  image?: { data: string; mimeType: string };
  /** A tool call the child could not run without the user's approval (it has no one to ask). */
  denied?: boolean;
  /** The child waits for the user's answer to a dialog (`true`), or that answer arrived (`false`). */
  waiting?: boolean;
  /** The child compacts its context now (`true`), or that compaction ended (`false`). */
  compacting?: boolean;
  /** Its compactions so far, sent when one ends. */
  compactions?: Compactions;
  /** Its warnings and errors so far, sent when a new one arrives. */
  notices?: ChildNotices;
}

/**
 * What the events tell about a running child. The reducer (`reduceEvent`) updates it; the watchdog, `stop` and the exit
 * path of `runSubagent` read it, so the order of updates within one event is part of the behavior.
 */
export interface ChildState {
  /** Between `agent_start` and `agent_settled`: only a running child has anything to abort. */
  running: boolean;
  /** `agent_settled` arrived: the child leaves on its own once its input is closed. */
  settledSeen: boolean;
  toolsRunning: number;
  /** The answer is in and the child holds for its background jobs. */
  settling: boolean;
  /** While busy (tools running, or settling after the answer), the child is hung only past this moment. */
  busyUntil: number;
  /** The longest deadline of a background job it started: settling waits for timed jobs. */
  jobsMs: number;
  lastActivity: string;
  usage: Usage;
  thought: Thought;
  compactions: Compactions;
  notices: ChildNotices;
  /** Every distinct notice, keyed by level and text, in the order they first came: the source of the saved log. `notices.kept` shares its first entries. */
  noticeLog: Map<string, ChildNotice>;
}

export function newChildState(): ChildState {
  return {
    running: false, settledSeen: false, toolsRunning: 0, settling: false, busyUntil: 0, jobsMs: 0, lastActivity: 'started', usage: emptyUsage(), thought: new Thought(),
    compactions: { done: 0, aborted: 0, failed: 0 }, notices: { kept: [], more: 0 }, noticeLog: new Map(),
  };
}

/** What the lifecycle does for an event, in order, once the state is updated. */
export type ChildEffect =
  | { kind: 'progress'; update: ChildProgress }
  /** A dialog of the child (`extension_ui_request`) for the relay. */
  | { kind: 'ui'; event: Record<string, unknown> }
  /** The child refused its task: the run fails with this message and stops. */
  | { kind: 'rejected'; message: string }
  /** Tell the child to shut down in order (close its stdin). */
  | { kind: 'close' }
  /** The run is over (`agent_settled`); `aborted` is what the child said, before the lifecycle weighs a stall. */
  | { kind: 'settled'; aborted: boolean }
  /** A message that is not the child's own reply (its prompt, a team message) reached it. */
  | { kind: 'incoming'; role: unknown }
  /** An assistant reply ended: the report tracker reads its text. */
  | { kind: 'answer'; text: string | undefined; stopReason: unknown; errorMessage: unknown };

export function parseEvent(line: string): Record<string, unknown> | undefined {
  try {
    const value = JSON.parse(line) as unknown;
    return isRecord(value) ? value : undefined;
  } catch {
    return undefined;
  }
}

export function assistantText(message: unknown): string | undefined {
  if (!isRecord(message) || message['role'] !== 'assistant' || !Array.isArray(message['content'])) return undefined;
  const text = contentText(message['content']).trim();
  return text || undefined;
}

/** A reply's reasoning as trail lines: each thinking block's gist, then its narration when tool calls follow (a final answer is the report). */
function replySteps(message: Record<string, unknown>): string[] {
  if (!Array.isArray(message['content'])) return [];
  const steps: string[] = [];
  for (const part of message['content']) {
    if (!isRecord(part)) continue;
    const gist = part['type'] === 'thinking' && typeof part['thinking'] === 'string' ? thoughtGist(part['thinking'], true) : undefined;
    if (gist) steps.push(`✻ ${gist}`);
  }
  const narration = message['stopReason'] === 'toolUse' ? assistantText(message) : undefined;
  if (narration) steps.push(`› ${redactSecrets(narration.replace(/\s+/g, ' '))}`);
  return steps;
}

/** A call's own deadline in ms: its `timeout` argument (seconds), when it declares one. */
function declaredTimeout(args: unknown): number | undefined {
  const seconds = isRecord(args) ? args['timeout'] : undefined;
  return typeof seconds === 'number' && seconds > 0 ? seconds * 1000 : undefined;
}

function describeToolCall(name: unknown, args: unknown): string {
  const hint = toolHint(args);
  return `→ ${String(name)}${hint ? ` ${hint}` : ''}`;
}

/** The prompt command's own answer; every other `response` is for a command this parent never sends. */
function promptResponse(event: Record<string, unknown>): ChildEffect[] {
  if (event['command'] !== 'prompt') return [];
  if (event['success'] === false) return [{ kind: 'rejected', message: `Subagent rejected its task: ${typeof event['error'] === 'string' ? event['error'] : 'no reason given'}` }];
  // An extension command consumed the prompt: no run starts, so no `agent_settled` will come.
  return isRecord(event['data']) && event['data']['disposition'] === 'handled' ? [{ kind: 'close' }] : [];
}

/** A compaction ended: counts it, and adds the summary's own model usage to the run (it is no reply, so no `message_end` carries it). */
function endCompaction(state: ChildState, event: Record<string, unknown>): void {
  const result = event['result'];
  if (isRecord(result)) {
    const tally = state.compactions;
    tally.done += 1;
    const [before, after] = [finiteNumber(result['tokensBefore']), finiteNumber(result['estimatedTokensAfter'])];
    if (before !== undefined) tally.tokensBefore = before;
    if (after !== undefined) tally.tokensAfter = after;
    addUsage(state.usage, result['usage']);
  } else if (event['aborted'] === true) state.compactions.aborted += 1;
  else {
    state.compactions.failed += 1;
    const message = event['errorMessage'];
    state.compactions.error = visibleControls(typeof message === 'string' && message ? message : 'compaction failed');
  }
}

/**
 * A child's `notify` of level warning or error (the audit channel and info notes are not for the parent's report): kept
 * whole, folded into its identical earlier one with a count, else logged as a new notice; it joins the inline page while
 * the page has room, and once one does not fit, none after it joins (the page is a prefix, so it never skips ahead).
 */
function noteNotice(state: ChildState, event: Record<string, unknown>): boolean {
  const [message, level] = [event['message'], event['notifyType']];
  if (event['method'] !== 'notify' || typeof message !== 'string' || message.startsWith(AUDIT_NOTIFY_PREFIX) || (level !== 'warning' && level !== 'error')) return false;
  const text = visibleControls(message.replace(/\s+/g, ' ').trim());
  const key = `${level}\u0000${text}`;
  const seen = state.noticeLog.get(key);
  if (seen) seen.count += 1;
  else {
    const notice: ChildNotice = { level, text, count: 1 };
    state.noticeLog.set(key, notice);
    const { kept } = state.notices;
    const room = state.notices.more === 0 && kept.length < NOTICES_INLINE && kept.reduce((chars, entry) => chars + entry.text.length, text.length) <= NOTICES_INLINE_CHARS;
    if (room) kept.push(notice);
    else state.notices.more += 1;
  }
  if (level === 'error') state.notices.lastError = text;
  return true;
}

/**
 * Applies one parsed event of the child to `state` and returns what the lifecycle does about it, in order. `now` is the
 * clock for the busy deadline. Nothing here touches the process, the report or the parent's row.
 */
export function reduceEvent(state: ChildState, event: Record<string, unknown>, limits: WatchdogLimits, now: number): ChildEffect[] {
  const type = event['type'];
  if (type === 'response') return promptResponse(event);
  if (type === 'extension_ui_request') {
    const ui: ChildEffect = { kind: 'ui', event };
    return noteNotice(state, event) ? [ui, { kind: 'progress', update: { notices: structuredClone(state.notices), usage: state.usage } }] : [ui];
  }
  // Calls a tool makes itself (a codemode script) are part of that call, not activity of their own.
  if (event['parentToolCallId']) return [];
  const effects: ChildEffect[] = [];
  const progress = (update: Omit<ChildProgress, 'usage'>) => effects.push({ kind: 'progress', update: { ...update, usage: state.usage } });
  if (type === 'agent_start') state.running = true;
  if (type === 'compaction_start') {
    state.lastActivity = 'compacting its context';
    progress({ compacting: true });
    return effects;
  }
  if (type === 'compaction_end') {
    endCompaction(state, event);
    state.lastActivity = 'compacted its context';
    progress({ compacting: false, compactions: { ...state.compactions } });
    return effects;
  }
  if (type === 'agent_settled') {
    state.running = false;
    state.settledSeen = true;
    // The run is over and the report is in: the child has nothing left to do, so the lifecycle tells it to leave.
    return [{ kind: 'settled', aborted: event['aborted'] === true }];
  }
  // A new turn or a streaming reply means the previous turn's tools are over, even one that never reported its end.
  if (type === 'turn_start' || type === 'message_update') state.toolsRunning = 0;
  // A turn after the answer (a background job's report woke it) is a model turn again, not settling.
  if (type === 'turn_start') state.settling = false;
  if (type === 'message_update' && isRecord(event['assistantMessageEvent'])) {
    const update = event['assistantMessageEvent'];
    if (update['type'] === 'thinking_start') state.thought.reset();
    if (update['type'] === 'thinking_delta' && typeof update['delta'] === 'string' && state.thought.add(update['delta'])) progress({ thinking: state.thought.gist ?? '' });
    return effects;
  }
  if (type === 'tool_execution_start') {
    state.toolsRunning++;
    state.settling = false;
    const own = declaredTimeout(event['args']);
    if (own !== undefined && isRecord(event['args']) && event['args']['background'] === true) state.jobsMs = Math.max(state.jobsMs, own);
    state.busyUntil = Math.max(state.busyUntil, now + (own ?? limits.busyMs) + limits.idleMs);
    state.lastActivity = describeToolCall(event['toolName'], event['args']);
    progress({ activity: state.lastActivity });
  }
  if (type === 'tool_execution_end') {
    state.toolsRunning = Math.max(0, state.toolsRunning - 1);
    const image = imagesOf(event['result']).at(-1);
    if (image) progress({ image });
    const result = event['result'];
    if (event['isError'] === true && isRecord(result) && contentText(result['content']).startsWith(NEEDS_APPROVAL)) progress({ denied: true });
  }
  if (type !== 'message_end' || !isRecord(event['message'])) return effects;
  const message = event['message'];
  if (message['role'] !== 'assistant') return [...effects, { kind: 'incoming', role: message['role'] }];
  addUsage(state.usage, message['usage']);
  delete state.notices.lastError;
  state.thought.reset();
  for (const step of replySteps(message)) progress({ step });
  progress({ reply: message, thinking: '' });
  state.settling = message['stopReason'] !== 'toolUse';
  if (state.settling) state.busyUntil = now + Math.max(state.jobsMs, limits.busyMs) + limits.idleMs;
  state.lastActivity = state.settling ? 'answered; settling' : 'model turn';
  effects.push({ kind: 'answer', text: assistantText(message), stopReason: message['stopReason'], errorMessage: message['errorMessage'] });
  return effects;
}
