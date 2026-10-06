import type { AutocompleteItem } from '@earendil-works/pi-tui';
import type { ExtensionAPI, ExtensionCommandContext, ExtensionContext } from '@earendil-works/pi-coding-agent';
import { withDialog } from './locks.js';
import { sanitizeTerminalText } from './sanitize.js';

/**
 * Shows a command's output. With a UI (TUI, and RPC, which forwards notifications) it is a notification. Print and
 * JSON modes have no UI (`notify` does nothing there) and Pi reserves stdout for the final answer or the JSONL event
 * stream, so the text goes to stderr: visible in a terminal, and never mixed into `-p` output or `--mode json` records.
 */
export function say(ctx: Pick<ExtensionContext, 'hasUI' | 'ui'>, text: string, level: 'info' | 'warning' | 'error' = 'info'): void {
  ctx.ui.notify(text, level);
  if (!ctx.hasUI) process.stderr.write(`${sanitizeTerminalText(text)}\n`);
}

/**
 * The subcommands of the single `/octocode` command. Features add theirs (`api`, `review`, `rewind`);
 * `src/index.ts` registers the one Pi command that routes `/octocode <name> <args>` to them and completes names and
 * their arguments.
 */

export interface Subcommand {
  /** One line for `/octocode help` and completions, including usage, e.g. `review on|off — ask before file changes`. */
  description: string;
  handler(args: string, ctx: ExtensionCommandContext): Promise<void>;
  /** Completions for the text after `<name> `. */
  complete?(prefix: string): AutocompleteItem[] | null;
}

export class Subcommands {
  private readonly entries = new Map<string, Subcommand>();
  private readonly aliased = new Set<string>();

  add(name: string, command: Subcommand): void {
    this.entries.set(name, command);
  }

  get(name: string): Subcommand | undefined {
    return this.entries.get(name);
  }

  has(name: string): boolean {
    return this.entries.has(name);
  }

  /** Subcommands by name, so help and completions read the same whatever order features registered them in. */
  list(): Array<[string, Subcommand]> {
    return [...this.entries].sort(([a], [b]) => a.localeCompare(b));
  }

  /** One line per subcommand, for `/octocode help` and a mistyped name. */
  help(): string {
    return ['Usage: /octocode [subcommand]', '  (none) — status', '  help — this list', ...this.list().map(([, command]) => `  ${command.description}`)].join('\n');
  }

  /**
   * Registers the one Pi command: `/octocode` (or `/octocode status`) notifies `status()`, `/octocode help` lists the
   * subcommands, and `/octocode <name> <args>` runs that subcommand with the rest of the line.
   */
  register(pi: ExtensionAPI, status: () => string): void {
    pi.registerCommand('octocode', {
      description: `Octocode status, or: ${this.list().map(([name]) => name).join(', ')}, help`,
      getArgumentCompletions: (prefix) => this.complete(prefix),
      handler: async (args, ctx) => {
        const line = args.trim();
        const space = line.search(/\s/);
        const name = space === -1 ? line : line.slice(0, space);
        const rest = space === -1 ? '' : line.slice(space + 1).trim();
        if (name === '' || name === 'status') return say(ctx, status());
        if (name === 'help') return say(ctx, this.help());
        const command = this.entries.get(name);
        if (!command) return say(ctx, `Unknown subcommand "${name}".\n${this.help()}`, 'warning');
        // Subcommands open dialogs (menus, confirms) while a run's tools may open theirs: one shows at a time.
        await withDialog(() => command.handler(rest, ctx));
      },
    });
  }

  /**
   * Registers `/<name>` as a top-level shortcut for `/octocode <name>`, with the same handler and completions. Every
   * subcommand stays reachable under `/octocode`; `src/index.ts` decides which get a shortcut. Aliasing twice is a no-op.
   */
  alias(pi: ExtensionAPI, name: string): void {
    const command = this.entries.get(name);
    if (!command) throw new Error(`Unknown subcommand: ${name}`);
    if (this.aliased.has(name)) return;
    this.aliased.add(name);
    pi.registerCommand(name, {
      description: command.description,
      getArgumentCompletions: (prefix) => command.complete?.(prefix) ?? null,
      handler: (args, ctx) => withDialog(() => command.handler(args.trim(), ctx)),
    });
  }

  /** Completions for everything typed after `/octocode `: subcommand names first, then that subcommand's arguments. */
  complete(prefix: string): AutocompleteItem[] | null {
    const space = prefix.indexOf(' ');
    if (space === -1) {
      const items = [...this.list(), ['help', { description: 'help — list subcommands' }] as const]
        .filter(([name]) => name.startsWith(prefix))
        .map(([name, command]) => ({ value: name === 'help' ? name : `${name} `, label: name, description: command.description }));
      return items.length > 0 ? items : null;
    }
    const name = prefix.slice(0, space);
    const rest = prefix.slice(space + 1);
    const items = this.entries.get(name)?.complete?.(rest.trimStart());
    return items?.length ? items.map((item) => ({ ...item, value: `${name} ${item.value}` })) : null;
  }
}

/** Completions for a fixed list of words, each optionally `[word, description]`. */
export function wordCompletions(words: Array<string | [string, string]>, prefix: string): AutocompleteItem[] | null {
  const items = words
    .map((entry): [string, string | undefined] => (typeof entry === 'string' ? [entry, undefined] : entry))
    .filter(([word]) => word.startsWith(prefix))
    .map(([word, description]) => ({ value: word, label: word, ...(description ? { description } : {}) }));
  return items.length > 0 ? items : null;
}
