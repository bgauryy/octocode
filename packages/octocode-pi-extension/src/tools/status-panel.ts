/**
 * Legacy status-panel projection.
 *
 * The custom footer is now the only persistent plan/worker owner. The pure
 * composer remains useful to explicit inspection views and compatibility tests.
 */

import type { PiContext, PiTheme } from '../types.js';
import { renderStack } from '../tui/components.js';
import { activePlanScope } from './active-plan.js';
import { getCurrentPlanReadModel } from './plan-read-model.js';
import { planPanelModelLines } from './plan-tool.js';


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
const STATUS_PANEL_MAX_LINES = 7;

/** Join non-empty sections densely and disclose overflow instead of filling the viewport. */
function composeSections(sections: string[][]): string[] {
  const lines = renderStack({ sections }, { width: Number.MAX_SAFE_INTEGER });
  if (lines.length <= STATUS_PANEL_MAX_LINES) return lines;
  const visible = lines.slice(0, STATUS_PANEL_MAX_LINES - 1);
  visible.push(`… ${lines.length - visible.length} more · /octocode-status`);
  return visible;
}

export function composeStatusPanelLines(ctx: PiContext, theme: PiTheme | undefined, width?: number): BuiltPanel {
  // Resolve the plan scope at render time so /tree, /fork, resume, and
  // compaction always inspect the current branch.
  const scope = activePlanScope(ctx);
  const planSection = planPanelModelLines(getCurrentPlanReadModel(ctx, scope), theme, width);
  const agentSection = agentPanelSource?.(theme, width) ?? [];
  return {
    lines: composeSections([planSection, agentSection]),
  };
}

export function resetStatusPanelStateForTests(): void {
  agentPanelSource = undefined;
}
