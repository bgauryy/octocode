import { truncateToWidth, visibleWidth } from '../tui/width.js';
/**
 * ui-overlays — reusable Octocode TUI overlay helpers built on `@earendil-works/pi-tui`.
 *
 * We deliberately depend ONLY on pi-tui (which Pi's extension loader aliases to the
 * host copy), NOT on `@earendil-works/pi-coding-agent` helpers like getSelectListTheme /
 * DynamicBorder / SessionSelectorComponent: those are not our dependency and are not
 * guaranteed to resolve from the extension's install location at runtime. We reimplement
 * the small pieces we need (theme mapping, a framed select overlay with type-to-filter)
 * against the pi-tui primitives so the overlay works on any Pi that ships pi-tui.
 */

import { SelectList } from "@earendil-works/pi-tui";
import type { PiTheme, PiContext } from "../types.js";
import { MultiSelectList, multiSelectKeyAction, type MultiSelectTheme } from "./multi-select-list.js";

import { TOKEN, type SemanticToken } from "../tui/palette.js";
import { OVERLAY_HELP_MULTI, OVERLAY_HELP_SELECT, OVERLAY_HELP_SELECT_FILTER } from "../tui/content.js";

/**
 * Semantic paint with defensive chaining (overlay themes may lack fg in tests).
 * Routes every overlay color through the TOKEN map so a palette remap reaches
 * the overlays too.
 */
function fgTok(theme: PiTheme | undefined, token: SemanticToken, text: string): string {
  return theme?.fg?.(TOKEN[token], text) ?? text;
}

/**
 * Shared responsive geometry for Octocode overlays: cap the height and let pi
 * hide the overlay entirely on terminals too narrow to render it legibly.
 */
export const OCTOCODE_OVERLAY_OPTIONS = {
  width: 72,
  minWidth: 32,
  maxHeight: "70%",
  margin: 2,
  visible: (termWidth: number) => termWidth >= 32,
} as const;

/** Quiet overlay title: the host already provides the application identity. */
function overlayHeading(theme: PiTheme | undefined, title: string): string {
  return fgTok(theme, "brand", theme?.bold?.(title) ?? title);
}

/**
 * An overlay is layered over existing terminal cells. Paint its complete row,
 * including trailing spaces, so text and artwork from the transcript cannot
 * bleed through the transparent remainder of a short line.
 */
function paintOverlayLine(line: string, width: number): string {
  const clipped = truncateToWidth(line, width);
  return `${clipped}${' '.repeat(Math.max(0, width - visibleWidth(clipped)))}`;
}

/** pi-tui SelectList theme shape (5 colorizer fns). */
export interface SelectListThemeFns {
  selectedPrefix: (t: string) => string;
  selectedText: (t: string) => string;
  description: (t: string) => string;
  scrollInfo: (t: string) => string;
  noMatch: (t: string) => string;
}

const id = (t: string) => t;

/** Map the Octocode/Pi theme to a pi-tui SelectList theme via the semantic TOKEN map. */
export function octocodeSelectListTheme(theme?: PiTheme): SelectListThemeFns {
  const fg = (token: SemanticToken) => (t: string) => fgTok(theme, token, t) || id(t);
  return {
    selectedPrefix: fg("brand"),
    selectedText: fg("brand"),
    description: fg("muted"),
    scrollInfo: fg("dim"),
    noMatch: fg("muted"),
  };
}

export interface FilterKeyResult {
  buffer: string;
  changed: boolean;
}

/**
 * Pure key handler for a type-to-filter buffer. Returns the next buffer and whether it
 * changed. Appends printable chars, deletes on backspace (DEL 0x7f / BS 0x08), and
 * ignores navigation/control keys (arrows, enter, tab, esc, ctrl-c) which the SelectList
 * handles itself.
 */
export function applyFilterKey(
  buffer: string,
  keyData: string,
): FilterKeyResult {
  if (keyData === "\x7f" || keyData === "\b") {
    if (buffer.length === 0) return { buffer, changed: false };
    return { buffer: buffer.slice(0, -1), changed: true };
  }
  // Single printable character (space through ~). Excludes ESC-sequences (multi-char),
  // CR/LF, TAB, and control bytes.
  if (keyData.length === 1 && keyData >= " " && keyData <= "~") {
    return { buffer: buffer + keyData, changed: true };
  }
  return { buffer, changed: false };
}

