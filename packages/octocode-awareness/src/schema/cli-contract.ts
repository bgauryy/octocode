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
  'signal ack': ['agent_id', 'signal_id'], 'signal resolve': ['agent_id'],
};

const CLI_ALLOWED: Record<string, string[]> = {
  'agent touch': ['workspace', 'agent_id', 'status'], 'agent leave': ['workspace', 'agent_id'],
  'plan create': ['name', 'objective', 'lead_agent_id', 'workspace', 'artifact'], 'plan list': ['workspace', 'artifact', 'status', 'limit', 'full'],
  'plan show': ['plan_id', 'full'], 'plan join': ['plan_id', 'agent_id'], 'plan doc': ['plan_id', 'agent_id', 'path', 'title'], 'plan status': ['plan_id', 'agent_id', 'status'],
  'task create': ['plan_id', 'title', 'reasoning', 'acceptance', 'path', 'depends_on', 'agent_id', 'priority', 'lease_minutes', 'test_plan'],
  'task list': ['plan_id', 'workspace', 'status', 'limit', 'full'], 'task ready': ['plan_id', 'workspace', 'limit', 'full'], 'task show': ['task_id', 'full'],
  'task claim': ['task_id', 'plan_id', 'agent_id', 'next', 'lease_minutes'], 'task heartbeat': ['task_id', 'run_id', 'agent_id', 'lease_minutes'],
  'task submit': ['task_id', 'run_id', 'agent_id', 'message'], 'task release': ['task_id', 'run_id', 'agent_id', 'blocked_reason'],
  'task retry': ['task_id', 'agent_id', 'message'], 'task depend': ['task_id', 'depends_on', 'agent_id'],
  'work start': ['agent_id', 'session_id', 'workspace', 'artifact', 'run_id', 'rationale', 'test_plan', 'context_ref', 'file', 'exclusive', 'ttl_minutes', 'ttl_seconds'],
  'work touch': ['agent_id', 'run_id', 'file', 'ttl_minutes', 'ttl_seconds'], 'work end': ['agent_id', 'run_id', 'file'],
  'work list': ['agent_id', 'workspace', 'artifact', 'run_id', 'all', 'full'], 'work show': ['workspace', 'artifact', 'file', 'all', 'full'],
  'memory recall': ['query', 'limit', 'min_importance', 'label', 'tag', 'smart', 'workspace', 'artifact', 'repo', 'ref', 'state', 'sort', 'global_only', 'strict_scope', 'all_workspaces', 'as_of', 'reference', 'regex', 'file_regex', 'file', 'explain', 'semantic', 'full'],
  'refinement get': ['workspace', 'refinement_id', 'artifact', 'repo', 'ref', 'quality', 'include_handoffs', 'state', 'limit', 'full'],
  'lock prune': ['older_than_minutes', 'expired_only', 'agent_id', 'workspace', 'artifact', 'target_file', 'dry_run'],
  'reflect record': ['agent_id', 'task', 'outcome', 'lesson', 'worked', 'didnt_work', 'fix_repo', 'fix_file', 'fix_harness', 'fix_instructions', 'failure_signature', 'importance', 'judgment_note', 'duo', 'eval_failure_json', 'workspace', 'artifact', 'repo', 'ref', 'allow_similar'],
  'signal publish': ['agent_id', 'workspace', 'artifact', 'repo', 'ref', 'kind', 'subject', 'body', 'to_agent', 'file', 'ref_id', 'importance'],
  'signal list': ['agent_id', 'workspace', 'artifact', 'repo', 'ref', 'all', 'unread_only', 'mark_read', 'limit', 'include_bodies', 'format'],
  'signal reply': ['agent_id', 'in_reply_to', 'subject', 'body', 'to_agent', 'file', 'ref_id', 'importance'],
  'signal ack': ['agent_id', 'signal_id'], 'signal resolve': ['agent_id', 'signal_id', 'thread_id'],
};

/** Exact canonical root flags for a noun/action route, excluding global flags. */
export function cliAllowedFlags(commandName: string): readonly string[] | undefined {
  return CLI_ALLOWED[commandName];
}

function aliasesFor(commandName: string): Record<string, string> {
  return {
    workspace_path: 'workspace', target_files: 'file', tags: 'tag', references: 'reference', labels: 'label', files: 'file', states: 'state',
    eval_failures: 'eval_failure_json', ...(commandName.startsWith('lock ') ? { target_files: 'target_file' } : {}),
  };
}

export function projectCliProperties(properties: Record<string, unknown>, commandName: string): Record<string, string> {
  const aliases = aliasesFor(commandName);
  for (const [from, to] of Object.entries(aliases)) {
    if (properties[from] && !properties[to]) properties[to] = properties[from];
    delete properties[from];
  }
  const allowed = CLI_ALLOWED[commandName];
  if (allowed) {
    for (const property of Object.keys(properties)) {
      if (!allowed.includes(property)) delete properties[property];
    }
  }
  return aliases;
}
