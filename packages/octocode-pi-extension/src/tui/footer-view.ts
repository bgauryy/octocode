import { truncateToWidth } from './width.js';
/** Pure footer view: state collection stays outside, layout stays testable here. */
import type { PiTheme } from '../types.js';
import { renderInlineRows, type InlineSegment, type TuiRenderContext } from './components.js';
import { paint, SEP, type SemanticToken } from './palette.js';


export interface FooterAgentView {
  label: string;
  state: string;
  elapsed: string;
  task?: string;
  doing?: string;
  token?: SemanticToken;
  attention?: boolean;
}

export interface FooterViewProps {
  /**
   * Semantic footer rows. Each row wraps responsively, but no state is replaced
   * by a `+N` disclosure. Keep related facts together (activity/context, plan,
   * repository identity, metrics) so a narrow terminal never hides an entire
   * category of live state.
   */
  rows: readonly (readonly InlineSegment[])[];
  agents?: readonly FooterAgentView[];
}

function agentRow(agent: FooterAgentView, context: TuiRenderContext): string[] {
  const stateToken = agent.token ?? (agent.state === 'failed' ? 'error' : agent.state === 'blocked' ? 'warning' : agent.state === 'done' ? 'success' : 'brand');
  const required = `  ${paint(context.theme, 'muted', agent.label)}${SEP}${paint(context.theme, stateToken, agent.state)}`;
  const summary = [agent.elapsed, agent.task ? `task ${agent.task}` : ''].filter(Boolean).join(SEP);
  const lines = [truncateToWidth(summary ? `${required}${SEP}${paint(context.theme, 'dim', summary)}` : required, context.width)];
  if (agent.doing) lines.push(truncateToWidth(`    ${paint(context.theme, 'muted', `doing ${agent.doing}`)}`, context.width));
  return lines;
}

/** Unified persistent state: responsive semantic rows plus every visible worker. */
export function renderFooterView(props: FooterViewProps, context: TuiRenderContext & { theme?: PiTheme }): string[] {
  const header = props.rows.flatMap((segments) => renderInlineRows({ segments }, context));
  const agents = (props.agents ?? [])
    .filter((agent) => agent.state !== 'killed')
    .map((agent, index) => ({ agent, index }))
    .sort((a, b) => Number(b.agent.attention) - Number(a.agent.attention) || a.index - b.index)
    .map(({ agent }) => agent);
  return [...header, ...agents.flatMap((agent) => agentRow(agent, context))];
}