export interface SelectOverlayItem {
  value: string;
  label: string;
  description?: string;
  /** Optional multi-line preview shown under the item while focused (multi-select overlay only). */
  preview?: string;
}

export interface SelectOverlayOptions {
  title: string;
  items: SelectOverlayItem[];
  /** Enable type-to-filter (adds a filter line + wires SelectList.setFilter). Default true when >8 items. */
  filter?: boolean;
  maxVisible?: number;
}

/**
 * Present a framed, keyboard-navigable select overlay and resolve with the chosen value
 * (or null on cancel). Type-to-filter is wired through the pure `applyFilterKey` buffer.
 * Returns undefined when the host has no interactive UI.
 */
/** Case-insensitive substring match on everything the user can SEE (label,
 * description) plus the value — not a value-prefix match, because values are
 * often internal (`cmd:…`, commit SHAs) and never what the user types. */
export function selectItemMatchesFilter(item: SelectOverlayItem, filter: string): boolean {
  const f = filter.trim().toLowerCase();
  if (!f) return true;
  return [item.label, item.value, item.description ?? ""].some((s) =>
    s.toLowerCase().includes(f),
  );
}

export async function runSelectOverlay(
  ctx: PiContext | undefined,
  opts: SelectOverlayOptions,
): Promise<string | null | undefined> {
  if (ctx?.mode !== "tui" || !ctx?.hasUI || typeof ctx.ui?.custom !== "function") return undefined;
  const enableFilter = opts.filter ?? opts.items.length > 8;

  // TODO(abort): the awaited custom() promise settles only on a user keypress
  // (done() in onSelect/onCancel/empty-esc). If the surrounding tool turn is
  // aborted while the overlay is open, this can hang. There is currently no safe
  // wiring to force-dismiss it: PiContext exposes no AbortSignal or abort event
  // (see PiContext in types.ts), and the custom() `onHandle` callback yields an
  // untyped `handle: unknown` with no documented dismiss method — calling one
  // would fabricate an API. Wiring a real abort path needs either a signal on
  // PiContext or a typed dismiss handle, plus threading a signal from callers.
  return ctx.ui.custom<string | null>(
    (
      tui: any,
      theme: PiTheme,
      _kb: unknown,
      done: (v: string | null) => void,
    ) => {
      const heading = overlayHeading(theme, opts.title);
      let filter = "";

      // The list is rebuilt when the filter changes: pi-tui's own setFilter
      // prefix-matches on item.value (internal ids like `cmd:…` / SHAs), which
      // made typing what you see filter everything out. We filter on the
      // visible label/description ourselves instead.
      const makeList = (keepValue?: string): { list: any; empty: boolean } => {
        const visible = opts.items.filter((o) => selectItemMatchesFilter(o, filter));
        if (visible.length === 0) return { list: undefined, empty: true };
        const list = new SelectList(
          visible.map((o) => ({
            value: o.value,
            label: o.label,
            description: o.description,
          })) as any,
            Math.min(opts.maxVisible ?? 8, Math.max(1, visible.length)),
          octocodeSelectListTheme(theme) as any,
        );
        (list as any).onSelect = (item: { value: string }) => done(item.value);
        (list as any).onCancel = () => done(null);
        // Rebuilds must not lose the user's place: re-select the previously
        // highlighted item when it survives the filter (SelectList exposes
        // setSelectedIndex/getSelectedItem for exactly this).
        if (keepValue) {
          const keepIndex = visible.findIndex((o) => o.value === keepValue);
          if (keepIndex > 0) (list as any).setSelectedIndex?.(keepIndex);
        }
        return { list, empty: false };
      };
      let { list, empty } = makeList();

      const help = enableFilter ? OVERLAY_HELP_SELECT_FILTER : OVERLAY_HELP_SELECT;
      const helpLine = fgTok(theme, "dim", help);

      return {
        render: (w: number) => {
          const lines: string[] = [heading];
          if (enableFilter && filter) {
            lines.push(fgTok(theme, "dim", `filter: ${filter}`));
          }
          if (empty) {
            lines.push(fgTok(theme, "muted", "  no matches — backspace to clear"));
          } else {
            lines.push(...(list.render(w) as string[]).map((l: string) => ` ${l}`));
          }
          lines.push(helpLine);
          return lines.map((line) => paintOverlayLine(line, w));
        },
        invalidate: () => list?.invalidate?.(),
        handleInput: (data: string) => {
          if (enableFilter) {
            const next = applyFilterKey(filter, data);
            if (next.changed) {
              filter = next.buffer;
              const keepValue = (list as any)?.getSelectedItem?.()?.value as string | undefined;
              ({ list, empty } = makeList(keepValue));
              tui?.requestRender?.();
              return;
            }
          }
          if (empty) {
            // Only esc/ctrl-c can act while nothing matches.
            if (data === "\x1b" || data === "\x03") done(null);
            return;
          }
          (list as any).handleInput(data);
          tui?.requestRender?.();
        },
      };
    },
    { overlay: true, overlayOptions: OCTOCODE_OVERLAY_OPTIONS },
  );
}

