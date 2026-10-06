import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { clip, timingOf, resultBlock, timedTool, toolHeader } from '../shared/render.js';
import { Type } from 'typebox';
import { withDialog } from '../shared/locks.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { showAskDialog, type AskQuestion, type AskResult } from './dialog.js';

const OTHER = 'Write an answer';
const DONE = 'Submit selection';

/** RPC hosts have dialogs but no custom TUI components: fall back to select/input per question. */
async function askWithDialogs(ctx: ExtensionContext, questions: AskQuestion[], signal?: AbortSignal): Promise<AskResult> {
  const answers: AskResult['answers'] = [];
  const options = signal ? { signal } : undefined;
  const cancelled = (): AskResult => ({ answers, cancelled: true, ...(signal?.aborted ? { reason: 'aborted' as const } : {}) });
  for (const q of questions) {
    if (signal?.aborted) return cancelled();
    if (q.options.length === 0) {
      const typed = (await ctx.ui.input(sanitizeTerminalText(q.question), undefined, options))?.trim();
      if (signal?.aborted || !typed) return cancelled();
      answers.push({ question: q.question, answer: typed, custom: true });
      continue;
    }
    // Numbered identities distinguish duplicate labels and labels named after UI actions.
    const labels = q.options.map((option, index) => sanitizeTerminalText(`${index + 1}. ${option.label}${option.description ? ` — ${option.description}` : ''}`));
    // Multi-select: pick one option per round until Done (or every option is picked), as the TUI's toggles.
    const chosen = new Set<number>();
    let typed: string | undefined;
    for (;;) {
      const remaining = labels.filter((_, index) => !chosen.has(index));
      const extra = q.multiSelect && chosen.size > 0 ? [DONE, OTHER] : [OTHER];
      const selected = [...chosen].map((index) => q.options[index]!.label).join(', ');
      const title = q.multiSelect ? `${q.question} (choose one or more, then ${DONE}${selected ? `; selected: ${selected}` : ''})` : q.question;
      const picked = await ctx.ui.select(sanitizeTerminalText(title), [...remaining, ...extra], options);
      if (signal?.aborted || picked === undefined) return cancelled();
      if (picked === OTHER) {
        // An empty answer is no answer, as in the TUI dialog.
        typed = (await ctx.ui.input(sanitizeTerminalText(q.question), undefined, options))?.trim();
        if (signal?.aborted || !typed) return cancelled();
        break;
      }
      if (picked === DONE) break;
      const index = labels.indexOf(picked);
      if (index < 0 || chosen.has(index)) return cancelled();
      chosen.add(index);
      if (!q.multiSelect || chosen.size === labels.length) break;
    }
    const picks = q.options.flatMap((option, index) => chosen.has(index) ? [option.label] : []);
    answers.push({ question: q.question, answer: [...picks, ...(typed ? [typed] : [])].join(', '), custom: typed !== undefined, ...(typed !== undefined && picks.length ? { picks, typed } : {}) });
  }
  return { answers, cancelled: false };
}

export function formatAnswers(result: AskResult): string {
  const answered = result.answers.map((entry) => {
    if (!entry.custom) return `"${entry.question}" → ${entry.answer}`;
    // Toggled options plus a write-in: name both parts, since the joined answer cannot be split back.
    return `"${entry.question}" → ${entry.picks?.length ? `${entry.picks.join(', ')} + (typed) ${entry.typed ?? ''}` : `(typed) ${entry.answer}`}`;
  });
  if (!result.cancelled) return answered.join('\n');
  // Answers given before the user cancelled still count.
  const declined = result.reason === 'unavailable'
    ? 'No interactive user is available. No answer or approval was supplied.'
    : result.reason === 'aborted'
      ? 'The question was interrupted. No further answer or approval was supplied.'
      : `The user declined to answer${answered.length > 0 ? ' the remaining questions' : ''}. Unanswered questions do not grant approval.`;
  const recovery = 'Continue work covered by existing authorization; state reversible assumptions. If an answer is required to proceed, report what remains blocked.';
  return [...answered, declined, recovery].join('\n');
}

