import { truncateToWidth, visibleWidth } from '../tui/width.js';
import { paint } from '../tui/palette.js';
/**
 * askUser — interactive elicitation tool.
 *
 * Lets the agent ask the human a question and get a real answer through the TUI
 * instead of dumping a numbered list in prose and hoping the user types the
 * matching token. Modes are chosen from the arguments and rendered INLINE via
 * ctx.ui.custom(builder) (no overlay options), so the prompt appears in the
 * conversation/message flow at the bottom — reading as part of the message list
 * — rather than as a floating modal box pinned over the conversation:
 *
 *   • options[]  → a keyboard-navigable numbered list (↑↓ / 1-9 quick-select /
 *     enter / esc) with an always-available custom free-text answer; long lists
 *     scroll in a window around the cursor, and `/` filters large lists in place.
 *   • options[] + multiSelect → a checkbox list (space or 1-9 toggles, `a`
 *     all/clear, `i` invert, enter confirms once min/max are satisfied, esc
 *     cancels) with a live selection count in the footer; options may carry a
 *     preview block shown while focused.
 *   • fields[]   → a simple sequential form; required/length/pattern validation
 *     keeps focus on the invalid field instead of silently accepting bad input.
 *   • no options → a single-line text prompt.
 *
 * Non-interactive hosts (rpc / json / print, or any host without overlay
 * support) return a clear instruction telling the agent to ask the question
 * inline in its next message. The tool never uses Pi's built-in select/input
 * surfaces, and it never blocks or fakes an answer.
 */

import { CLI_GLYPH, CLI_STATUS_TEXT, cliSpinnerFrame, cliToolTitle } from '../tui/cli-design.js';
import type { ToolDefinition, ToolCallResult, PiTheme, PiContext, RenderResultOptions } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { makeComponentRenderer } from './render-helpers.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';
import { ASK_HEADER_LABEL } from '../tui/content.js';
import { renderFrame } from '../tui/components.js';
import { CURSOR_MARKER, Input, Key, matchesKey, wrapTextWithAnsi } from '@earendil-works/pi-tui';
import { answerPendingInteraction, createPendingInteraction, shouldBrokerInteraction } from './interaction-broker.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

export interface AskOption {
  value: string;
  label?: string;
  description?: string;
  /** Upsides of this option — rendered as ✓ lines under the focused row. */
  pros?: string[];
  /** Downsides / risks of this option — rendered as ✗ lines under the focused row. */
  cons?: string[];
  /** Marks the recommended default: badges the row and lands the cursor here first. */
  recommended?: boolean;
  /** Optional multi-line preview shown under the option while it is focused (multi-select overlay). */
  preview?: string;
  /** Internal durable-interaction ID; the interactive selection still returns value. */
  brokerId?: string;
  /** Disabled choices are visible but cannot be selected; a string is shown as the reason. */
  disabled?: boolean | string;
  /** Optional group heading for clustering related choices in the list. */
  group?: string;
}

interface AskField {
  name: string;
  label?: string;
  placeholder?: string;
  required?: boolean;
  minLength?: number;
  maxLength?: number;
  pattern?: string;
}

interface AskParams {
  question: string;
  options?: Array<AskOption | string>;
  placeholder?: string;
  multiSelect?: boolean;
  min?: number;
  max?: number;
  fields?: AskField[];
  /** Optional bounded wait; expiry never selects a default. */
  timeoutMs?: number;
}

export interface AskOutcome {
  status: 'selected' | 'text' | 'back' | 'cancelled' | 'timed_out' | 'unavailable' | 'pending' | 'multiSelected' | 'form';
  value?: string;
  label?: string;
  /** multiSelected → string[] of chosen values; form → Record<fieldName, answer>. */
  values?: string[] | Record<string, string>;
  /** Durable broker request when this prompt must be completed by a non-TUI host. */
  interaction?: ReturnType<typeof createPendingInteraction>;
}

function normalizeOptions(raw: AskParams['options']): AskOption[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .map((o) => (typeof o === 'string' ? { value: o } : o))
    .filter((o): o is AskOption => Boolean(o && typeof o.value === 'string' && o.value.length > 0))
    .map((o) => ({
      value: o.value,
      label: o.label || o.value,
      description: o.description,
      pros: cleanBullets(o.pros),
      cons: cleanBullets(o.cons),
      recommended: o.recommended === true,
      preview: o.preview,
      disabled: typeof o.disabled === 'string' && o.disabled.trim() ? o.disabled.trim() : o.disabled === true,
      group: typeof o.group === 'string' && o.group.trim() ? o.group.trim() : undefined,
    }));
}

/** Trim and drop empty entries from a pros/cons bullet list; undefined when nothing remains. */
function cleanBullets(raw: unknown): string[] | undefined {
  if (!Array.isArray(raw)) return undefined;
  const items = raw.map((s) => String(s ?? '').trim()).filter((s) => s.length > 0);
  return items.length > 0 ? items : undefined;
}

function normalizeFields(raw: AskParams['fields']): AskField[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((f): f is AskField => Boolean(f && typeof f.name === 'string' && f.name.length > 0))
    .map((f) => ({
      name: f.name,
      label: f.label,
      placeholder: f.placeholder,
      required: f.required === true,
      minLength: typeof f.minLength === 'number' && Number.isFinite(f.minLength) ? Math.max(0, Math.floor(f.minLength)) : undefined,
      maxLength: typeof f.maxLength === 'number' && Number.isFinite(f.maxLength) ? Math.max(1, Math.floor(f.maxLength)) : undefined,
      pattern: typeof f.pattern === 'string' && f.pattern.length > 0 ? f.pattern : undefined,
    }));
}

function validateUniqueFieldNames(fields: readonly AskField[]): void {
  const seen = new Set<string>();
  for (const field of fields) {
    if (seen.has(field.name)) throw new Error(`field names must be unique; duplicate "${field.name}".`);
    seen.add(field.name);
  }
}

function disabledReason(option: Pick<AskOption, 'disabled'>): string | undefined {
  if (typeof option.disabled === 'string') return option.disabled;
  return option.disabled ? 'disabled' : undefined;
}

