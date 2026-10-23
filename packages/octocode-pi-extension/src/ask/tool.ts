import type { ExtensionAPI, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { clip, plural, resultBlock, toolHeader } from '../shared/render.js';
import { timingOf, timedTool } from '../shared/tool-timing.js';
import { Type } from 'typebox';
import { withDialog } from '../shared/locks.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { answerOf, showAskDialog, SUBMIT_PICKS, TYPE_OWN, type AskQuestion, type AskResult } from './dialog.js';

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
      answers.push(answerOf(q.question, [], typed));
      continue;
    }
    // Numbered identities distinguish duplicate labels and labels named after UI actions.
    const labels = q.options.map((option, index) => sanitizeTerminalText(`${index + 1}. ${option.label}${option.description ? ` — ${option.description}` : ''}`));
    // Multi-select: pick one option per round until Done (or every option is picked), as the TUI's toggles.
    const chosen = new Set<number>();
    let typed: string | undefined;
    for (;;) {
      const remaining = labels.filter((_, index) => !chosen.has(index));
      const extra = q.multiSelect && chosen.size > 0 ? [SUBMIT_PICKS, TYPE_OWN] : [TYPE_OWN];
      const selected = [...chosen].map((index) => q.options[index]!.label).join(', ');
      const title = q.multiSelect ? `${q.question} (choose one or more, then ${SUBMIT_PICKS}${selected ? `; selected: ${selected}` : ''})` : q.question;
      const picked = await ctx.ui.select(sanitizeTerminalText(title), [...remaining, ...extra], options);
      if (signal?.aborted || picked === undefined) return cancelled();
      if (picked === TYPE_OWN) {
        // An empty answer is no answer, as in the TUI dialog.
        typed = (await ctx.ui.input(sanitizeTerminalText(q.question), undefined, options))?.trim();
        if (signal?.aborted || !typed) return cancelled();
        break;
      }
      if (picked === SUBMIT_PICKS) break;
      const index = labels.indexOf(picked);
      if (index < 0 || chosen.has(index)) return cancelled();
      chosen.add(index);
      if (!q.multiSelect || chosen.size === labels.length) break;
    }
    const picks = q.options.flatMap((option, index) => chosen.has(index) ? [option.label] : []);
    answers.push(answerOf(q.question, picks, typed));
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
      'Ask the user for preferences or facts that tools cannot discover and that change your next step. Batch related, self-contained questions in one call. ' +
      'Decision request: 2-4 options when known (none for free text), a supported recommendation first labelled \' (Recommended)\'; the UI always adds a write-in field, so do not add "Other".',
    promptSnippet: 'Ask the user a question only they can answer',
    parameters: Type.Object({
      questions: Type.Array(
        Type.Object({
          question: Type.String({ description: 'Self-contained question with the context needed to answer' }),
          header: Type.Optional(Type.String({ description: 'Short tab label (about 12 characters are shown); omit for an automatic question number' })),
          options: Type.Optional(Type.Array(
            Type.Object({
              label: Type.String({ description: 'Concise answer label' }),
              description: Type.Optional(Type.String({ description: 'Practical effect or trade-off, when it helps distinguish the choices' })),
            }),
          )),
          multiSelect: Type.Optional(Type.Boolean({ description: 'true: choices are not exclusive; the user may pick several. Default false: exactly one choice' })),
        }),
      ),
    }),
    async execute(_id, params, signal, _onUpdate, ctx) {
      // multiSelect means nothing without options: a free-text question opens its editor directly.
      let skippedOptions = 0;
      const questions = params.questions.filter((q) => q.question.trim()).map((q) => {
        const options = (q.options ?? []).filter((option) => option.label.trim());
        skippedOptions += (q.options?.length ?? 0) - options.length;
        return { ...q, options, ...(q.header ? { header: clip(q.header, 12) } : {}), ...(options.length ? {} : { multiSelect: false }) };
      });
      if (questions.length === 0) throw new Error('Ask at least one question with text.');
      const result: AskResult = signal?.aborted
        ? { answers: [], cancelled: true, reason: 'aborted' }
        : !ctx.hasUI
          ? { answers: [], cancelled: true, reason: 'unavailable' }
          : await withDialog(() => (ctx.mode === 'tui' ? showAskDialog(ctx, questions, signal) : askWithDialogs(ctx, questions, signal)), signal);
      // Empty questions and options are not asked: say how many, so a missing answer is not mistaken for a skipped one.
      const skippedQuestions = params.questions.length - questions.length;
      const skipped = [skippedQuestions > 0 && plural(skippedQuestions, 'empty question'), skippedOptions > 0 && plural(skippedOptions, 'option with an empty label', 'options with an empty label')].filter(Boolean);
      const note = skipped.length > 0 ? `\n(Not asked: ${skipped.join(' and ')}.)` : '';
      return { content: [{ type: 'text', text: `${formatAnswers(result)}${note}` }], details: result };
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
