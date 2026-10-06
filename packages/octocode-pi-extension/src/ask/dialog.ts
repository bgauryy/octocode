import type { ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Editor, type EditorTheme, Key, matchesKey, truncateToWidth, visibleWidth, wrapTextWithAnsi } from '@earendil-works/pi-tui';
import { sanitizeTerminalText } from '../shared/sanitize.js';

interface AskOption {
  label: string;
  description?: string;
}

export interface AskQuestion {
  question: string;
  header?: string;
  options: AskOption[];
  multiSelect?: boolean;
}

interface AskAnswer {
  question: string;
  answer: string;
  custom: boolean;
  /** Multi-select with a write-in: the toggled option labels, kept apart from the typed text so the model can tell them apart. */
  picks?: string[];
  typed?: string;
}

export interface AskResult {
  answers: AskAnswer[];
  cancelled: boolean;
  reason?: 'unavailable' | 'aborted';
}

const TYPE_OWN = 'Write an answer';
const SUBMIT_PICKS = 'Submit selection';

type DialogTheme = Parameters<Parameters<ExtensionContext['ui']['custom']>[0]>[1];

/**
 * Inline question dialog (adapted from Pi's questionnaire example): a tab per
 * question plus Submit, ↑↓ to move and a free-text row on every question.
 * Single-choice questions answer on Enter; multi-select questions toggle with
 * Space or Enter and answer only from their own "Submit selection" row.
 */
export function showAskDialog(ctx: ExtensionContext, questions: AskQuestion[], signal?: AbortSignal): Promise<AskResult> {
  return ctx.ui.custom<AskResult>((tui, theme, _keybindings, done) => new AskDialog(questions, theme, new Editor(tui, editorTheme(theme)), () => tui.requestRender(), done, signal));
}

function editorTheme(theme: DialogTheme): EditorTheme {
  return {
    borderColor: (text) => theme.fg('accent', text),
    selectList: {
      selectedPrefix: (text) => theme.fg('accent', text),
      selectedText: (text) => theme.fg('accent', text),
      description: (text) => theme.fg('muted', text),
      scrollInfo: (text) => theme.fg('dim', text),
      noMatch: (text) => theme.fg('warning', text),
    },
  };
}

/** Option rows, then the free-text row, then (multi-select only) the row that confirms the toggled set. */
const rows = (q: AskQuestion) => [...q.options.map((option) => option.label), TYPE_OWN, ...(q.multiSelect ? [SUBMIT_PICKS] : [])];

/** The dialog's state and keys; `tab === questions.length` is the Submit tab. */
class AskDialog {
  private readonly isMulti: boolean;
  private readonly answers = new Map<number, AskAnswer>();
  private readonly toggled = new Map<number, Set<number>>();
  private readonly drafts = new Map<number, string>();
  private tab = 0;
  private cursor = 0;
  private typing = false;
  private finished = false;
  private warning = '';
  private cache: string[] | undefined;
  private cacheWidth = 0;
  private readonly onAbort = () => this.finish(true);

  constructor(
    private readonly questions: AskQuestion[],
    private readonly theme: DialogTheme,
    private readonly editor: Editor,
    private readonly requestRender: () => void,
    private readonly done: (result: AskResult) => void,
    private readonly signal?: AbortSignal,
  ) {
    this.isMulti = questions.length > 1;
    this.typing = questions[0]?.options.length === 0;
    editor.onSubmit = (value) => this.submitTyped(value);
    // The run was aborted (e.g. the user interrupted the agent) while the dialog was open.
    if (signal?.aborted) queueMicrotask(this.onAbort);
    else signal?.addEventListener('abort', this.onAbort, { once: true });
  }

  // Focusable: pass focus through to the editor so it emits the cursor marker (hardware cursor, IME placement).
  get focused(): boolean {
    return this.editor.focused;
  }

  set focused(value: boolean) {
    this.editor.focused = value;
  }

  invalidate(): void {
    this.cache = undefined;
    this.editor.invalidate();
  }

  dispose(): void {
    this.signal?.removeEventListener('abort', this.onAbort);
  }

  private refresh(): void {
    this.cache = undefined;
    this.requestRender();
  }

  private finish(cancelled: boolean): void {
    if (this.finished) return;
    this.finished = true;
    this.dispose();
    this.done({ answers: this.questions.flatMap((_, index) => this.answers.get(index) ?? []), cancelled, ...(this.signal?.aborted ? { reason: 'aborted' as const } : {}) });
  }