function validateFieldValue(field: AskField, raw: string): string | undefined {
  const label = field.label || field.name;
  const value = raw.trim();
  if (field.required && !value) return `${label} is required.`;
  if (field.minLength !== undefined && value.length < field.minLength) return `${label} must be at least ${field.minLength} character${field.minLength === 1 ? '' : 's'}.`;
  if (field.maxLength !== undefined && value.length > field.maxLength) return `${label} must be at most ${field.maxLength} character${field.maxLength === 1 ? '' : 's'}.`;
  if (field.pattern) {
    try {
      if (!new RegExp(field.pattern).test(value)) return `${label} has the wrong format.`;
    } catch {
      return `${label} validation pattern is invalid.`;
    }
  }
  return undefined;
}

function matchesQuery(option: AskOption, query: string): boolean {
  if (!query.trim()) return true;
  const needle = query.trim().toLocaleLowerCase();
  return [option.label ?? option.value, option.value, option.description ?? '']
    .some((part) => part.toLocaleLowerCase().includes(needle));
}
function supportsAskOverlay(ctx?: PiContext): boolean {
  // Guard on mode === 'tui': in RPC mode hasUI is true and custom() exists but
  // RETURNS undefined (per pi docs), which would make askUser silently resolve
  // as cancelled instead of falling back to an inline question. custom() is a
  // TUI-only feature.
  return Boolean(ctx?.mode === 'tui' && ctx?.hasUI && typeof ctx.ui?.custom === 'function');
}

function hasInteractiveUi(ctx?: PiContext): boolean {
  return supportsAskOverlay(ctx);
}

function isCancelKey(data: string): boolean {
  return data === '\x1b' || data === '\x03';
}

function isEnterKey(data: string): boolean {
  return data === '\r' || data === '\n';
}

function isBackspaceKey(data: string): boolean {
  return data === '\x7f' || data === '\b';
}

function isPrintableInput(data: string): boolean {
  if (!data) return false;
  return [...data].every((ch) => {
    const code = ch.charCodeAt(0);
    return code >= 32 && code !== 0x7f && !(code >= 0x80 && code <= 0x9f);
  });
}

/** Keep decision cards readable without floating them in an arbitrary center column. */
const ASK_FRAME_MAX_WIDTH = 80;

function askFrameWidth(width: number): number {
  const available = Math.max(1, Math.floor(width || 80));
  return Math.min(available, ASK_FRAME_MAX_WIDTH);
}

/**
 * Wrap a semantic row inside the frame body, budgeting both rails and its
 * leading space before any option indentation.
 */
function wrapAskPayload(
  text: string,
  firstPrefix: string,
  continuationPrefix: string,
  width: number,
): string[] {
  const payloadWidth = Math.max(
    1,
    askFrameWidth(width) - 3 - Math.max(visibleWidth(firstPrefix), visibleWidth(continuationPrefix)),
  );
  const wrapped = text.split('\n').flatMap((line) => wrapTextWithAnsi(line || ' ', payloadWidth));
  return wrapped.map((line, index) => `${index === 0 ? firstPrefix : continuationPrefix}${line}`);
}

/** The question wraps in full before the option or text-entry body. */
function askHeaderLines(question: string, width: number): string[] {
  return [...wrapTextWithAnsi(question, Math.max(1, askFrameWidth(width) - 3)), ''];
}

function renderAskFrame(body: string[], width: number, theme?: PiTheme, pagination?: { current: number; total: number }, headerLabel = ASK_HEADER_LABEL): string[] {
  const title = pagination ? `${headerLabel} · ${pagination.current} of ${pagination.total} ·` : headerLabel;
  return renderFrame({ title, body }, { width: askFrameWidth(width), theme });
}

function askFooterLines(theme: PiTheme | undefined, help: string, width: number, warning?: string): string[] {
  const footerText = warning ? `⚠ ${warning}` : help;
  return wrapAskPayload(paint(theme, warning ? 'warning' : 'muted', footerText), '', '', width);
}

/** Max option rows painted at once; longer lists scroll in a window around the cursor. */
const ASK_LIST_MAX_VISIBLE = 7;

