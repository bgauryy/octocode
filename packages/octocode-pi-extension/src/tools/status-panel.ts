/**
 * status-panel — the single unified below-editor "Octocode" panel.
 *
 * Rather than separate plan and Awareness widgets stacking under the editor,
 * this composes them into ONE compact widget. Spawned agents use the same canonical
 * ledger projection as the footer: the panel is the complete list, while the footer
 * keeps the live glanceable state. Each source module
 * exposes a pure `*PanelLines(theme)` builder and delegates its widget rendering
 * here; this module owns the sole `octocode-status-panel` widget.
 *
 * Sections (in order): Plan → Agents → Awareness. Model and context already live in
 * Pi's status/footer rows and must not be duplicated below the editor. Empty sections are
 * omitted; when all are empty the widget is cleared entirely.
 *
 * Uses runtime-only imports of the section builders (called inside the renderer,
 * never at module load) so the mutual module references stay cycle-safe.
 */

import type { PiContext, PiTheme } from '../types.js';
import { setManagedWidget } from './runtime-renderer.js';
import { makeRenderer } from './render-helpers.js';
import { renderStack } from '../tui/components.js';
import { activePlanScope } from './active-plan.js';
import { getCurrentPlanReadModel } from './plan-read-model.js';
import { planPanelModelLines } from './plan-tool.js';


const WIDGET_NAME = 'octocode-status-panel';

type AgentPanelSource = (theme: PiTheme | undefined, width?: number) => string[];
let agentPanelSource: AgentPanelSource | undefined;

/** Agent runtime injects its pure list builder here to avoid a module cycle. */
export function setStatusPanelAgentSource(source: AgentPanelSource | undefined): void {
  agentPanelSource = source;
}

interface BuiltPanel {
  lines: string[];
}

/**
 * Collapse a header+rows section to at most maxRows rows, appending a muted
 * "… N more" line when trimmed. `section[0]` is treated as the header.
 */
/** Join non-empty sections densely, within the total budget. */
function composeSections(sections: string[][]): string[] {
  return renderStack({ sections }, { width: Number.MAX_SAFE_INTEGER });
}

export function composeStatusPanelLines(ctx: PiContext, theme: PiTheme | undefined, width?: number): BuiltPanel {
  // Resolve the plan scope at render time, not registration time: /tree, /fork,
  // resume, and compaction can move the active branch while the widget remains
  // registered exactly once.
  const scope = activePlanScope(ctx);
  const planSection = planPanelModelLines(getCurrentPlanReadModel(ctx, scope), theme, width);
  const agentSection = agentPanelSource?.(theme, width) ?? [];
  return {
    lines: composeSections([planSection, agentSection]),
  };
}

/**
 * Re-render the unified below-editor status panel from live plan and Awareness
 * state. Clears the widget when every section is empty. Safe to
 * call from any refresh trigger; never throws.
 */
// Set during session_shutdown so late async callbacks (worker close events,
// awareness CLI refreshes) cannot resurrect the widget into the next session.
let panelSuppressed = false;
export function suppressStatusPanel(): void {
  panelSuppressed = true;
}
export function resumeStatusPanel(): void {
  panelSuppressed = false;
}

// Register-once per session ctx (pi docs: set a widget/footer ONCE and repaint
// via tui.requestRender). Re-calling setWidget with a fresh factory on every
// refresh — every 1s ledger tick, every plan mutation — rebuilt the component
// each time, which showed up as below-editor flicker and scroll jumps mid-turn.
// Keyed by ctx so a new session re-registers; cleared when the panel empties.
const panelRegisteredCtxs = new WeakSet<object>();
const panelRequestRenderByCtx = new WeakMap<object, () => void>();
export function resetStatusPanelStateForTests(): void {
  panelSuppressed = false;
}

function clearPanel(ctx: PiContext): void {
  setManagedWidget(ctx, WIDGET_NAME, undefined);
  panelRegisteredCtxs.delete(ctx);
  panelRequestRenderByCtx.delete(ctx);
}

export function refreshStatusPanel(ctx?: PiContext): void {
  if (!ctx?.hasUI) return;
  if (panelSuppressed) {
    clearPanel(ctx);
    return;
  }
  const hasPlan = getCurrentPlanReadModel(ctx, activePlanScope(ctx)).tasks.length > 0;
  const hasAgents = (agentPanelSource?.(undefined, 80).length ?? 0) > 0;
  if (!hasPlan && !hasAgents) {
    clearPanel(ctx);
    return;
  }
  if (panelRegisteredCtxs.has(ctx)) {
    // Live update: the registered renderer reads state at render time — just repaint.
    panelRequestRenderByCtx.get(ctx)?.();
    return;
  }
  panelRegisteredCtxs.add(ctx);
  setManagedWidget(
    ctx,
    WIDGET_NAME,
    (tui: unknown, theme: PiTheme) => {
      panelRequestRenderByCtx.set(ctx, () => (tui as { requestRender?: () => void } | undefined)?.requestRender?.());
      return makeRenderer((width) => {
        // Width flows into every section builder so lines are clipped at the
        // source (pi errors on over-wide lines); makeRenderer stays the net.
        const lines = composeStatusPanelLines(ctx, theme, width).lines;
        return lines.length > 0 ? lines : [''];
      });
    },
    { placement: 'belowEditor' },
  );
}
