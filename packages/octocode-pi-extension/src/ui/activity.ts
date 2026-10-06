import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { formatDuration } from '../shared/format.js';
import { dialogsWaiting, onDialogsChange } from '../shared/locks.js';
import { clip } from '../shared/render.js';
import { toolHint } from '../shared/util.js';

/** What the model is doing between tool runs. */
export type Phase = 'idle' | 'model' | 'thinking' | 'writing' | 'calling';

interface RunningTool {
  name: string;
  hint: string;
  startedAt: number;
  /** Its `tool_call` checks (reservations, the bash guard, PreToolUse hooks) run now; set while they do. */
  checkingSince?: number;
}

const TICK_MS = 1_000;
/** Elapsed time shows once a step has taken this long. */
const SHOW_ELAPSED_MS = 2_000;
const HINT_MAX = 48;
const TITLE_MAX = 60;

/** `mcp__octocode__localSearch` → `localSearch`: the server prefix only costs width. */
export function shortToolName(name: string): string {
  return name.replace(/^mcp__[^_]+(?:_[^_]+)*?__/, '');
}

/**
 * The agent's state for a run, from Pi's events: model phase, the top-level tool calls running now (parallel calls all
 * count; nested calls a tool makes are part of it), and a blocking user prompt. Pure state: `describe` renders it.
 */
export class Activity {
  phase: Phase = 'idle';
  private phaseAt = 0;
  private readonly tools = new Map<string, RunningTool>();
  private prompt: { title: string; at: number } | undefined;

  start(now: number): void {
    this.tools.clear();
    this.prompt = undefined;
    this.setPhase('model', now);
  }

  end(): void {
    this.tools.clear();
    this.prompt = undefined;
    this.phase = 'idle';
  }

  /** An assistant streaming event: `thinking_delta`, `text_start`, `toolcall_delta`, … */
  stream(type: string, now: number): void {
    const phase: Phase | undefined = type.startsWith('thinking') ? 'thinking' : type.startsWith('text') ? 'writing' : type.startsWith('toolcall') ? 'calling' : undefined;
    if (phase && phase !== this.phase) this.setPhase(phase, now);
  }

  /** The assistant message is complete: its tools run next, or the run ends. */
  messageDone(now: number): void {
    if (this.phase !== 'idle') this.setPhase('model', now);
  }

  toolStart(id: string, name: string, args: unknown, now: number): void {
    this.tools.set(id, { name: shortToolName(name), hint: toolHint(args), startedAt: now });
  }

  toolEnd(id: string, now: number): void {
    if (this.tools.delete(id) && this.tools.size === 0 && this.phase !== 'idle') this.setPhase('model', now);
  }

  /** A call's `tool_call` checks started (`active`) or ended. Pi checks the calls of a batch one at a time. */
  checking(id: string, active: boolean, now: number): void {
    const tool = this.tools.get(id);
    if (!tool) return;
    if (active) tool.checkingSince = now;
    else delete tool.checkingSince;
  }

  promptStart(title: string | undefined, now: number): void {
    this.prompt = { title: title ?? '', at: now };
  }

  promptEnd(): void {
    this.prompt = undefined;
  }

  /**
   * One line for the state, or undefined when there is nothing beyond Pi's own "Working". `now` adds the elapsed time
   * of a long step; without it the line only changes when the state does (the team panel stores it).
   */
  describe(now?: number, queuedDialogs = 0): string | undefined {
    if (this.phase === 'idle') return undefined;
    const since = (at: number) => (now !== undefined && now - at >= SHOW_ELAPSED_MS ? ` · ${formatDuration(now - at)}` : '');
    if (this.prompt) {
      const queued = queuedDialogs > 0 ? ` (+${queuedDialogs} queued)` : '';
      return `Waiting for you${this.prompt.title ? `: ${clip(this.prompt.title, TITLE_MAX)}` : ''}${queued}${since(this.prompt.at)}`;
    }
    const running = [...this.tools.values()];
    const checking = running.find((tool) => tool.checkingSince !== undefined);
    if (checking) return `Checking ${checking.name}${checking.hint ? ` ${clip(checking.hint, HINT_MAX)}` : ''}${since(checking.checkingSince!)}`;
    if (running.length === 1) {
      const [tool] = running as [RunningTool];
      return `Running ${tool.name}${tool.hint ? ` ${clip(tool.hint, HINT_MAX)}` : ''}${since(tool.startedAt)}`;
    }
    if (running.length > 1) {
      const counts = new Map<string, number>();
      for (const tool of running) counts.set(tool.name, (counts.get(tool.name) ?? 0) + 1);
      const names = [...counts].map(([name, count]) => (count > 1 ? `${name} ×${count}` : name)).join(' · ');
      return `Running ${running.length} tools: ${names}${since(Math.min(...running.map((tool) => tool.startedAt)))}`;
    }
    if (this.phase === 'thinking') return `Thinking${since(this.phaseAt)}`;
    if (this.phase === 'writing') return 'Writing';
    if (this.phase === 'calling') return 'Preparing tool calls';
    return undefined;
  }

