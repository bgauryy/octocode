import type { Api, Model, SimpleStreamOptions } from '@earendil-works/pi-ai';
import { convertToLlm, type CompactionResult, type ExtensionAPI, type ExtensionContext, type SessionBeforeCompactEvent } from '@earendil-works/pi-coding-agent';
import { sessionCleared } from '../files/reads.js';
import { CHARS_PER_TOKEN } from '../shared/tokens.js';
import { clipText, contentText, errorMessage, isRecord } from '../shared/util.js';
import { requestMessages } from './thinking.js';

/**
 * The summary request that reads the conversation from the prompt cache. Pi's `compact()` serializes the conversation
 * into a new prompt under its own system prompt, so the provider sees a prefix it never cached and bills the whole
 * context as fresh input. Here the request is the session's own (the same system and tool declarations, messages, model,
 * thinking level and routing id) with one user message added at the end: the provider finds the cached prefix of the
 * session's last request and bills only that message. Tools stay declared and the thinking settings stay as they are,
 * because changing either invalidates the cached messages; the reply is told to be text only, and a reply that calls a
 * tool counts as a failure. Any failure returns nothing and the caller writes the summary the standalone way.
 */

/** Pi's summary format, as the request to write one now. Pi's own prompts are not exported. */
export const SHARED_SUMMARY_PROMPT = `Stop here and write a context checkpoint summary of the conversation above, which another LLM will use to continue the work. Reply with text only: do not call any tool, and do not continue the work. This request is not part of the conversation: do not record it in the summary.

Use this EXACT format:

## Goal
[What is the user trying to accomplish? Can be multiple items if the session covers different tasks.]

## Constraints & Preferences
- [Any constraints, preferences, or requirements mentioned by user]
- [Or "(none)" if none were mentioned]

## Progress
### Done
- [x] [Completed tasks/changes]

### In Progress
- [ ] [Current work]

### Blocked
- [Issues preventing progress, if any]

## Key Decisions
- **[Decision]**: [Brief rationale]

## Next Steps
1. [Ordered list of what should happen next]

## Critical Context
- [Any data, examples, or references needed to continue]
- [Or "(none)" if not applicable]

Keep each section concise. Preserve exact file paths, function names, and error messages. Code adds the lists of files read and modified and the files saved from tool results after your summary: do not write \`<read-files>\`, \`<modified-files>\` or \`<saved-results>\` blocks.`;

/** Said when the conversation starts with an earlier summary, which this one replaces. */
const PREVIOUS_SUMMARY_NOTE = `The conversation begins with an earlier summary. Update it: keep its entries (goals, progress, decisions, context), move finished work to Done, and add what the later messages show; newer messages win where they contradict it. Code appended the sections "## User Messages" and "## Recovery" and the \`<read-files>\`, \`<modified-files>\` and \`<saved-results>\` blocks to that summary and appends them again to yours: do not copy them.`;

/** The longest a quoted start of a message gets. */
const QUOTE_CHARS = 120;

const quote = (text: string): string => clipText(text.replace(/\s+/g, ' ').trim(), QUOTE_CHARS);

/** How the message is told apart in words, or undefined when it has nothing to quote. */
function describeMessage(message: unknown): string | undefined {
  if (!isRecord(message)) return undefined;
  const content = message['content'];
  const text = contentText(content);
  if (message['role'] === 'toolResult') return `result of the ${String(message['toolName'])} tool call`;
  const role = message['role'] === 'user' || message['role'] === 'assistant' ? message['role'] : undefined;
  if (!role) return undefined;
  if (quote(text)) return `${role} message beginning "${quote(text)}"`;
  const toolCall = Array.isArray(content) ? content.find((part) => isRecord(part) && part['type'] === 'toolCall') : undefined;
  return isRecord(toolCall) ? `${role} message that calls the ${String(toolCall['name'])} tool with ${quote(JSON.stringify(toolCall['arguments']))}` : undefined;
}

/**
 * Where the kept messages begin, as the model can recognise it. The model sees the whole conversation, but the messages
 * from `firstKeptEntryId` on stay in context after the summary, so it summarizes what comes before them.
 */
function boundaryNote(entries: ReadonlyArray<{ sourceEntry: { id: string }; messages: unknown[] }>, firstKeptEntryId: string): string {
  const kept = entries.find((entry) => entry.sourceEntry.id === firstKeptEntryId);
  const first = kept?.messages.map(describeMessage).find((description) => description !== undefined);
  return first === undefined ? '' : `The conversation continues after your summary with the messages from the ${first} on; they stay in context word for word, so summarize only what comes before that point and leave out what happens from there on, although you can see it. Where the instructions below say "the last messages above", they mean the last ones before it.`;
}