function renderAskChoiceLines(
  theme: PiTheme | undefined,
  question: string,
  items: Array<{ label: string; description?: string; preview?: string; pros?: string[]; cons?: string[]; recommended?: boolean; freeText?: boolean; disabled?: boolean | string; empty?: boolean; optionIndex?: number; groupHeader?: boolean }>,
  cursor: number,
  selected: Set<number> | undefined,
  help: string,
  width: number,
  warning?: string,
  searchQuery?: string,
  pagination?: { current: number; total: number },
  headerLabel?: string,
): string[] {
  // Scroll window: long lists would overflow the terminal height (pi clips the
  // component), so paint at most ASK_LIST_MAX_VISIBLE rows centered on the
  // cursor with dim "N more" markers for the hidden remainder. Hidden options
  // remain reachable by navigation; the focused option itself is never clipped
  // or capped, so its complete decision context remains readable.
  const focused = items[cursor];
  const focusedDetail = focused
    ? (focused.description ? 1 : 0) +
      (focused.pros?.length ?? 0) +
      (focused.cons?.length ?? 0) +
      (focused.preview ? focused.preview.split('\n').length : 0)
    : 0;
  const visibleRows = Math.max(3, ASK_LIST_MAX_VISIBLE - focusedDetail);
  let start = 0;
  let end = items.length;
  if (items.length > visibleRows) {
    start = Math.min(
      Math.max(0, cursor - Math.floor(visibleRows / 2)),
      items.length - visibleRows,
    );
    end = start + visibleRows;
  }
  const rows = items.slice(start, end).flatMap((item, offset) => {
    const index = start + offset;
    const active = index === cursor;
    const marker = active ? paint(theme, 'brand', '›') : ' ';
    if (item.groupHeader) {
      return wrapAskPayload(
        paint(theme, 'dim', `┌ ${item.label}`),
        ``,
        `  `,
        width,
      );
    }
    const disabled = disabledReason(item);
    // ASCII checkboxes match multi-select-list and keep the columns aligned —
    // ☑/☐ are East-Asian-ambiguous and render 2 cells on some terminals.
    // Brand (not success) color: a checked box is selection state, not an outcome.
    const optionIndex = item.optionIndex ?? index;
    // Only real, selectable option rows get a checkbox — the free-text/empty rows
    // are not toggleable, so a [ ]/[x] there is a lie (and the index fallback could
    // even paint a spurious [x] when a filtered row index collides with a selection).
    const checked = selected && !item.empty && !item.freeText ? (selected.has(optionIndex) ? paint(theme, 'brand', '[x]') : paint(theme, 'dim', '[ ]')) : '';
    // Numbered rows advertise the 1-9 quick-select keys; rows past 9 pad the same
    // 3 cells so the label column doesn't jump. Free-text/empty rows are unnumbered.
    const ordinal = !item.freeText && !item.empty
      ? `${paint(theme, active ? 'brand' : 'dim', `${optionIndex + 1}.`)} `
      : '';
    const rawLabel = disabled || item.empty ? paint(theme, 'muted', item.label) : item.freeText ? paint(theme, active ? 'brand' : 'muted', item.label) : (active ? paint(theme, 'brand', item.label) : item.label);
    // Recommended pill: [recommended] in brand color, always visible regardless
    // of focus so the safe default is scannable at a glance.
    const badge = item.recommended
      ? ` ${paint(theme, 'dim', '[')}${paint(theme, 'brand', 'recommended')}${paint(theme, 'dim', ']')}`
      : '';
    const disabledBadge = disabled ? ` ${paint(theme, 'muted', `(${disabled})`)}` : '';
    const labelPrefix = `${marker} ${checked ? `${checked} ` : ''}${ordinal}`;
    const labelLines = wrapAskPayload(
      `${rawLabel}${badge}${disabledBadge}`,
      labelPrefix,
      `    `,
      width,
    );
    // Expand the FOCUSED row with its trade-offs (pros ✓ / cons ✗) and any
    // preview — collapsed rows stay one line so the list stays scannable.
    const detail: string[] = [];
    if (active) {
      if (item.description) {
        detail.push(...wrapAskPayload(
          paint(theme, 'dim', item.description),
          `    `,
          `    `,
          width,
        ));
      }
      // Pros/cons are descriptive trade-offs, not outcomes — green/red are reserved
      // for real outcomes. The ✓/✗ glyphs carry the polarity; pros read at default
      // fg (prominent) and cons muted (secondary), no status color misused.
      for (const pro of item.pros ?? []) {
        detail.push(...wrapAskPayload(
          paint(theme, 'bright', `✓ ${pro}`),
          `    `,
          `      `,
          width,
        ));
      }
      for (const con of item.cons ?? []) {
        detail.push(...wrapAskPayload(
          paint(theme, 'muted', `✗ ${con}`),
          `    `,
          `      `,
          width,
        ));
      }
      if (item.preview) {
        for (const previewLine of item.preview.split('\n')) {
          detail.push(...wrapAskPayload(
            paint(theme, 'dim', previewLine || ' '),
            `    `,
            `    `,
            width,
          ));
        }
      }
    }
    return [...labelLines, ...detail];
  });
  if (start > 0) rows.unshift(`${paint(theme, 'dim', `↑ ${start} more`)}`);
  if (end < items.length) rows.push(`${paint(theme, 'dim', `↓ ${items.length - end} more`)}`);
  return renderAskFrame([
    ...askHeaderLines(question, width),
    ...rows,
    // Breathing room between the last row and the footer rule.
    '',
    ...(searchQuery !== undefined
      ? wrapAskPayload(
        paint(theme, searchQuery ? 'brand' : 'dim', `/ ${searchQuery || 'type to filter…'}`),
        ``,
        `  `,
        width,
      )
      : []),
    ...askFooterLines(theme, help, width, warning),
  ], width, theme, pagination, headerLabel);
}

function renderAskTextLines(
  theme: PiTheme | undefined,
  question: string,
  inputLine: string,
  isEmpty: boolean,
  placeholder: string | undefined,
  help: string,
  width: number,
  warning?: string,
  headerLabel?: string,
): string[] {
  const inputBody = inputLine.startsWith('> ') ? inputLine.slice(2) : inputLine;
  const label = placeholder ? `Answer · ${placeholder}` : 'Answer';
  return renderAskFrame([
    ...askHeaderLines(question, width),
    ...wrapAskPayload(
      `${paint(theme, 'brand', label)}${isEmpty ? paint(theme, 'dim', ' · paste or type') : ''}`,
      ``,
      ``,
      width,
    ),
    `${paint(theme, 'brand', '›')} ${inputBody}`,
    '',
    ...askFooterLines(theme, help, width, warning),
  ], width, theme, undefined, headerLabel);
}

function renderAskFinalLines(
  theme: PiTheme | undefined,
  question: string,
  outcome: AskOutcome,
  width: number,
  headerLabel?: string,
): string[] {
  const summary = (() => {
    if (outcome.status === 'selected') return `${CLI_GLYPH.success} ${outcome.label ?? outcome.value ?? 'selected'}`;
    if (outcome.status === 'text') return `${CLI_GLYPH.success} ${outcome.value ?? 'answered'}`;
    if (outcome.status === 'multiSelected') {
      const n = Array.isArray(outcome.values) ? outcome.values.length : 0;
      return `${CLI_GLYPH.success} ${n} selected`;
    }
    if (outcome.status === 'form') return `${CLI_GLYPH.success} form submitted`;
    if (outcome.status === 'back') return '← back';
    if (outcome.status === 'cancelled') return `⨯ ${CLI_STATUS_TEXT.cancelled}`;
    if (outcome.status === 'timed_out') return '⏱ timed out';
    return CLI_STATUS_TEXT.unavailable;
  })();
  // success (green) is an OUTCOME color; cancelled/unavailable are neutral, not wins.
  const token = outcome.status === 'back' || outcome.status === 'cancelled' || outcome.status === 'timed_out' || outcome.status === 'unavailable' ? 'muted' : 'success';
  const footer = outcome.status === 'back'
    ? 'back'
    : outcome.status === 'cancelled'
      ? CLI_STATUS_TEXT.cancelled
      : outcome.status === 'timed_out'
        ? 'timed out'
        : outcome.status === 'unavailable'
          ? CLI_STATUS_TEXT.unavailable
          : 'submitted';
  return renderAskFrame([
    ...askHeaderLines(question, width),
    ...wrapAskPayload(paint(theme, token, summary), ``, ``, width),
    ...askFooterLines(theme, footer, width),
  ], width, theme, undefined, headerLabel);
}

