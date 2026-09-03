/**
 * command-palette — fuzzy-ish command/action picker for the Octocode pi extension.
 *
 * REQUIRED WIRING in src/index.ts (this module never edits index.ts itself):
 *
 *   import { registerCommandPalette } from './tools/command-palette.js';
 *
 *   // after the other register* calls, inside the extension activation:
 *   registerCommandPalette(pi, {
 *     commands: [
 *       // octocode + pi built-in slash commands the palette should surface.
 *       // Anything discoverable via pi.getCommands() is merged in automatically;
 *       // list here only commands that want an args prefill (takesArgs: true),
 *       // e.g. { name: 'octocode-theme', description: 'Switch theme', takesArgs: true },
 *     ],
 *     actions: [
 *       // direct actions (open inbox, toggle watch, …) contributed by other features:
 *       // { id: 'inbox', label: 'Open inbox', description: '…', run: (ctx) => … },
 *     ],
 *   });
 *
 *   // and add '/octocode-palette' to listExtensionHarness().extensionCommands.
 *
 * Contract caveats (verified against pi-coding-agent 0.80.3):
 * - pi.registerShortcut only fires while the MAIN editor has focus — the palette
 *   shortcut is dead inside overlays/custom components. The '/octocode-palette'
 *   command is the always-available path.
 * - Built-in shortcuts marked restrictOverride:true cannot be overridden; pi skips
 *   the registration with a warning (and hosts may throw). The default key
 *   may collide with a host binding on some pi versions — registration is wrapped
 *   in try/catch and degrades to command-only. Override via OCTOCODE_PALETTE_KEY.
 * - runSelectOverlay's type-to-filter is a case-insensitive SUBSTRING match over
 *   label/value/description (its own filtering — NOT pi-tui SelectList.setFilter,
 *   which prefix-matches item `value` and would be useless for `cmd:<name>` ids).
 *   Items keep the stable `cmd:<name>` / `action:<id>` value scheme and are
 *   pre-sorted (actions first, then commands, each alphabetically) so the list
 *   is scannable even when the filter buffer is empty. Do NOT reintroduce setFilter.
 */

import type { PiCommandContext, PiContext, PiInstance } from '../types.js';
import { runSelectOverlay, type SelectOverlayItem } from './ui-overlays.js';

// ─── Deps ────────────────────────────────────────────────────────────────────

/** A slash command surfaced in the palette (octocode's or a pi built-in). */
export interface PaletteCommand {
  /** Command name WITHOUT the leading slash, e.g. 'octocode-status'. */
  name: string;
  description?: string;
  /**
   * When true, selecting the command prefills the editor with '/name ' so the
   * user completes the arguments; when false/omitted it is sent immediately.
   */
  takesArgs?: boolean;
}

/** A direct action (open inbox, toggle watch, …) injected by the wiring. */
export interface PaletteAction {
  /** Stable id — becomes the item value 'action:<id>'. */
  id: string;
  label: string;
  description?: string;
  run(ctx: PiContext | PiCommandContext | undefined): void | Promise<void>;
}

export interface CommandPaletteDeps {
  /** Slash commands to list. Merged with pi.getCommands() (deps win on name clash). */
  commands?: PaletteCommand[];
  /** Direct actions to list; injectable so other features can extend the palette. */
  actions?: PaletteAction[];
  /** Env source for OCTOCODE_PALETTE_KEY (default process.env). */
  env?: NodeJS.ProcessEnv;
  /** Overlay title (default 'Octocode palette'). */
  title?: string;
}

export interface CommandPaletteRegistration {
  /** The shortcut actually registered, or undefined when degraded to command-only. */
  shortcut: string | undefined;
}

// ─── Pure item building ──────────────────────────────────────────────────────

export const CMD_VALUE_PREFIX = 'cmd:';
export const ACTION_VALUE_PREFIX = 'action:';

/**
 * Build the (pre-sorted) palette item list. Pure: actions first, then commands,
 * each sorted alphabetically. Values follow the `cmd:<name>` / `action:<id>`
 * scheme consumed by dispatchPaletteSelection.
 */
export function buildPaletteItems(deps: Pick<CommandPaletteDeps, 'commands' | 'actions'>): SelectOverlayItem[] {
  const byLabel = (a: SelectOverlayItem, b: SelectOverlayItem) => a.label.localeCompare(b.label);

  const actionItems: SelectOverlayItem[] = (deps.actions ?? []).map((action) => ({
    value: `${ACTION_VALUE_PREFIX}${action.id}`,
    label: action.label,
    description: action.description,
  })).sort(byLabel);

  const commandItems: SelectOverlayItem[] = (deps.commands ?? []).map((cmd) => ({
    value: `${CMD_VALUE_PREFIX}${cmd.name}`,
    label: `/${cmd.name}`,
    description: cmd.takesArgs ? `${cmd.description ?? ''} (prompts for args)`.trim() : cmd.description,
  })).sort(byLabel);

  return [...actionItems, ...commandItems];
}

