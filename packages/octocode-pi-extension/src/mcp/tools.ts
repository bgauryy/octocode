import type { AgentToolResult, ToolAnnotations as PiToolAnnotations, ToolDefinition } from '@earendil-works/pi-coding-agent';
import { toLlmContent, type CallToolResult, type Tool } from '@earendil-works/pi-mcp';
import type { TSchema } from 'typebox';
import { clip, queryTarget, resultBlock, resultText, toolHeader } from '../shared/render.js';
import { timedTool, timingOf } from '../shared/tool-timing.js';
import { visibleControls } from '../shared/sanitize.js';
import { boundImagePart, capOutputToFile } from '../shared/spill.js';
import { isRecord } from '../shared/util.js';
import { imageLimits, type ImageLimits } from '../shared/vision.js';

/** What the extension registers a server's tool as. */
interface ToolBinding {
  /** Pi's tool name prefix for the server, `mcp__octocode__`. */
  prefix: string;
  /** The tools' namespace: the server's name and its instructions. */
  namespace: NonNullable<ToolDefinition['namespace']>;
  call(name: string, args: Record<string, unknown>, signal?: AbortSignal): Promise<CallToolResult>;
  /** The text the turn's results have put in the context so far; every result of one assistant message shares it. */
  budget?: TurnBudget;
}

/** Hints the server gave about the tool, as Pi's annotations (booleans only: nothing else is a hint). */
function annotations(tool: Tool): PiToolAnnotations | undefined {
  const hints = tool.annotations;
  if (!hints) return undefined;
  const picked: PiToolAnnotations = {};
  for (const key of ['readOnlyHint', 'destructiveHint', 'idempotentHint', 'openWorldHint'] as const) {
    if (typeof hints[key] === 'boolean') picked[key] = hints[key];
  }
  return Object.keys(picked).length > 0 ? picked : undefined;
}

/** UTF-16 characters kept whole per MCP text part, allowing headroom around the server's 50,000-character page. */
export const TEXT_MAX_CHARS = 60_000;
/** Characters of tool results one assistant message may put in the context whole (Claude Code's per-message budget). */
export const TURN_MAX_CHARS = 200_000;
/** The preview a result keeps once its turn is over budget: the head and tail of a result about 1,000 tokens long. */
const OVER_BUDGET_CHARS = 4_000;

/**
 * What the MCP results of one assistant message have put in the context. Pi runs a turn's parallel tool calls
 * together, so each gets whole text only while the turn is under `TURN_MAX_CHARS`: a result that would pass it is cut
 * to what is left (at least `OVER_BUDGET_CHARS`) and the later ones to that preview. Every cut result is saved
 * whole to a file the preview names, so the budget bounds the context, not the evidence. `reset` at `turn_start`.
 */
export class TurnBudget {
  private used = 0;

  /** UTF-16 characters the next text part may keep while the turn is within its budget. */
  limit(): number {
    return Math.min(TEXT_MAX_CHARS, Math.max(TURN_MAX_CHARS - this.used, OVER_BUDGET_CHARS));
  }

  /** Counts `chars` characters the model now reads. */
  charge(chars: number): void {
    this.used += chars;
  }

  reset(): void {
    this.used = 0;
  }
}

/**
 * A tool result for Pi. Text and images pass through, embedded text resources become text and other blocks a short
 * placeholder (`toLlmContent`). A server bounds and pages its own output and a cut can hide the continuation inside
 * it, so text is left whole up to `TEXT_MAX_CHARS` (less once the turn's `budget` is nearly used); a longer part is
 * saved whole to the session's `output/` folder and replaced by its head and tail plus the file's path. An oversized
 * image is saved and resized by Pi to `limits` (the active model's image limit, `imageLimits(ctx.model)`); the model sees the fitted image, its original path and the
 * dimension mapping (only the path if resizing fails). Repository, GitHub and registry text is untrusted: terminal control
 * characters, bidi overrides and invisible characters (Unicode tags among them) are shown as visible escapes
 * (`\u{200d}`, `\r`) in every string before the model reads it or the terminal draws it, so nothing the server
 * returned disappears and nothing raw reaches the terminal. `structuredContent` is the server's, kept for tools that declare an
 * `outputSchema`. An error result is the server's text as an error.
 */