/**
 * Programmatic entry to the inline ask flow, for other harness features
 * (e.g. the plan tool's propose/approve card). Same renderer, same keys,
 * same free-text escape hatch as the askUser tool itself.
 */
export async function runAskPrompt(
  ctx: PiContext,
  params: {
    question: string;
    options: AskOption[];
    placeholder?: string;
    /** Context-specific label for the always-present free-text escape row. */
    freeTextLabel?: string;
    /** Overrides the generic 'Input needed' header with a context-specific label. */
    headerLabel?: string;
    /** When set, renders a '· N of T ·' pagination badge in the header. */
    pagination?: { current: number; total: number };
    /** Disable durable brokering for presentation-only choices that can safely fall back. */
    durable?: boolean;
    /** Mark a durable choice as authorization so consumers can validate its provenance. */
    kind?: 'question' | 'authorization';
  },
): Promise<AskOutcome | undefined> {
  const request = params.durable !== false && shouldBrokerInteraction(ctx)
    ? createPendingInteraction(ctx, {
        question: params.question,
        kind: params.kind,
        options: params.options.map((option) => ({
          id: option.brokerId ?? option.value,
          label: option.label ?? option.value,
          ...(option.description ? { description: option.description } : {}),
          ...(option.recommended ? { recommended: true } : {}),
          ...(disabledReason(option) ? { disabledReason: disabledReason(option)! } : {}),
        })),
      })
    : undefined;
  if (!supportsAskOverlay(ctx)) {
    return request ? { status: 'pending', interaction: request } : { status: 'unavailable' };
  }
  const outcome = await runAskOverlay(ctx, params);
  if (request && outcome && outcome.status !== 'timed_out') answerPendingInteraction(request, outcome);
  return outcome;
}

