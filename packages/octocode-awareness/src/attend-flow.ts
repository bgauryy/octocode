/** Read-first routing. These decisions advise the host; they never execute actions. */
export interface AttendNext {
  action: 'verify_owned_work' | 'inspect_lock' | 'inspect_overlap' | 'resume_owned_task'
    | 'inspect_inbox' | 'revalidate_memory' | 'inspect_ready_task' | 'continue'
    | 'inspect_context_headroom' | 'inspect_recent_tool_failures';
  reason: string;
  target?: { run_id?: string; task_id?: string; file?: string };
  command?: { name: 'verify audit' | 'work show' | 'signal list' | 'task show'; args: string[] };
}

interface FlowInput {
  databasePath: string;
  workspacePath: string;
  artifact?: string | null;
  agentId: string;
  verificationRequired: boolean;
  verificationRunId?: string;
  inspection?: { file: string; locked: boolean };
  runtimeActions: string[];
  ownedTaskId?: string;
  ownedRunId?: string;
  peerFile?: string;
  inboxCount: number;
  hasEvidence: boolean;
  readyTaskId?: string;
}

export function decideNext(input: FlowInput): AttendNext {
  // Literal argv preserves paths and identities without shell interpolation.
  // A subprocess cannot reconnect to this connection's in-memory database.
  const command = (name: NonNullable<AttendNext['command']>['name'], extra: string[] = []): Pick<AttendNext, 'command'> => {
    if (input.databasePath === ':memory:') return {};
    return { command: { name, args: [
      '--db', input.databasePath, '--workspace', input.workspacePath,
      ...(input.artifact ? ['--artifact', input.artifact] : []),
      ...(input.agentId ? ['--agent-id', input.agentId] : []),
      ...extra, '--compact',
    ] } };
  };
  if (input.verificationRequired) return {
    action: 'verify_owned_work', reason: 'Inspect owned debt and run the declared checks before recording a receipt.',
    ...(input.verificationRunId ? { target: { run_id: input.verificationRunId } } : {}),
    ...command('verify audit', []),
  };
  if (input.inspection) return {
    action: input.inspection.locked ? 'inspect_lock' : 'inspect_overlap',
    reason: 'Inspect the scoped peer work before interacting edits.', target: { file: input.inspection.file },
    ...command('work show', ['--file', input.inspection.file]),
  };
  if (input.runtimeActions.includes('inspect_context_headroom')) return {
    action: 'inspect_context_headroom', reason: 'Inspect host context headroom before expanding the working set.',
  };
  if (input.runtimeActions.includes('inspect_recent_tool_failures')) return {
    action: 'inspect_recent_tool_failures', reason: 'Inspect recent tool failures before retrying the same mechanism.',
  };
  if (input.ownedTaskId) return {
    action: 'resume_owned_task', reason: 'Resume the owned task; renew its lease only while work is active.',
    target: { task_id: input.ownedTaskId, ...(input.ownedRunId ? { run_id: input.ownedRunId } : {}) },
  };
  if (input.peerFile) return {
    action: 'inspect_overlap', reason: 'Read peer work before choosing an overlapping edit.', target: { file: input.peerFile },
    ...command('work show', ['--file', input.peerFile]),
  };
  if (input.inboxCount > 0) return {
    action: 'inspect_inbox', reason: 'Read the scoped inbox and resolve its relevant prerequisite.',
    ...command('signal list', ['--limit', '3']),
  };
  if (input.hasEvidence || input.runtimeActions.includes('revalidate_memory')) return {
    action: 'revalidate_memory', reason: 'Check recalled references against current source before relying on them.',
  };
  if (input.readyTaskId) return {
    action: 'inspect_ready_task', reason: 'Inspect scope and prerequisites before deciding whether to claim.',
    target: { task_id: input.readyTaskId }, ...command('task show', ['--task-id', input.readyTaskId]),
  };
  return { action: 'continue', reason: 'Continue the authorized task; no actionable shared prerequisite was observed.' };
}