  /** Whether a step is under way whose elapsed time grows (worth a repaint every second). */
  ticking(): boolean {
    return this.phase !== 'idle' && (this.tools.size > 0 || this.prompt !== undefined || this.phase === 'thinking');
  }

  private setPhase(phase: Phase, now: number): void {
    this.phase = phase;
    this.phaseAt = now;
  }
}

/**
 * Feed `Activity` from Pi's events. `onChange` hears the line without elapsed time whenever it changes (the team panel
 * shows it to the parent and teammates). With a UI, Pi's working line shows the line with elapsed time, repainted each
 * second while a step runs; Pi's default comes back when the run ends.
 */
export interface ActivityControl {
  activity: Activity;
  /** The gate pipeline (src/index.ts) reports a call's `tool_call` checks starting and ending. */
  checking(id: string, active: boolean): void;
}

export function registerActivity(pi: ExtensionAPI, options: { onChange?: (line: string | undefined) => void; workingLine: boolean }): ActivityControl {
  const activity = new Activity();
  let ctx: ExtensionContext | undefined;
  let shown: string | undefined;
  let reported: string | undefined;
  let timer: NodeJS.Timeout | undefined;
  /** Repaints when interactions queue behind an open dialog; held per session. */
  let unsubscribe: (() => void) | undefined;

  const paint = () => {
    const line = activity.describe(undefined, dialogsWaiting());
    if (line !== reported) {
      reported = line;
      options.onChange?.(line);
    }
    if (!options.workingLine || !ctx?.hasUI) return;
    const working = activity.describe(Date.now(), dialogsWaiting());
    if (working !== shown) {
      shown = working;
      try {
        ctx.ui.setWorkingMessage(working);
      } catch {
        // The session was replaced.
      }
    }
    if (activity.ticking() && !timer) {
      timer = setInterval(paint, TICK_MS);
      timer.unref?.();
    } else if (!activity.ticking() && timer) {
      clearInterval(timer);
      timer = undefined;
    }
  };
  const stop = () => {
    activity.end();
    paint();
  };

  pi.on('session_start', async (_event, next) => {
    ctx = next;
    shown = undefined;
    unsubscribe?.();
    unsubscribe = onDialogsChange(paint);
    stop();
  });
  pi.on('session_shutdown', async () => {
    unsubscribe?.();
    unsubscribe = undefined;
    stop();
    ctx = undefined;
  });
  pi.on('agent_start', async (_event, next) => {
    ctx = next;
    activity.start(Date.now());
    paint();
  });
  pi.on('message_update', async (event) => {
    if (event.message.role !== 'assistant') return;
    const before = activity.phase;
    activity.stream(event.assistantMessageEvent.type, Date.now());
    if (activity.phase !== before) paint();
  });
  pi.on('message_end', async (event) => {
    if (event.message.role !== 'assistant') return;
    activity.messageDone(Date.now());
    paint();
  });
  pi.on('tool_execution_start', async (event) => {
    if (event.parentToolCallId) return;
    activity.toolStart(event.toolCallId, event.toolName, event.args, Date.now());
    paint();
  });
  pi.on('tool_execution_end', async (event) => {
    if (event.parentToolCallId) return;
    activity.toolEnd(event.toolCallId, Date.now());
    paint();
  });
  pi.on('ui_prompt_start', async (event) => {
    activity.promptStart(event.title, Date.now());
    paint();
  });
  pi.on('ui_prompt_end', async () => {
    activity.promptEnd();
    paint();
  });
  pi.on('agent_settled', async () => stop());
  return {
    activity,
    checking: (id, active) => {
      activity.checking(id, active, Date.now());
      paint();
    },
  };
}