  private answer(index: number, value: AskAnswer): void {
    this.answers.set(index, value);
    if (!this.isMulti) return this.finish(false);
    this.tab = index < this.questions.length - 1 ? index + 1 : this.questions.length;
    this.cursor = 0;
    this.typing = this.questions[this.tab]?.options.length === 0;
    this.editor.setText(this.drafts.get(this.tab) ?? '');
    this.refresh();
  }

  private submitTyped(value: string): void {
    const q = this.questions[this.tab]!;
    const text = value.trim();
    this.typing = false;
    this.editor.setText('');
    if (!text) return this.refresh();
    this.drafts.set(this.tab, text);
    const picked = [...(this.toggled.get(this.tab) ?? [])].sort((a, b) => a - b).map((index) => q.options[index]!.label);
    this.answer(this.tab, { question: q.question, answer: [...picked, text].join(', '), custom: true, ...(picked.length ? { picks: picked, typed: text } : {}) });
  }

  private moveTab(step: number): void {
    this.tab = (this.tab + step + this.questions.length + 1) % (this.questions.length + 1);
    this.cursor = 0;
    this.warning = '';
    // A free-text question opens its editor (with the saved draft) however it is reached.
    this.typing = this.questions[this.tab]?.options.length === 0;
    this.editor.setText(this.typing ? (this.drafts.get(this.tab) ?? '') : '');
    this.refresh();
  }

  handleInput(data: string): void {
    if (this.finished) return;
    if (this.typing) {
      if (matchesKey(data, Key.escape)) {
        this.drafts.set(this.tab, this.editor.getText());
        this.typing = false;
        this.editor.setText('');
      } else this.editor.handleInput(data);
      return this.refresh();
    }
    if (this.isMulti && (matchesKey(data, Key.tab) || matchesKey(data, Key.right))) return this.moveTab(1);
    if (this.isMulti && (matchesKey(data, Key.shift('tab')) || matchesKey(data, Key.left))) return this.moveTab(-1);
    if (matchesKey(data, Key.escape)) return this.finish(true);
    if (this.tab === this.questions.length) {
      if (matchesKey(data, Key.enter) && this.answers.size === this.questions.length) this.finish(false);
      return;
    }
    this.questionKey(this.questions[this.tab]!, data);
  }

  private questionKey(q: AskQuestion, data: string): void {
    const count = rows(q).length;
    this.warning = '';
    if (matchesKey(data, Key.up)) this.cursor = (this.cursor + count - 1) % count;
    else if (matchesKey(data, Key.down)) this.cursor = (this.cursor + 1) % count;
    // Number keys only move the cursor, so a stray digit never answers or toggles.
    else if (/^[1-9]$/.test(data) && Number(data) <= count) this.cursor = Number(data) - 1;
    else if (q.multiSelect && matchesKey(data, Key.space) && this.cursor < q.options.length) this.toggle();
    else if (matchesKey(data, Key.enter)) return this.choose(q);
    else return;
    this.refresh();
  }

  private toggle(): void {
    const set = this.toggled.get(this.tab) ?? new Set<number>();
    if (set.has(this.cursor)) set.delete(this.cursor);
    else set.add(this.cursor);
    this.toggled.set(this.tab, set);
  }

  private choose(q: AskQuestion): void {
    if (this.cursor === q.options.length) {
      this.typing = true;
      this.editor.setText(this.drafts.get(this.tab) ?? '');
      return this.refresh();
    }
    if (!q.multiSelect) return this.answer(this.tab, { question: q.question, answer: q.options[this.cursor]!.label, custom: false });
    // Multi-select: Enter on an option toggles it; only the Submit row answers.
    if (this.cursor < q.options.length) {
      this.toggle();
      return this.refresh();
    }
    const set = this.toggled.get(this.tab);
    if (!set?.size) {
      this.warning = 'Select at least one option, or write an answer';
      return this.refresh();
    }
    const labels = [...set].sort((a, b) => a - b).map((index) => q.options[index]!.label);
    this.answer(this.tab, { question: q.question, answer: labels.join(', '), custom: false });
  }