// ─── Dispatch ────────────────────────────────────────────────────────────────

/**
 * Route a selected palette value.
 * - `cmd:<name>` without args → pi.sendUserMessage('/name', { deliverAs: 'followUp', expandPromptTemplates: true })
 *   (Pi dispatches extension slash commands from extension-originated input only when expansion is enabled).
 * - `cmd:<name>` with takesArgs → ctx.ui.setEditorText('/name ') prefill.
 * - `action:<id>` → the injected handler.
 * Returns true when the value was recognized and handled.
 */
export async function dispatchPaletteSelection(
  value: string,
  pi: Pick<PiInstance, 'sendUserMessage'>,
  ctx: PiContext | PiCommandContext | undefined,
  deps: Pick<CommandPaletteDeps, 'commands' | 'actions'>,
): Promise<boolean> {
  if (value.startsWith(CMD_VALUE_PREFIX)) {
    const name = value.slice(CMD_VALUE_PREFIX.length);
    if (!name) return false;
    const cmd = (deps.commands ?? []).find((c) => c.name === name);
    if (cmd?.takesArgs) {
      // Prefill so the user completes the arguments in the main editor.
      ctx?.ui?.setEditorText?.(`/${name} `);
      return true;
    }
    pi.sendUserMessage?.(`/${name}`, { deliverAs: 'followUp', expandPromptTemplates: true });
    return true;
  }
  if (value.startsWith(ACTION_VALUE_PREFIX)) {
    const id = value.slice(ACTION_VALUE_PREFIX.length);
    const action = (deps.actions ?? []).find((a) => a.id === id);
    if (!action) return false;
    await action.run(ctx);
    return true;
  }
  return false;
}

// ─── Open + registration ─────────────────────────────────────────────────────

/** Merge injected commands with pi.getCommands() discovery (deps win on name). */
function collectCommands(pi: PiInstance, deps: CommandPaletteDeps): PaletteCommand[] {
  const merged = new Map<string, PaletteCommand>();
  let discovered: Array<{ name: string; description?: string }> = [];
  try {
    discovered = pi.getCommands?.() ?? [];
  } catch {
    // getCommands is best-effort discovery only.
  }
  for (const cmd of discovered) {
    if (cmd?.name) merged.set(cmd.name, { name: cmd.name, description: cmd.description });
  }
  for (const cmd of deps.commands ?? []) merged.set(cmd.name, cmd);
  return [...merged.values()];
}

/** Open the palette overlay and dispatch the chosen item. Exported for tests/wiring. */
export async function openCommandPalette(
  pi: PiInstance,
  ctx: PiContext | PiCommandContext | undefined,
  deps: CommandPaletteDeps = {},
): Promise<void> {
  if (!ctx?.hasUI) return;
  const commands = collectCommands(pi, deps);
  const actions = deps.actions ?? [];
  const items = buildPaletteItems({ commands, actions });
  if (items.length === 0) {
    ctx.ui?.notify?.('Command palette: nothing to show', 'info');
    return;
  }
  const value = await runSelectOverlay(ctx, {
    title: deps.title ?? 'Octocode palette',
    items,
    filter: true,
  });
  if (!value) return; // null = cancelled, undefined = no interactive UI
  await dispatchPaletteSelection(value, pi, ctx, { commands, actions });
}

/**
 * Register the '/octocode-palette' command and (best-effort) a keyboard shortcut.
 * Shortcut key: OCTOCODE_PALETTE_KEY env override, default 'ctrl+shift+k'. If shortcut
 * registration throws (host conflict / restrictOverride built-in), the palette
 * degrades gracefully to command-only.
 */
/** The shortcut the palette actually registered this process, for UI hints. */
let registeredPaletteShortcut: string | undefined;
export function getPaletteShortcut(): string | undefined {
  return registeredPaletteShortcut;
}

export function registerCommandPalette(
  pi: PiInstance,
  deps: CommandPaletteDeps = {},
): CommandPaletteRegistration {
  const env = deps.env ?? process.env;
  // ctrl+o is pi's RESERVED app.tools.expand — an extension shortcut on it is
  // silently skipped at resolution, so the default must be an unbound key.
  const key = env['OCTOCODE_PALETTE_KEY'] || 'ctrl+shift+k';

  pi.registerCommand?.('octocode-palette', {
    description: 'Open the Octocode command palette (commands + actions picker)',
    handler: async (_args, ctx) => {
      await openCommandPalette(pi, ctx, deps);
    },
  });

  let shortcut: string | undefined;
  try {
    if (typeof pi.registerShortcut === 'function') {
      pi.registerShortcut(key, {
        description: 'Open the Octocode command palette',
        handler: async (ctx) => {
          await openCommandPalette(pi, ctx, deps);
        },
      });
      shortcut = key;
    }
  } catch {
    // Conflict with a restrictOverride built-in or host rejection —
    // degrade to command-only ('/octocode-palette' still works).
    shortcut = undefined;
  }
  registeredPaletteShortcut = shortcut;
  return { shortcut };
}