async function runAskOverlay(
  ctx: PiContext,
  params: {
    question: string;
    options: AskOption[];
    placeholder?: string;
    multiSelect?: boolean;
    min?: number;
    max?: number;
    fields?: AskField[];
    freeTextLabel?: string;
    /** Overrides the generic 'Input needed' header with a context-specific label. */
    headerLabel?: string;
    pagination?: { current: number; total: number };
    timeoutMs?: number;
  },
): Promise<AskOutcome | undefined> {
  if (!supportsAskOverlay(ctx)) return undefined;

  const options = params.options.map((o) => ({ ...o, label: o.label ?? o.value }));
  const fields = params.fields ?? [];

  // Render INLINE (non-overlay ctx.ui.custom) so the prompt appears in the
  // conversation/message flow at the bottom — reading as part of the message
  // list — instead of a floating modal box pinned over the conversation. When
  // the prompt resolves it disappears and the tool result (renderResult) is what
  // remains in the scrollback. The free-text "type my own answer" row is always
  // appended AFTER the listed options so the user can redirect instead of being
  // boxed into the choices.
  return ctx.ui!.custom!<AskOutcome>(
    (tuiRaw: unknown, theme: PiTheme, _kb: unknown, done: (o: AskOutcome) => void) => {
      const tui = tuiRaw as { requestRender?: () => void };
      let finished = false;
      // Land the cursor on the recommended option (if any) so the safe default
      // is preselected and one Enter accepts it. Positioned in ROW space below,
      // once choiceRows() exists — group headers make option index ≠ row index.
      let cursor = 0;
      const textInput = new Input();
      let fieldIndex = 0;
      let warning: string | undefined;
      let searchMode = false;
      let searchQuery = '';
      let finalOutcome: AskOutcome | undefined;
      const selected = new Set<number>();
      const formValues: Record<string, string> = {};
      let mode: 'single' | 'multi' | 'text' | 'form' = fields.length
        ? 'form'
        : options.length
          ? params.multiSelect
            ? 'multi'
            : 'single'
          : 'text';

      let timeout: ReturnType<typeof setTimeout> | undefined;
      const finish = (outcome: AskOutcome): void => {
        if (finished) return;
        if (timeout) clearTimeout(timeout);
        finalOutcome = outcome;
        finished = true;
        rerender();
        done(outcome);
      };
      const rerender = (): void => tui?.requestRender?.();
      const submitText = (value: string): void => {
        if (mode === 'form') {
          const field = fields[fieldIndex]!;
          const validation = validateFieldValue(field, value);
          if (validation) {
            warning = validation;
            rerender();
            return;
          }
          formValues[field.name] = value;
          fieldIndex += 1;
          textInput.setValue('');
          warning = undefined;
          if (fieldIndex >= fields.length) finish({ status: 'form', values: formValues });
          else rerender();
          return;
        }
        finish({ status: 'text', value });
      };
      textInput.onSubmit = submitText;
      textInput.onEscape = () => {
        if (mode === 'text' && options.length > 0) {
          mode = params.multiSelect ? 'multi' : 'single';
          textInput.setValue('');
          warning = undefined;
          rerender();
          return;
        }
        finish({ status: 'cancelled' });
      };
      const renderTextPrompt = (
        question: string,
        placeholder: string | undefined,
        help: string,
        width: number,
      ): string[] => {
        const inputWidth = Math.max(1, askFrameWidth(width) - 4);
        const inputLine = textInput.render(inputWidth)[0] ?? '';
        return renderAskTextLines(
          theme,
          question,
          inputLine,
          textInput.getValue().length === 0,
          placeholder,
          help,
          width,
          warning,
          params.headerLabel,
        );
      };

      type ChoiceRow = { label: string; description?: string; preview?: string; pros?: string[]; cons?: string[]; recommended?: boolean; freeText?: boolean; disabled?: boolean | string; empty?: boolean; optionIndex?: number; group?: string; groupHeader?: boolean };
      const optionRows = (): ChoiceRow[] => {
        const rows: ChoiceRow[] = [];
        let lastGroup: string | undefined;
        for (const [optionIndex, option] of options.entries()) {
          if (!matchesQuery(option, searchQuery)) continue;
          if (option.group && option.group !== lastGroup) {
            rows.push({ label: option.group, groupHeader: true });
            lastGroup = option.group;
          }
          rows.push({ label: option.label!, description: option.description, preview: option.preview, pros: option.pros, cons: option.cons, recommended: option.recommended, disabled: option.disabled, optionIndex, group: option.group });
        }
        return rows;
      };
      const choiceRows = (): ChoiceRow[] => {
        const visible = optionRows();
        const rows = visible.length ? visible : [{ label: 'No matches', description: 'press esc to clear search', empty: true }];
        const ftLabel = params.freeTextLabel
          ? `✎ ${params.freeTextLabel}`
          : '✎ Discuss or type your own answer…';
        rows.push({ label: ftLabel, description: 'ask a question or reply in your own words', freeText: true });
        return rows;
      };
      // Now that choiceRows() exists, place the cursor on the recommended option's
      // ROW (skipping any inserted group-header rows).
      {
        const recIdx = options.findIndex((o) => o.recommended);
        if (recIdx >= 0) {
          const rowIdx = choiceRows().findIndex((r) => r.optionIndex === recIdx);
          if (rowIdx >= 0) cursor = rowIdx;
        }
      }
      const activeOptionIndex = (): number | undefined => choiceRows()[cursor]?.optionIndex;
      const clampCursor = (): void => {
        const rows = choiceRows();
        cursor = Math.min(Math.max(0, cursor), Math.max(0, rows.length - 1));
        if (rows[cursor]?.groupHeader) cursor = Math.min(cursor + 1, Math.max(0, rows.length - 1));
      };
      const selectableOptionIndexes = (): number[] => options
        .map((option, index) => ({ option, index }))
        .filter(({ option }) => !disabledReason(option))
        .map(({ index }) => index);
      const setSearch = (next: string): void => {
        searchMode = true;
        searchQuery = next;
        cursor = 0;
        warning = undefined;
        rerender();
      };
      const disabledWarning = (index: number): string => {
        const reason = disabledReason(options[index]!);
        return reason ? `"${options[index]!.label}" is ${reason}.` : `"${options[index]!.label}" cannot be selected.`;
      };
      const toggleOption = (index: number): void => {
        const option = options[index];
        if (!option) return;
        const reason = disabledReason(option);
        if (reason) { warning = disabledWarning(index); return; }
        if (selected.has(index)) selected.delete(index);
        else if (params.max === undefined || selected.size < params.max) selected.add(index);
        else warning = `Choose at most ${params.max} option${params.max === 1 ? '' : 's'}.`;
      };

      const render = (width: number): string[] => {
        const w = width > 0 ? width : 80;
        if (finalOutcome) return renderAskFinalLines(theme, params.question, finalOutcome, w, params.headerLabel);
        if (mode === 'text') {
          const help = askFrameWidth(w) < 56
            ? 'enter submit • esc cancel'
            : 'enter submit • esc cancel • paste supported';
          return renderTextPrompt(params.question, params.placeholder, help, w);
        }
        if (mode === 'form') {
          const field = fields[fieldIndex]!;
          const label = field.label || field.name;
          const step = `${fieldIndex + 1}/${fields.length}`;
          const help = askFrameWidth(w) < 56
            ? 'enter next • esc cancel'
            : 'enter next • esc cancel • paste supported';
          return renderTextPrompt(`${params.question} — ${label} (${step})`, field.placeholder, help, w);
        }
        const rows = choiceRows();
        clampCursor();
        const multiCount = mode === 'multi'
          ? `${selected.size} selected${params.min ? ` · min ${params.min}` : ''}${params.max ? ` · max ${params.max}` : ''} • `
          : '';
        // Narrow terminals can't fit the full hint; the footer would hard-truncate
        // and drop enter/esc — the keys the user most needs. Use a compact hint
        // that keeps the essential keys visible below the card's frame cap.
        const narrow = askFrameWidth(w) < 56;
        const help = mode === 'multi'
          ? narrow
            ? `${multiCount}↑↓ • space • a/i • enter ✓ • esc`
            : `${multiCount}↑↓ navigate • / filter • space toggle • a all • i invert • enter confirm • esc cancel`
          : narrow
            ? '← back • ↑↓ • enter • esc'
            : '← back • ↑↓ navigate • / filter • 1-9 select • enter select • esc cancel';
        return renderAskChoiceLines(
          theme,
          params.question,
          rows,
          cursor,
          mode === 'multi' ? selected : undefined,
          help,
          w,
          warning,
          searchMode ? searchQuery : undefined,
          params.pagination,
          params.headerLabel,
        );
      };

      const move = (delta: number): void => {
        const rows = choiceRows();
        const count = Math.max(1, rows.length);
        const step = delta >= 0 ? 1 : -1;
        let next = (cursor + delta + count) % count;
        // Skip non-selectable group-header rows in the direction of travel. Doing
        // it here (not only in clampCursor, which always bumps DOWN) is what lets
        // Up-arrow cross a group boundary instead of getting shoved back down.
        for (let guard = 0; guard < count && rows[next]?.groupHeader; guard++) {
          next = (next + step + count) % count;
        }
        cursor = next;
        warning = undefined;
        rerender();
      };

      const handle = (data: string): void => {
        if (finished) return;
        if (mode === 'text' || mode === 'form') {
          const before = textInput.getValue();
          textInput.handleInput(data);
          if (textInput.getValue() !== before) warning = undefined;
          rerender();
          return;
        }
        if (searchMode && (mode === 'single' || mode === 'multi')) {
          if (isCancelKey(data)) { searchMode = false; searchQuery = ''; cursor = 0; warning = undefined; rerender(); return; }
          if (isBackspaceKey(data)) { setSearch(searchQuery.slice(0, -1)); return; }
          if (isPrintableInput(data) && data !== ' ') { setSearch(searchQuery + data); return; }
        }
        if (isCancelKey(data)) { finish({ status: 'cancelled' }); return; }

        if (mode === 'single' || mode === 'multi') {
          if (matchesKey(data, Key.left)) { finish({ status: 'back' }); return; }
          if (matchesKey(data, Key.up) || matchesKey(data, Key.ctrl('p'))) { move(-1); return; }
          if (matchesKey(data, Key.down) || matchesKey(data, Key.ctrl('n'))) { move(1); return; }
          if (data === '/') { setSearch(''); return; }
          // Digit quick keys mirror the numbered rows: single-select picks the
          // option outright; multi-select toggles it (enter still confirms).
          if (/^[1-9]$/.test(data)) {
            const index = Number(data) - 1;
            if (index < options.length) {
              warning = undefined;
              const reason = disabledReason(options[index]!);
              if (reason) { warning = disabledWarning(index); rerender(); return; }
              const rowIndex = choiceRows().findIndex((row) => row.optionIndex === index);
              if (rowIndex >= 0) cursor = rowIndex;
              if (mode === 'single') {
                const picked = options[index]!;
                finish({ status: 'selected', value: picked.value, label: picked.label ?? picked.value });
                return;
              }
              toggleOption(index);
              rerender();
            }
            return;
          }
          if (mode === 'multi' && data.toLocaleLowerCase() === 'a') {
            const selectable = selectableOptionIndexes();
            const shouldSelectAll = selectable.some((index) => !selected.has(index));
            if (shouldSelectAll && params.max !== undefined && selectable.length > params.max) warning = `Choose at most ${params.max} option${params.max === 1 ? '' : 's'}.`;
            else {
              selected.clear();
              if (shouldSelectAll) for (const index of selectable) selected.add(index);
              warning = undefined;
            }
            rerender();
            return;
          }
          if (mode === 'multi' && data.toLocaleLowerCase() === 'i') {
            const inverted = selectableOptionIndexes().filter((index) => !selected.has(index));
            if (params.max !== undefined && inverted.length > params.max) warning = `Choose at most ${params.max} option${params.max === 1 ? '' : 's'}.`;
            else {
              selected.clear();
              for (const index of inverted) selected.add(index);
              warning = undefined;
            }
            rerender();
            return;
          }
          if (mode === 'multi' && data === ' ') {
            const row = choiceRows()[cursor];
            if (row?.freeText) { mode = 'text'; textInput.setValue(''); warning = undefined; rerender(); return; }
            if (row?.groupHeader) { move(1); return; }
            if (row?.empty) { warning = 'No matching option to toggle.'; rerender(); return; }
            const index = activeOptionIndex();
            if (index !== undefined) toggleOption(index);
            rerender();
            return;
          }
          if (isEnterKey(data)) {
            const row = choiceRows()[cursor];
            if (row?.freeText) { mode = 'text'; textInput.setValue(''); warning = undefined; rerender(); return; }
            if (row?.groupHeader) { move(1); return; }
            if (row?.empty) { warning = 'No matching option to select.'; rerender(); return; }
            const index = activeOptionIndex();
            if (index !== undefined && disabledReason(options[index]!)) { warning = disabledWarning(index); rerender(); return; }
            if (mode === 'multi') {
              const min = params.min ?? 0;
              if (selected.size < min) {
                warning = `Choose at least ${min} option${min === 1 ? '' : 's'}.`;
                rerender();
                return;
              }
              finish({ status: 'multiSelected', values: [...selected].sort((a, b) => a - b).map((i) => options[i]!.value) });
              return;
            }
            const picked = index !== undefined ? options[index] : undefined;
            finish({ status: 'selected', value: picked?.value, label: picked?.label ?? picked?.value });
            return;
          }
          return;
        }

      };

      // The container owns focus, so propagate it into Pi's Input. Input places
      // CURSOR_MARKER at the real editing caret and handles bracketed paste,
      // grapheme-aware movement/deletion, undo, and horizontal scrolling.
      let componentFocused = false;
      const comp: {
        focused: boolean;
        render: (w: number) => string[];
        invalidate: () => void;
        handleInput: (data: string) => void;
      } = {
        get focused(): boolean { return componentFocused; },
        set focused(value: boolean) {
          componentFocused = value;
          textInput.focused = value;
        },
        render: (w: number) => render(w).map((line) =>
          line.includes(CURSOR_MARKER) ? line : truncateToWidth(line, w)),
        invalidate: () => textInput.invalidate(),
        handleInput: (data: string) => handle(data),
      };
      if (params.timeoutMs !== undefined) {
        timeout = setTimeout(() => finish({ status: 'timed_out' }), params.timeoutMs);
        timeout.unref?.();
      }
      return comp;
    },
    // No overlay options: a non-overlay component renders inline in the message
    // flow (at the bottom of the conversation, where input normally lives) and
    // automatically owns input focus while active, so the prompt appears in the
    // message list rather than as a floating modal. pi sets `comp.focused` for
    // the active inline component, which drives the IME cursor marker.
  );
}

