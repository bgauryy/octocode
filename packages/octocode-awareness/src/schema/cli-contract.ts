import { HISTORY_ROUTE_DESCRIPTORS } from './definitions-history.js';

const HISTORY_REQUIRED = Object.fromEntries(
  HISTORY_ROUTE_DESCRIPTORS.map(({ command, required }) => [command, [...required]]),
);
const HISTORY_ALLOWED = Object.fromEntries(
  HISTORY_ROUTE_DESCRIPTORS.map(({ command, allowed }) => [command, [...allowed]]),
);

export const CLI_REQUIRED: Record<string, string[]> = {
  'plan create': ['name', 'objective', 'lead_agent_id', 'workspace'], 'plan show': ['plan_id'],
  'plan join': ['plan_id', 'agent_id'], 'plan doc': ['plan_id', 'agent_id', 'path', 'title'],
  'plan status': ['plan_id', 'agent_id', 'status'], 'task create': ['plan_id', 'title', 'reasoning', 'acceptance', 'path', 'agent_id'],
  'task show': ['task_id'], 'task claim': ['agent_id'], 'task heartbeat': ['task_id', 'run_id', 'agent_id'],
  'task submit': ['task_id', 'run_id', 'agent_id'], 'task release': ['task_id', 'run_id', 'agent_id'],
  'task retry': ['task_id', 'agent_id'], 'task depend': ['task_id', 'depends_on', 'agent_id'],
  'work start': ['agent_id', 'file'], 'work touch': ['agent_id', 'run_id'], 'work end': ['agent_id', 'run_id'],
  'work show': ['workspace', 'file'], 'memory record': ['agent_id', 'task_context', 'observation', 'importance'],
  'signal publish': ['agent_id', 'kind', 'subject'], 'signal reply': ['agent_id', 'in_reply_to', 'subject'],
  'signal resolve': ['agent_id'],
  ...HISTORY_REQUIRED,
};

const CLI_ALLOWED: Record<string, string[]> = {
  'agent touch': ['workspace', 'agent_id', 'status'], 'agent leave': ['workspace', 'agent_id'],
  'plan create': ['name', 'objective', 'lead_agent_id', 'workspace', 'artifact'], 'plan list': ['workspace', 'artifact', 'status', 'limit', 'full'],
  'plan show': ['plan_id', 'full'], 'plan join': ['plan_id', 'agent_id'], 'plan doc': ['plan_id', 'agent_id', 'path', 'title'], 'plan status': ['plan_id', 'agent_id', 'status'],
  'task create': ['plan_id', 'title', 'reasoning', 'acceptance', 'path', 'depends_on', 'agent_id', 'priority'],
  'task list': ['plan_id', 'workspace', 'status', 'limit', 'full'], 'task ready': ['plan_id', 'workspace', 'limit', 'full'], 'task show': ['task_id', 'full'],
  'task claim': ['task_id', 'plan_id', 'agent_id', 'next', 'lease_minutes', 'test_plan'], 'task heartbeat': ['task_id', 'run_id', 'agent_id', 'lease_minutes'],
  'task submit': ['task_id', 'run_id', 'agent_id', 'message'], 'task release': ['task_id', 'run_id', 'agent_id', 'blocked_reason'],
  'task retry': ['task_id', 'agent_id', 'message'], 'task depend': ['task_id', 'depends_on', 'agent_id'],
  'work start': ['agent_id', 'session_id', 'workspace', 'artifact', 'run_id', 'rationale', 'test_plan', 'context_ref', 'file', 'exclusive', 'ttl_minutes', 'ttl_seconds'],
  'work touch': ['agent_id', 'run_id', 'file', 'ttl_minutes', 'ttl_seconds'], 'work end': ['agent_id', 'run_id', 'file'],
  'work list': ['agent_id', 'workspace', 'artifact', 'run_id', 'all', 'full', 'limit', 'offset'], 'work show': ['agent_id', 'workspace', 'artifact', 'run_id', 'file', 'all', 'full', 'limit', 'offset'],
  'verify audit': ['agent_id', 'workspace', 'artifact', 'older_than_days', 'origin', 'before', 'limit', 'offset'],
  'verify mark': ['run_id', 'all_pending', 'agent_id', 'status', 'message', 'workspace', 'artifact', 'adopt_verification'],
  'signal publish': ['agent_id', 'workspace', 'artifact', 'repo', 'ref', 'kind', 'subject', 'body', 'data', 'to_agent', 'file', 'ref_id', 'importance'],
  'signal list': ['agent_id', 'workspace', 'artifact', 'repo', 'ref', 'kind', 'thread_id', 'signal_id', 'all', 'unread_only', 'mark_read', 'limit', 'cursor', 'include_bodies', 'format'],
  'signal reply': ['agent_id', 'workspace', 'artifact', 'repo', 'ref', 'in_reply_to', 'subject', 'body', 'data', 'to_agent', 'file', 'ref_id', 'importance'],
  'signal resolve': ['agent_id', 'signal_id', 'thread_id'],
  ...HISTORY_ALLOWED,
};

/** Exact canonical root flags for a noun/action route, excluding global flags. */
export function cliAllowedFlags(commandName: string): readonly string[] | undefined {
  return CLI_ALLOWED[commandName];
}

function canonicalFieldNames(commandName: string): Record<string, string> {
  return {
    workspace_path: 'workspace', target_files: 'file', tags: 'tag', references: 'reference', labels: 'label', files: 'file', states: 'state',
    eval_failures: 'eval_failure_json', to_agents: 'to_agent', refs: 'ref_id',
    ...(commandName.startsWith('lock ') ? { target_files: 'target_file' } : {}),
  };
}

export function projectCliProperties(properties: Record<string, unknown>, commandName: string): Record<string, string> {
  const canonicalNames = canonicalFieldNames(commandName);
  if (commandName === 'signal list' && properties.kinds) {
    properties.kind = properties.kinds;
    properties.all = { type: 'boolean', description: 'Include read as well as unread signals.' };
  }
  for (const [from, to] of Object.entries(canonicalNames)) {
    if (properties[from] && !properties[to]) properties[to] = properties[from];
    delete properties[from];
  }
  const allowed = CLI_ALLOWED[commandName];
  if (allowed) {
    // These are presentation options consumed by the root CLI, not domain inputs.
    if (allowed.includes('full')) properties.full ??= { type: 'boolean', description: 'Include full row details.' };
    if (allowed.includes('limit')) properties.limit ??= { type: 'integer', minimum: 1, maximum: 200, description: 'Maximum rows returned.' };
    for (const property of Object.keys(properties)) {
      if (!allowed.includes(property)) delete properties[property];
    }
  }
  return canonicalNames;
}