export async function toToolResult(tool: string, result: CallToolResult, declaresOutput: boolean, limits?: ImageLimits, budget?: TurnBudget): Promise<AgentToolResult<{ tool: string }>> {
  const content: AgentToolResult['content'] = [];
  for (const part of sanitizeJson(toLlmContent(result)) as ReturnType<typeof toLlmContent>) {
    if (part.type === 'text') {
      const maxChars = budget?.limit() ?? TEXT_MAX_CHARS;
      const text = part.text.length <= maxChars ? part.text : capOutputToFile(part.text, { label: `mcp ${tool}`, maxBytes: maxChars, maxLines: maxChars + 1 });
      budget?.charge(text.length);
      content.push({ ...part, text });
    }
    else if (part.type === 'image') content.push(...await boundImagePart(part, `mcp ${tool} image`, limits ? { limits } : {}));
    else content.push(part);
  }
  if (result.isError && content.length === 0) content.push({ type: 'text', text: `${tool} failed without a message.` });
  return {
    content,
    details: { tool },
    ...(declaresOutput && result.structuredContent !== undefined ? { structuredContent: sanitizeJson(result.structuredContent) as AgentToolResult['structuredContent'] } : {}),
    ...(result.isError ? { isError: true } : {}),
  };
}

/** Every string in a JSON value with its control and invisible characters made visible; the same object when nothing changed. */
export function sanitizeJson(value: unknown): unknown {
  if (typeof value === 'string') return visibleControls(value);
  if (Array.isArray(value)) {
    const next = value.map(sanitizeJson);
    return next.some((item, i) => item !== value[i]) ? next : value;
  }
  if (value && typeof value === 'object') {
    let changed = false;
    const next: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(value)) {
      next[key] = sanitizeJson(item);
      if (next[key] !== item) changed = true;
    }
    return changed ? next : value;
  }
  return value;
}

/** The call's header: the tool name, then what its first query looks for; a batch adds one line per query. */
function callSummary(args: unknown): { summary: string; queries: string[] } {
  const queries = isRecord(args) && Array.isArray(args['queries']) ? args['queries'].filter(isRecord) : [];
  const targets = queries.map((query) => clip(queryTarget(query), 200));
  return { summary: targets.length === 1 ? targets[0]! : targets.length > 1 ? `${targets.length} queries` : '', queries: targets.length > 1 ? targets : [] };
}

/**
 * The Pi tool for one tool the server lists. Its input schema is the server's JSON Schema as is (Pi validates calls
 * against it), and every definition is `direct`: declared to the model from the first request, no `tool_search` step.
 */
export function octocodeToolDefinition(tool: Tool, binding: ToolBinding): ToolDefinition {
  const hints = annotations(tool);
  const declaresOutput = tool.outputSchema !== undefined;
  return timedTool({
    name: `${binding.prefix}${tool.name}`,
    label: tool.title?.trim() || tool.name,
    description: tool.description?.trim() || tool.title?.trim() || `Octocode tool ${tool.name}`,
    parameters: tool.inputSchema as unknown as TSchema,
    ...(declaresOutput ? { outputSchema: tool.outputSchema as unknown as TSchema } : {}),
    exposure: 'direct',
    namespace: binding.namespace,
    ...(hints ? { annotations: hints } : {}),
    renderCall(args, theme, context) {
      const { summary, queries } = callSummary(args);
      const lines = queries.map((query) => `${theme.fg('mdCode', query)}`);
      return toolHeader(theme, context, tool.name, summary, { lines });
    },
    renderResult(result, _options, theme, context) {
      const text = resultText(result);
      // An error is one message: its first line is the summary, the rest the body.
      const failed = context.isError === true;
      return resultBlock(theme, context, { summary: failed ? text : '', body: failed ? text.split('\n').slice(1).join('\n') : text, ...timingOf(result.details) });
    },
    async execute(_toolCallId, params, signal, _onUpdate, ctx) {
      return toToolResult(tool.name, await binding.call(tool.name, isRecord(params) ? params : {}, signal), declaresOutput, imageLimits(ctx.model), binding.budget);
    },
  });
}
