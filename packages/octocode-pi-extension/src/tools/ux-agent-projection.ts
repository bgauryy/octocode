import type { AgentFooterEntry } from '../ui-extras.js';
import { effectiveAgentStatus } from './agents/display-state.js';

export interface UxAgentV1 {
  id: string;
  label: string;
  state: string;
  assignment?: string;
  planStep?: string;
  activeOperation?: string;
  pendingMessages: number;
  elapsedMs?: number;
  updatedAt: number;
}

const TERMINAL_AGENT_STATES = new Set([
  'done', 'failed', 'killed', 'completed', 'exited', 'error',
]);

function parsedTime(value: string, fallback: number): number {
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function stripHandbackMarkers(text: string): string {
  return text
    .replace(/\[(?:STATUS|ACTION|FINDING|METRIC|PLAN|BLOCKED|DONE|FAILED|ERROR|EVIDENCE|RESULT|ROOT|FIX|IMPACT|RISK|GAP|ASSUMPTION|QUERY|VERIFICATION|VERIFY|NEXT|ARTIFACT|HANDOFF|CONFIDENCE)\]\s*/gi, '')
    .trim();
}

export function projectUxAgents(
  source: readonly AgentFooterEntry[] | undefined,
  now: number,
): UxAgentV1[] {
  return (source ?? []).map((agent) => {
    const updatedAt = parsedTime(agent.updatedAt, now);
    const startedAt = parsedTime(agent.startedAt, updatedAt);
    const state = effectiveAgentStatus(agent);
    const terminal = TERMINAL_AGENT_STATES.has(state) || state === 'blocked';
    const messageUpdate = agent.lastMessage
      ? `msg${agent.lastMessage.direction === 'to-agent' ? '→' : '←'} ${agent.lastMessage.action}: ${stripHandbackMarkers(agent.lastMessage.preview)}`
      : undefined;
    const latestMessage = agent.lastMessage && agent.lastMessage.timestamp >= updatedAt;
    const ambientDelta = agent.deltaSummary ? stripHandbackMarkers(agent.deltaSummary) : undefined;
    const activeOperation = state === 'running' && agent.activeTool
      ? `tool ${agent.activeTool}`
      : (state === 'queued' || latestMessage) && messageUpdate
        ? messageUpdate
        : ambientDelta || messageUpdate;
    return {
      id: agent.agentId,
      label: agent.name,
      state,
      ...(agent.task ? { assignment: agent.task } : {}),
      ...(agent.planStep ? { planStep: agent.planStep } : {}),
      ...(activeOperation ? { activeOperation } : {}),
      pendingMessages: agent.pendingMessages ?? 0,
      elapsedMs: Math.max(0, (terminal ? updatedAt : now) - startedAt),
      updatedAt,
    };
  }).sort((a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id));
}