export function registerAskUserTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'askUser',
    label: 'Ask user',
    description: [
      'Collect one genuine, decision-changing human answer through the terminal UI. Research first; do not use this for facts available from code, routine confirmation, or normal conversation.',
      'Use options[] for mutually exclusive choices. Keep labels short, add one non-duplicative description only when needed, and mark the safest sensible default recommended:true. A discussion/free-text escape is always available.',
      'Use multiSelect only when several choices can be true; use fields[] for a few related short answers. Disabled choices remain visible with their reason.',
      'Back and cancellation never authorize a default. Non-interactive hosts return a durable pending interaction so the question can be asked inline.',
    ].join('\n'),
    promptSnippet: 'Ask the user a question via an interactive list picker or text input (real UI, not prose)',
    promptGuidelines: [
      'Ask only when the answer changes scope, architecture, acceptance, authorization, or an irreducible preference. Prefer one question.',
      'Use short mutually exclusive labels; add descriptions or pros/cons only for distinct information, never repetition. Mark the safest sensible default recommended:true.',
      'Treat the discussion row as conversation: answer or revise the choices instead of forcing a selection.',
      'Back, cancel, and timeout do not authorize a default. If interactive UI is unavailable, ask inline.',
      'Use multiSelect for genuinely independent selections and fields[] for related formatted answers; constrain counts or validation only when required.',
    ],
    parameters: buildQueryEnvelopeSchema(Type, Type.Object({
      question: Type.String({ description: 'The question to show the user. Keep it one clear sentence.' }),
      options: Type.Optional(
        Type.Array(
          Type.Object({
            value: Type.String({ description: 'Value returned to you when this option is chosen.' }),
            label: Type.Optional(Type.String({ description: 'Short display label (defaults to value).' })),
            description: Type.Optional(Type.String({ description: 'Optional one-line nuance shown only for the focused option; omit when the label is enough.' })),
            pros: Type.Optional(Type.Array(Type.String(), { description: 'Upsides of this option — short bullets shown as ✓ lines under the focused row.' })),
            cons: Type.Optional(Type.Array(Type.String(), { description: 'Downsides/risks of this option — short bullets shown as ✗ lines under the focused row.' })),
            recommended: Type.Optional(Type.Boolean({ description: 'Mark the safe/recommended default: badges the row and lands the cursor here first.' })),
            preview: Type.Optional(Type.String({ description: 'Optional multi-line preview shown under the option while it is focused (multi-select overlay).' })),
            disabled: Type.Optional(Type.Union([
              Type.Boolean({ description: 'true makes this option visible but not selectable.' }),
              Type.String({ description: 'Reason shown next to a visible but non-selectable option.' }),
            ], { description: 'Visible but non-selectable option, optionally with a reason.' })),
            group: Type.Optional(Type.String({ description: 'Optional group heading used to cluster related choices in the list.' })),
          }),
          { description: 'Options for a list picker. Omit for a free-text prompt.' },
        ),
      ),
      placeholder: Type.Optional(Type.String({ description: 'Placeholder for the free-text input.' })),
      multiSelect: Type.Optional(
        Type.Boolean({ description: 'With options[]: let the user toggle several options (space), all/clear with a, invert with i, and confirm (enter). Returns the chosen values[].' }),
      ),
      min: Type.Optional(Type.Integer({ minimum: 0, description: 'Multi-select only: minimum number of selections required to confirm.' })),
      max: Type.Optional(Type.Integer({ minimum: 1, description: 'Multi-select only: maximum number of selections allowed.' })),
      fields: Type.Optional(
        Type.Array(
          Type.Object({
            name: Type.String({ description: 'Key for this answer in the returned values object.' }),
            label: Type.Optional(Type.String({ description: 'Prompt label shown to the user (defaults to name).' })),
            placeholder: Type.Optional(Type.String({ description: 'Placeholder for this field input.' })),
            required: Type.Optional(Type.Boolean({ description: 'Keep focus on this field until a non-empty value is provided.' })),
            minLength: Type.Optional(Type.Integer({ minimum: 0, description: 'Minimum trimmed character count for this field.' })),
            maxLength: Type.Optional(Type.Integer({ minimum: 1, description: 'Maximum trimmed character count for this field.' })),
            pattern: Type.Optional(Type.String({ description: 'JavaScript regular expression the trimmed field value must match.' })),
          }),
          { description: 'Simple sequential form: one text input per field, answers returned keyed by name. Takes precedence over options[].' },
        ),
      ),
      timeoutMs: Type.Optional(Type.Integer({ minimum: 1, maximum: 86_400_000, description: 'Interactive wait limit in milliseconds. Expiry returns timed_out and never selects a default.' })),
    }, { additionalProperties: false }), {
      reasoningDescription: 'Concise reason this question is necessary to decide the next action.',
    }),

    async execute(id: string, raw: Record<string, unknown>, signal, onUpdate, ctx?: PiContext): Promise<ToolCallResult> {
      const queries = Array.isArray(raw.queries)
        ? raw.queries as Record<string, unknown>[]
        : [];
      const runQuery = async (query: Record<string, unknown>): Promise<ToolCallResult> => {
      const p = query as unknown as AskParams;
      const question = String(p.question ?? '').trim();
      if (!question) {
        return { content: [{ type: 'text', text: '[askUser] error: question is required.' }], isError: true };
      }
      const options = normalizeOptions(p.options);
      const fields = normalizeFields(p.fields);
      validateUniqueFieldNames(fields);

      if (!hasInteractiveUi(ctx)) {
        const mode = ctx?.mode ?? 'unknown';
        const listHint = options.length
          ? ` Present these options inline and ask them to choose: ${options.map((o) => o.label).join(', ')}.`
          : '';
        const multiHint = p.multiSelect && options.length ? ' The user may choose more than one.' : '';
        const fieldHint = fields.length
          ? ` Collect these fields inline: ${fields.map((f) => f.label || f.name).join(', ')}.`
          : '';
        const interaction = shouldBrokerInteraction(ctx) ? createPendingInteraction(ctx, {
          question,
          options: options.map((option) => ({
            id: option.value,
            label: option.label ?? option.value,
            ...(option.description ? { description: option.description } : {}),
            ...(option.recommended ? { recommended: true } : {}),
            ...(disabledReason(option) ? { disabledReason: disabledReason(option)! } : {}),
          })),
          ...(p.timeoutMs !== undefined ? { expiresInMs: p.timeoutMs } : {}),
        }) : undefined;
        return interaction ? {
          content: [{
            type: 'text',
            text: `[askUser] Structured interaction pending (mode=${mode}, correlation=${interaction.correlationId}). The host must submit one matching answer through the InteractionBroker adapter, then drain its durable continuation; do not infer a default.${listHint}${multiHint}${fieldHint}`,
          }],
          details: {
            status: 'pending',
            mode,
            interaction,
            continuation: { version: 1, adapter: 'interaction-broker', resumeOn: ['answer', 'session_start'] },
          },
        } as unknown as ToolCallResult : {
          content: [{
            type: 'text',
            text: `[askUser] Input prompt unavailable on this host: no durable InteractionBroker answer route is registered. Ask the user inline instead; do not infer a default.${listHint}${multiHint}${fieldHint}`,
          }],
          details: { status: 'unavailable', mode, reason: 'interaction-answer-route-unavailable' },
        } as unknown as ToolCallResult;
      }

      let outcome: AskOutcome;
      try {
        const interaction = shouldBrokerInteraction(ctx) ? createPendingInteraction(ctx, {
          question,
          options: options.map((option) => ({
            id: option.value,
            label: option.label ?? option.value,
            ...(option.description ? { description: option.description } : {}),
            ...(option.recommended ? { recommended: true } : {}),
            ...(disabledReason(option) ? { disabledReason: disabledReason(option)! } : {}),
          })),
          ...(p.timeoutMs !== undefined ? { expiresInMs: p.timeoutMs } : {}),
        }) : undefined;
        outcome = (await runAskOverlay(ctx!, {
          question,
          options,
          placeholder: p.placeholder,
          multiSelect: p.multiSelect,
          min: p.min,
          max: p.max,
          fields,
          timeoutMs: p.timeoutMs,
        })) ?? { status: 'cancelled' };
        if (interaction && outcome.status !== 'timed_out') answerPendingInteraction(interaction, outcome);
      } catch (err) {
        return {
          content: [{ type: 'text', text: `[askUser] UI error: ${err instanceof Error ? err.message : String(err)}. Ask the user inline instead.` }],
          isError: true,
        };
      }

      // Every outcome echoes the question: the tool result is what survives in
      // the transcript (and compaction summaries), and an answer without its
      // question is meaningless there.
      switch (outcome.status) {
        case 'selected':
          return {
            content: [{ type: 'text', text: `Question: ${question}\nUser selected: ${outcome.label}\nvalue: ${outcome.value}` }],
            details: outcome,
          } as unknown as ToolCallResult;
        case 'text':
          return {
            content: [{ type: 'text', text: `Question: ${question}\nUser answered: ${outcome.value}` }],
            details: outcome,
          } as unknown as ToolCallResult;
        case 'multiSelected': {
          const values = Array.isArray(outcome.values) ? outcome.values : [];
          const labels = values.map((v) => options.find((o) => o.value === v)?.label ?? v);
          return {
            content: [{
              type: 'text',
              text: `Question: ${question}\nUser selected ${values.length} option${values.length === 1 ? '' : 's'}: ${labels.join(', ') || '(none)'}\nvalues: ${JSON.stringify(values)}`,
            }],
            details: outcome,
          } as unknown as ToolCallResult;
        }
        case 'form': {
          const record = (outcome.values ?? {}) as Record<string, string>;
          const body = fields.map((f) => `${f.name}: ${record[f.name] ?? ''}`).join('\n');
          return {
            content: [{ type: 'text', text: `Question: ${question}\nUser provided:\n${body}` }],
            details: outcome,
          } as unknown as ToolCallResult;
        }
        case 'back':
          return {
            content: [{ type: 'text', text: `[askUser] User chose Back from: "${question}". Return to the previous decision step; do not infer an answer for this question.` }],
            details: outcome,
          } as unknown as ToolCallResult;
        case 'timed_out':
          return {
            content: [{ type: 'text', text: `[askUser] Timed out waiting for the user to answer: "${question}". No default was selected; ask again or continue only if the answer is optional.` }],
            details: outcome,
          } as unknown as ToolCallResult;
        case 'cancelled': {
          const requiredNote = outcome.label ? ` Required field "${outcome.label}" was left empty.` : '';
          return {
            content: [{ type: 'text', text: `[askUser] User cancelled (esc) the question: "${question}".${requiredNote} Proceed without a forced choice or ask again if the answer is essential.` }],
            details: outcome,
          } as unknown as ToolCallResult;
        }
        default:
          return {
            content: [{ type: 'text', text: '[askUser] Input prompt unavailable on this host. Ask the user inline instead.' }],
            details: outcome,
          } as unknown as ToolCallResult;
      }
      };

      if (queries.length === 1) {
        const query = queries[0]!;
        const reasoning = typeof query['reasoning'] === 'string' ? query['reasoning'].trim() : '';
        if (!reasoning) throw new Error('queries[0] requires non-empty reasoning.');
        if (reasoning.length > 240) throw new Error('queries[0].reasoning must be at most 240 characters.');
        return runQuery(query);
      }

      return executeQueryBatch({
        toolCallId: id,
        raw,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        ctx,
        preflight(query) {
          if (!String(query['question'] ?? '').trim()) throw new Error('question is required.');
          validateUniqueFieldNames(normalizeFields((query as unknown as AskParams).fields));
        },
        execute: runQuery,
      });
    },

    renderCall(raw: unknown, theme?: PiTheme) {
      const envelope = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
      const queries = Array.isArray(envelope['queries']) ? envelope['queries'] as AskParams[] : [];
      const p = queries[0] ?? {} as AskParams;
      const q = String(p?.question ?? 'ask');
      const count = Array.isArray(p?.options) ? p.options.length : 0;
      const fieldCount = Array.isArray(p?.fields) ? p.fields.length : 0;
      const suffix = fieldCount > 0
        ? ` (form: ${fieldCount} field${fieldCount === 1 ? '' : 's'})`
        : count > 0
          ? p?.multiSelect
            ? ` (${count} options, multi)`
            : ` (${count} options)`
          : ' (free text)';
      const title = cliToolTitle(theme, 'askUser');
      const body = paint(theme, 'dim', q + suffix);
      return makeComponentRenderer((_props, { width: w }) => wrapTextWithAnsi(`${title} ${body}`, Math.max(1, w)), undefined);
    },

    renderResult(result: ToolCallResult, opts: RenderResultOptions, theme?: PiTheme) {
      // Partial: spinner while the interactive overlay is open.
      if (opts.isPartial) {
        const title = cliToolTitle(theme, 'askUser');
        return makeComponentRenderer((_props, { width: w }) => [
          truncateToWidth(
            `${paint(theme, 'brand', cliSpinnerFrame())} ${title} ${paint(theme, 'dim', CLI_STATUS_TEXT.running)}`,
            w,
          ),
        ], undefined);
      }
      const d = (result.details ?? {}) as AskOutcome;
      let line: string;
      if (d.status === 'selected')      line = paint(theme, 'success', `${CLI_GLYPH.success} ${d.label}`);
      else if (d.status === 'text')     line = paint(theme, 'success', `${CLI_GLYPH.success} ${d.value}`);
      else if (d.status === 'multiSelected') {
        const n = Array.isArray(d.values) ? d.values.length : 0;
        line = paint(theme, 'success', `${CLI_GLYPH.success} ${n} selected`);
      } else if (d.status === 'form') {
        const n = d.values && !Array.isArray(d.values) ? Object.keys(d.values).length : 0;
        line = paint(theme, 'success', `${CLI_GLYPH.success} form submitted (${n} field${n === 1 ? '' : 's'})`);
      } else if (d.status === 'back') {
        line = paint(theme, 'muted', '← back');
      } else if (d.status === 'cancelled') {
        line = paint(theme, 'muted', `⨯ ${CLI_STATUS_TEXT.cancelled}`);
      } else if (d.status === 'timed_out') {
        line = paint(theme, 'muted', '⏱ timed out');
      } else {
        line = paint(theme, 'dim', CLI_STATUS_TEXT.unavailable);
      }
      return makeComponentRenderer((_props, { width: w }) => wrapTextWithAnsi(line, Math.max(1, w)), undefined);
    },
  });
}
