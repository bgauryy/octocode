/** Typed contract for commands implemented by the host integration adapter. */
export type CommandParamType = 'string' | 'integer' | 'boolean' | 'string[]';

export interface CommandParam {
  name: string;
  flag: string;
  type: CommandParamType;
  required?: boolean;
  enum?: readonly string[];
  description: string;
  min?: number;
  max?: number;
  durationMs?: boolean;
}

export interface CommandAction {
  action: string;
  summary: string;
  /** Inject the current agent id (host-owned) as a flag. */
  needsAgentId?: boolean;
  /** Flag name for the injected agent id (default `agent-id`). */
  agentIdFlag?: string;
  params: readonly CommandParam[];
}

export interface CommandGroup {
  /** Tool/resource name exposed to hosts, e.g. `lock`, `agent`. */
  resource: string;
  /** Canonical CLI noun, e.g. `lock`, `check`, or `agent`. */
  cli: string;
  label: string;
  summary: string;
  /** A single read with no action discriminator (e.g. status). */
  singleton?: boolean;
  actions: readonly CommandAction[];
}


const text = (name: string, flag: string, required = false): CommandParam => ({ name, flag, type: 'string', required, description: flag.replaceAll('-', ' ') });
export const AWARENESS_COMMANDS: readonly CommandGroup[] = [
  { resource: 'awarenessStatus', cli: 'status', label: 'Status', singleton: true, summary: 'Read shared activity and verification counts.', actions: [
    { action: 'status', summary: 'Read shared activity.', params: [{ name: 'staleAfterMs', flag: 'stale-after', type: 'integer', durationMs: true, min: 0, description: 'Peer freshness window.' }] },
  ] },
  { resource: 'handoff', cli: 'handoff', label: 'Handoff', summary: 'Record and clear continuity notes.', actions: [
    { action: 'add', summary: 'Record a handoff.', needsAgentId: true, params: [text('summary', 'summary', true), { name: 'files', flag: 'file', type: 'string[]', description: 'Related files.' }] },
    { action: 'list', summary: 'Read handoffs.', params: [{ name: 'includeCleared', flag: 'include-cleared', type: 'boolean', description: 'Include cleared notes.' }] },
    { action: 'clear', summary: 'Clear a handoff.', params: [text('handoffId', 'handoff-id', true)] },
  ] },
  { resource: 'agent', cli: 'agent', label: 'Agent', summary: 'Update registered agent presence.', actions: [
    { action: 'touch', summary: 'Refresh presence.', needsAgentId: true, params: [{ ...text('status', 'status'), enum: ['ACTIVE', 'IDLE'] }] },
    { action: 'leave', summary: 'End presence.', needsAgentId: true, params: [] },
  ] },
];

export function getCommandGroup(resource: string): CommandGroup | undefined {
  return AWARENESS_COMMANDS.find(group => group.resource === resource);
}
