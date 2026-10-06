import type { AgentToolResult, MessageRenderer, Theme, ToolRenderResultOptions } from '@earendil-works/pi-coding-agent';
import { Box, type Component } from '@earendil-works/pi-tui';
import { formatDuration, formatTokens, shortPath } from '../shared/format.js';
import { clip, timingOf, plural, resultBlock, toolHeader, type RenderContext } from '../shared/render.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { contentText, isRecord } from '../shared/util.js';

/**
 * How the `agent` tool and a background subagent's report message are drawn: `● Agent(profile) · 38s`, then live
 * progress while it runs and `⎿ Done (4 tool calls · 12k tokens · 38s)` with the answer once it finishes. The row
 * stays one short line; the full prompt the agent was given shows in the expanded view, above its progress or report.
 */

/** The `agent` tool's result details, updated as the child streams. */
export interface RunDetails {
  id: string;
  profile: string;
  toolCalls: number;
  activity: string[];
  input: number;
  output: number;
  startedAt: number;
  seconds?: number;
  /** running: foreground, still working; background: returned at once, the report arrives as a message. */
  status: 'running' | 'background' | 'done' | 'failed';
  /** The subagent's latest screenshot, saved to this path. */
  shot?: string;
}

/** What the background-result message carries for its renderer: the outcome, not parsed back out of the text. */
export interface ResultDetails {
  id: string;
  status: 'done' | 'failed' | 'stopped';
  /** The `⎿` line, e.g. `Done (4 tool calls · 12k tokens · 38s)`. */
  summary: string;
  durationMs?: number;
  /** A stop someone asked for (`/agents kill`, `coordinate stop`): drawn as an outcome, not as a failure. */
  requested?: boolean;
  /** The full prompt the subagent was given, shown when the report is expanded. */
  task?: string;
}

/** `Done (4 tool calls · 12k tokens · 38s)`. */
export function doneSummary(details: Pick<RunDetails, 'toolCalls' | 'input' | 'output'>, durationMs: number | undefined): string {
  return `Done (${[plural(details.toolCalls, 'tool call'), `${formatTokens(details.input + details.output)} tokens`, durationMs === undefined ? '' : formatDuration(durationMs, true)].filter(Boolean).join(' · ')})`;
}

interface AgentArgs {
  task?: string | undefined;
  profile?: string | undefined;
  model?: string | undefined;
  background?: boolean | undefined;
  isolate?: boolean | undefined;
}

const argsOf = (context: RenderContext): AgentArgs => (isRecord(context.args) ? (context.args as AgentArgs) : {});

/**
 * `Prompt` and the task, then the heading of what follows (`Report`, `Progress`): the expanded view's preface. Task
 * text comes from the model and may carry escape sequences; it is sanitized here.
 */
export function promptPreface(theme: Theme, task: string | undefined, next: string | undefined): string[] {
  const rows = sanitizeTerminalText(task ?? '').replace(/\t/g, '   ').split('\n');
  while (rows.length > 0 && rows.at(-1)!.trim() === '') rows.pop();
  while (rows.length > 0 && rows[0]!.trim() === '') rows.shift();
  if (rows.length === 0) return [];
  const heading = (text: string) => theme.fg('accent', theme.bold(text));
  return [heading('Prompt'), ...rows.map((row) => theme.fg('muted', row)), ...(next ? ['', heading(next)] : [])];
}

/** `implementer · gpt-5 · isolated`: what the row says about the run, never the prompt. */
function runLabel(args: AgentArgs): string {
  return [args.profile ?? 'general', args.model ? clip(args.model, 40) : '', args.isolate ? 'isolated' : ''].filter(Boolean).join(' · ');
}

export function renderAgentCall(args: AgentArgs, theme: Theme, context: RenderContext): Component {
  return toolHeader(theme, context, 'Agent', runLabel(args), args.background ? { meta: 'background' } : {});
}

export function renderAgentResult(result: AgentToolResult<unknown>, { isPartial }: ToolRenderResultOptions, theme: Theme, context: RenderContext): Component {
  const details = result.details as RunDetails | undefined;
  const body = contentText(result.content);
  const task = argsOf(context).task;
  if (context.isError || !details) {
    const [head = '', ...rest] = body.split('\n');
    const more = rest.join('\n');
    return resultBlock(theme, context, { summary: head, body: more, prefaceLabel: 'prompt', preface: promptPreface(theme, task, more.trim() ? 'Output' : undefined), ...timingOf(result.details) });
  }
  if (isPartial) {
    // Live progress: counts, then the latest steps (the header carries the running clock).
    const lines = [...details.activity.slice(-3).map((line) => theme.fg('dim', clip(line, 200))), ...(details.shot ? [theme.fg('dim', `📷 ${clip(shortPath(details.shot), 200)}`)] : [])];
    return resultBlock(theme, context, { summary: `${details.id} · ${plural(details.toolCalls, 'tool call')} · ↑${formatTokens(details.input)} ↓${formatTokens(details.output)}`, lines, max: lines.length, prefaceLabel: 'prompt', preface: promptPreface(theme, task, lines.length > 0 ? 'Progress' : undefined) });
  }
  if (details.status === 'background') {
    // The first line is for the model; only a later note (an isolation warning) is worth expanding to.
    const note = body.split('\n').slice(1).join('\n').trim();
    return resultBlock(theme, context, { summary: `Running in the background as ${details.id}`, ...(note ? { body: note } : {}), prefaceLabel: 'prompt', preface: promptPreface(theme, task, note ? 'Note' : undefined), max: 0, ...timingOf(result.details) });
  }
  const timing = timingOf(result.details);
  const durationMs = timing.durationMs ?? (details.seconds === undefined ? undefined : details.seconds * 1000);
  return resultBlock(theme, context, { summary: doneSummary(details, durationMs), body, prefaceLabel: 'prompt', preface: promptPreface(theme, task, body.trim() ? 'Report' : undefined), ...timing, ...(durationMs === undefined ? {} : { durationMs }) });
}

/** The report message's `Task: …` line: for the model (it names the task a report answers); the renderer shows the full prompt instead. */
const TASK_LINE = /^Task: /;

/**
 * A background subagent's report, drawn like the tool row it came from: `● Agent(id)` then `⎿ Done (…)` and
 * the report; expanded, the full prompt comes first.
 */
export const renderAgentMessage: MessageRenderer = (message, { expanded, outputPad }, theme) => {
  const details = (message.details ?? {}) as Partial<ResultDetails>;
  const [head = '', ...rest] = contentText(message.content).split('\n');
  // Older reports carry no task in their details: keep their task line as text.
  if (details.task !== undefined && rest.length > 0 && TASK_LINE.test(rest[0]!)) rest.shift();
  const report = rest.join('\n');
  const stopped = details.status === 'stopped';
  // A stop (asked for or not) is an interruption, not a failure: drawn `◼` in the warning colour.
  const context = { isPartial: false, isError: details.status === 'failed' || stopped, expanded, state: stopped ? { stopped: true } : {} };
  // Pi draws custom messages flush left; pad them like a tool row.
  const box = new Box(outputPad ?? 1, 0);
  box.addChild(toolHeader(theme, context, 'Agent', details.id ?? 'background', details.durationMs === undefined ? { meta: 'background' } : { meta: 'background', durationMs: details.durationMs }));
  box.addChild(resultBlock(theme, context, { summary: details.summary ?? head, body: report, stopped, prefaceLabel: 'prompt', preface: promptPreface(theme, details.task, report.trim() ? 'Report' : undefined) }));
  return box;
};
