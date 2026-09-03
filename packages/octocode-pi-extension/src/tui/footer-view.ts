/** Pure footer view: state collection stays outside, layout stays testable here. */
import type { PiTheme } from '../types.js';
import type { SemanticToken } from './palette.js';
import { renderInlineRows, renderStack, type InlineSegment, type TuiRenderContext } from './components.js';

export interface FooterAgentView {
  label: string;
  model?: string;
  task?: string;
  planStep?: string;
  state: string;
  elapsed: string;
  doing?: string;
  token?: SemanticToken;
  attention?: boolean;
}

export interface FooterViewProps {
  identity: readonly InlineSegment[];
  metrics: readonly InlineSegment[];
  agents: readonly FooterAgentView[];
  shortcuts: readonly InlineSegment[];
}

function agentSegments(agent: FooterAgentView): InlineSegment[] {
  return [
    { text: `  ${agent.label}`, token: 'muted' },
    { text: agent.state, token: agent.token ?? (agent.state === 'failed' ? 'error' : agent.state === 'blocked' ? 'warning' : agent.state === 'done' ? 'success' : 'brand'), attention: agent.attention },
    { text: agent.elapsed, token: 'dim' },
    ...(agent.model ? [{ text: `model ${agent.model}`, token: 'link' as const }] : []),
    ...(agent.task ? [{ text: `task: ${agent.task}`, token: 'muted' as const }] : []),
    ...(agent.planStep ? [{ text: `plan: ${agent.planStep}`, token: 'symbol' as const }] : []),
    ...(agent.doing ? [{ text: `now: ${agent.doing}`, token: 'dim' as const }] : []),
  ];
}

/** Responsive component used by Pi and by the visual permutation generator. */
export function renderFooterView(props: FooterViewProps, context: TuiRenderContext & { theme?: PiTheme }): string[] {
  const identity = renderInlineRows({ segments: props.identity }, context);
  const metrics = renderInlineRows({ segments: props.metrics, prioritizeAttention: true }, context);
  const agents = props.agents.flatMap((agent) => renderInlineRows({ segments: agentSegments(agent) }, context));
  const shortcuts = renderInlineRows({ segments: props.shortcuts, prefix: props.shortcuts.length > 0 ? 'keys ' : '' }, context);
  return renderStack({ sections: [identity, metrics, agents, shortcuts] }, context);
}