/** Pi's file lists: files read and never changed, and files changed (edited or written), each sorted. */
function fileLists(fileOps: SessionBeforeCompactEvent['preparation']['fileOps']): { readFiles: string[]; modifiedFiles: string[] } {
  const modified = new Set([...fileOps.edited, ...fileOps.written]);
  return { readFiles: [...fileOps.read].filter((file) => !modified.has(file)).sort(), modifiedFiles: [...modified].sort() };
}

/** Pi's format of the lists appended to a summary. */
function fileBlocks({ readFiles, modifiedFiles }: { readFiles: string[]; modifiedFiles: string[] }): string {
  const blocks = [
    ...(readFiles.length > 0 ? [`<read-files>\n${readFiles.join('\n')}\n</read-files>`] : []),
    ...(modifiedFiles.length > 0 ? [`<modified-files>\n${modifiedFiles.join('\n')}\n</modified-files>`] : []),
  ];
  return blocks.length > 0 ? `\n\n${blocks.join('\n\n')}` : '';
}

/** Pi's cap on a summary: 80% of the reserve, and never beyond what the model can write. */
function summaryCapTokens(model: Pick<Model<Api>, 'maxTokens'>, reserveTokens: number): number {
  return Math.min(Math.floor(0.8 * reserveTokens), model.maxTokens > 0 ? model.maxTokens : Number.POSITIVE_INFINITY);
}

/**
 * Octocode's compaction result from the cache-sharing request, or undefined when it cannot be used: images are blocked
 * (the request would carry them), the call or the projection failed, or the reply stopped early, is empty, calls a tool
 * or is longer than the cap. `instructions` are the user's `/compact` focus and Octocode's own. `unusable` gets the reason
 * for every failure, so the caller can say why it falls back to a request that bills the whole input; `userChoice` marks
 * blocked images, the user's own setting, which needs no warning. A cancel (abort) reports nothing.
 */
export async function sharedSummary(
  pi: Pick<ExtensionAPI, 'getThinkingLevel' | 'getSettings'>,
  event: Pick<SessionBeforeCompactEvent, 'preparation' | 'signal'>,
  ctx: Pick<ExtensionContext, 'modelRegistry' | 'sessionManager'>,
  model: Model<Api>,
  instructions: string,
  unusable: (reason: string, userChoice?: boolean) => void = () => undefined,
): Promise<CompactionResult | undefined> {
  try {
    const settings = pi.getSettings();
    // Pi replaces images with a note in a session's requests; this request cannot show them, so it is not made.
    if (settings.images?.blockImages) return void unusable('images are blocked in settings', true);
    const { preparation } = event;
    const projection = ctx.sessionManager.buildSessionProjection();
    // The projection plus the transform Octocode's own `context` handler applies to every real request.
    const messages = requestMessages(projection.messages, model, sessionCleared(ctx.sessionManager));
    const prompt = [SHARED_SUMMARY_PROMPT, boundaryNote(projection.entries, preparation.firstKeptEntryId), preparation.previousSummary === undefined ? '' : PREVIOUS_SUMMARY_NOTE, `Additional focus:\n${instructions}`].filter(Boolean).join('\n\n');
    const cap = summaryCapTokens(model, preparation.settings.reserveTokens);
    const level = pi.getThinkingLevel();
    const options: SimpleStreamOptions = {
      // The session's own thinking level, budgets, transport and routing id: each one changes what the provider caches.
      ...(level === 'off' ? {} : { reasoning: level }),
      ...(settings.thinkingBudgets ? { thinkingBudgets: settings.thinkingBudgets } : {}),
      ...(settings.transport ? { transport: settings.transport } : {}),
      sessionId: ctx.sessionManager.getSessionId(),
      signal: event.signal,
      maxTokens: cap,
    };
    const response = await ctx.modelRegistry.streamSimple(model, { messages: [...convertToLlm(messages), { role: 'user', content: [{ type: 'text', text: prompt }], timestamp: Date.now() }] }, options).result();
    if (response.stopReason !== 'stop') return void unusable(`the reply stopped with ${response.stopReason}`);
    if (response.content.some((block) => block.type === 'toolCall')) return void unusable('the reply called a tool');
    const text = contentText(response.content).trim();
    if (text === '') return void unusable('the reply was empty');
    if (text.length / CHARS_PER_TOKEN > cap) return void unusable('the reply was longer than the cap');
    const lists = fileLists(preparation.fileOps);
    return { summary: `${text}${fileBlocks(lists)}`, firstKeptEntryId: preparation.firstKeptEntryId, tokensBefore: preparation.tokensBefore, usage: response.usage, details: lists };
  } catch (error) {
    // An abort is the user's own cancel: the standalone request is cancelled too, so there is nothing to explain.
    if (!event.signal.aborted) unusable(errorMessage(error));
    return undefined;
  }
}