// ─── Multi-select overlay ─────────────────────────────────────────────────────

export interface MultiSelectOverlayOptions {
  title: string;
  items: SelectOverlayItem[];
  /** Minimum selections required before enter confirms (default 0). */
  min?: number;
  /** Maximum selections allowed — extra toggles are no-ops (default unlimited). */
  max?: number;
  /** Values pre-toggled when the overlay opens. */
  initial?: string[];
}

/**
 * Present a framed, keyboard-navigable multi-select overlay (space toggles,
 * enter confirms once min/max are satisfied, esc cancels) and resolve with the
 * chosen values in display order. Resolves undefined on cancel or when the
 * host has no interactive UI. All list state lives in the pure MultiSelectList;
 * this wrapper only owns host plumbing, mirroring runSelectOverlay.
 */
export async function runMultiSelectOverlay(
  ctx: PiContext | undefined,
  opts: MultiSelectOverlayOptions,
): Promise<string[] | undefined> {
  if (ctx?.mode !== "tui" || !ctx?.hasUI || typeof ctx.ui?.custom !== "function") return undefined;

  // TODO(abort): same gap as runSelectOverlay — this awaited custom() promise
  // settles only on a user keypress (done() in the confirm/cancel handlers), so
  // an aborted tool turn can hang with the overlay open. No safe programmatic
  // dismiss exists: PiContext carries no AbortSignal/abort event and custom()'s
  // `onHandle` handle is untyped (`unknown`) with no documented dismiss API.
  const result = await ctx.ui.custom<string[] | null>(
    (
      tui: any,
      theme: PiTheme,
      _kb: unknown,
      done: (v: string[] | null) => void,
    ) => {
      const heading = overlayHeading(theme, opts.title);
      const help = OVERLAY_HELP_MULTI;
      const helpLine = fgTok(theme, "dim", help);

      const list = new MultiSelectList(
        opts.items.map((o) => ({
          value: o.value,
          label: o.label,
          description: o.description,
          preview: o.preview,
        })),
        { min: opts.min, max: opts.max, initial: opts.initial },
      );

      return {
        render: (w: number) =>
          [heading, ...list.render(w, theme as unknown as MultiSelectTheme), helpLine].map(
            (line) => paintOverlayLine(line, w),
          ),
        invalidate: () => {},
        handleInput: (data: string) => {
          const action = multiSelectKeyAction(data);
          if (action === "cancel") {
            done(null);
            return;
          }
          if (action === "confirm") {
            if (list.canConfirm()) {
              done(list.selectedValues());
              return;
            }
            // Not confirmable yet — fall through so the warning footer redraws.
          } else if (action === "up") list.moveCursor(-1);
          else if (action === "down") list.moveCursor(1);
          else if (action === "toggle") list.toggle();
          tui?.requestRender?.();
        },
      };
    },
    { overlay: true, overlayOptions: OCTOCODE_OVERLAY_OPTIONS },
  );
  return result ?? undefined;
}