export function registerAskUser(pi: ExtensionAPI): void {
  pi.registerTool(timedTool({
    name: 'askUser',
    label: 'Ask user',
    description:
      'Ask for missing preferences, requirements or facts that tools cannot discover and that materially change the next action. Use existing answers and authorization; make routine reversible decisions yourself. ' +
      'Ask one focused question or batch up to 4 related ones in a single call instead of asking in sequence. Make each question self-contained. ' +
      'Supply 2-4 distinct choices when the answer space is known; omit options for a free-text answer. Put a supported recommendation first with \' (Recommended)\' in its label. Do not add an "Other" choice: the UI always offers a write-in field. ' +
      'Set multiSelect: true when several choices can apply together (the user toggles any number, then confirms); leave it off when choices are mutually exclusive (a single pick answers the question). ' +
      'Cancellation or an unavailable user supplies no answer or approval; continue independent authorized work and report a required missing decision.',
    promptSnippet: 'Ask for a decision or fact only the user can supply',
    parameters: Type.Object({
      questions: Type.Array(
        Type.Object({
          question: Type.String({ minLength: 1, description: 'Self-contained question with the context needed to answer' }),
          header: Type.Optional(Type.String({ description: 'Short tab label; omit for an automatic question number' })),
          options: Type.Optional(Type.Array(
            Type.Object({
              label: Type.String({ minLength: 1, description: 'Concise answer label (1-5 words)' }),
              description: Type.Optional(Type.String({ description: 'Practical effect or trade-off, when it helps distinguish the choices' })),
            }),
            { maxItems: 4 },
          )),
          multiSelect: Type.Optional(Type.Boolean({ description: 'true: choices are not exclusive; the user may pick several. Default false: exactly one choice' })),
        }),
        { minItems: 1, maxItems: 4 },
      ),
    }),
    async execute(_id, params, signal, _onUpdate, ctx) {
      // multiSelect means nothing without options: a free-text question opens its editor directly.
      const questions = params.questions.map((q) => ({ ...q, options: q.options ?? [], ...(q.header ? { header: clip(q.header, 12) } : {}), ...(q.options?.length ? {} : { multiSelect: false }) }));
      const result: AskResult = signal?.aborted
        ? { answers: [], cancelled: true, reason: 'aborted' }
        : !ctx.hasUI
          ? { answers: [], cancelled: true, reason: 'unavailable' }
          : await withDialog(() => (ctx.mode === 'tui' ? showAskDialog(ctx, questions, signal) : askWithDialogs(ctx, questions, signal)), signal);
      return { content: [{ type: 'text', text: formatAnswers(result) }], details: result };
    },
    renderCall(args, theme, context) {
      const headers = (args.questions ?? []).map((q) => clip(String(q?.header ?? ''), 24)).filter(Boolean).join(', ');
      return toolHeader(theme, context, 'Ask', headers);
    },
    renderResult(result, _options, theme, context) {
      const details = result.details as Partial<AskResult> | undefined;
      const answers = Array.isArray(details?.answers) ? details.answers : [];
      // Every answer is shown, collapsed or not: the user just gave them and there are at most four.
      const lines = answers.map((entry) => `${theme.fg('success', '✓')} ${theme.fg('muted', clip(String(entry.question), 120))} ${theme.fg('dim', '→')} ${clip(String(entry.answer), 200)}`);
      if (details?.cancelled) lines.push(theme.fg('warning', details.reason === 'unavailable' ? 'Unavailable' : details.reason === 'aborted' ? 'Interrupted' : lines.length > 0 ? 'Declined the remaining questions' : 'Declined'));
      const fallback = lines.length === 0 ? clip(result.content.map((part) => (part.type === 'text' ? part.text : '')).join('\n'), 200) : '';
      if (fallback) lines.push(theme.fg('toolOutput', fallback));
      return resultBlock(theme, context, { summary: '', lines, max: lines.length, ...timingOf(result.details) });
    },
  }));
}