  render(width: number): string[] {
    if (this.cache && this.cacheWidth === width) return this.cache;
    const { theme, questions } = this;
    const w = Math.max(1, width);
    const lines: string[] = [];
    const add = (prefix: string, text: string) => {
      const lead = truncateToWidth(prefix, Math.max(0, w - 1), '');
      const indent = visibleWidth(lead);
      const wrapped = wrapTextWithAnsi(text, Math.max(1, w - indent));
      wrapped.forEach((line, index) => lines.push(`${index === 0 ? lead : ' '.repeat(indent)}${line}`));
    };
    lines.push(theme.fg('accent', '─'.repeat(w)));
    if (this.isMulti) {
      add(' ', this.tabsLine());
      lines.push('');
    }
    if (this.tab === questions.length) this.renderReview(add, lines);
    else this.renderQuestion(questions[this.tab]!, add, lines, w);
    lines.push('');
    const q = questions[this.tab];
    const enter = !q ? 'Enter submit' : q.multiSelect ? 'Space/Enter toggle · Submit selection to confirm' : 'Enter choose';
    const help = this.typing
      ? 'Enter submit · Esc back'
      : [this.isMulti ? 'Tab/←→ questions' : '', q ? '↑↓/1-9 move' : '', enter, 'Esc cancel'].filter(Boolean).join(' · ');
    add(' ', theme.fg('dim', help));
    lines.push(theme.fg('accent', '─'.repeat(w)));
    this.cache = lines;
    this.cacheWidth = width;
    return lines;
  }

  private tabsLine(): string {
    const { theme, questions, answers } = this;
    const tabs = questions.map((q, index) => {
      const label = ` ${answers.has(index) ? '■' : '□'} ${sanitizeTerminalText(q.header ?? `Q${index + 1}`)} `;
      return index === this.tab ? theme.bg('selectedBg', theme.fg('text', label)) : theme.fg(answers.has(index) ? 'success' : 'muted', label);
    });
    const submit = ' ✓ Submit ';
    tabs.push(this.tab === questions.length ? theme.bg('selectedBg', theme.fg('text', submit)) : theme.fg(answers.size === questions.length ? 'success' : 'dim', submit));
    return tabs.join(' ');
  }

  private renderReview(add: (prefix: string, text: string) => void, lines: string[]): void {
    const { theme, questions, answers } = this;
    for (const [index, q] of questions.entries()) {
      const given = answers.get(index);
      add(' ', `${theme.fg('muted', `${sanitizeTerminalText(q.header ?? `Q${index + 1}`)}: `)}${given ? theme.fg('text', sanitizeTerminalText(given.answer)) : theme.fg('warning', '(unanswered)')}`);
    }
    lines.push('');
    add(' ', answers.size === questions.length ? theme.fg('success', 'Enter to submit') : theme.fg('warning', 'Answer every question to submit'));
  }

  private renderQuestion(q: AskQuestion, add: (prefix: string, text: string) => void, lines: string[], width: number): void {
    const { theme } = this;
    const set = this.toggled.get(this.tab) ?? new Set<number>();
    add(' ', theme.fg('muted', `Question ${this.tab + 1} of ${this.questions.length}${q.multiSelect ? ' · Choose one or more, then Submit selection' : ' · Choose one'}`));
    add(' ', theme.bold(sanitizeTerminalText(q.question)));
    const answer = this.answers.get(this.tab);
    if (answer) add(' ', theme.fg('success', `Current answer: ${sanitizeTerminalText(answer.answer)}`));
    lines.push('');
    (q.options.length === 0 && this.typing ? [] : rows(q)).forEach((label, index) => {
      const selected = index === this.cursor;
      const box = q.multiSelect && index < q.options.length ? (set.has(index) ? '[x] ' : '[ ] ') : '';
      const text = label === SUBMIT_PICKS && index > q.options.length ? `✓ ${SUBMIT_PICKS} (${set.size} selected)` : sanitizeTerminalText(label);
      add(selected ? theme.fg('accent', '› ') : '  ', theme.fg(selected ? 'accent' : index > q.options.length ? 'success' : 'text', `${index + 1}. ${box}${text}`));
      const description = q.options[index]?.description;
      if (description) add('     ', theme.fg('muted', sanitizeTerminalText(description)));
    });
    if (this.warning) {
      lines.push('');
      add(' ', theme.fg('warning', this.warning));
    }
    if (this.typing) {
      lines.push('');
      for (const line of this.editor.render(Math.max(1, width - 2))) lines.push(truncateToWidth(` ${line}`, width, ''));
    }
  }
}
