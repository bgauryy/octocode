import { openAwarenessStore } from './open.js';

export interface ExternalAwarenessTaskActivity {
  taskId: string;
  title: string;
  state: 'doing' | 'ready';
  agentId?: string;
}

export interface ExternalAwarenessStatus {
  activePlans: number;
  readyTasks: number;
  inProgressTasks: number;
  verifyTasks: number;
  lockCount: number;
  workCount: number;
  agentCount: number;
  messageCount: number;
  taskActivities?: ExternalAwarenessTaskActivity[];
  lastMessage?: { from: string; to: string; preview: string };
  unreadInbox?: number;
  lastInbound?: { from: string; preview: string };
}

function preview(text: string): string {
  const compact = text.replace(/\s+/g, ' ').trim();
  return compact.length > 48 ? `${compact.slice(0, 47)}…` : compact;
}

/** Read the complete host-facing status projection through the typed Awareness API. */
export function readExternalAwarenessStatus(input: { workspace: string; agentId?: string }): ExternalAwarenessStatus {
  const aw = openAwarenessStore({ workspace: input.workspace });
  try {
    const status = aw.status();
    const claimed = aw.listTasks({ status: 'IN_PROGRESS' });
    const ready = aw.listReadyTasks({});
    const seen = new Set<string>();
    const taskActivities: ExternalAwarenessTaskActivity[] = [...claimed.map((task) => ({
      taskId: task.taskId,
      title: task.title,
      state: 'doing' as const,
      ...(task.agentId ? { agentId: task.agentId } : {}),
    })), ...ready.map((task) => ({ taskId: task.taskId, title: task.title, state: 'ready' as const }))]
      .filter((task) => !seen.has(task.taskId) && Boolean(seen.add(task.taskId)));
    const newest = aw.listMessages({ includeRead: true, limit: 1 })[0];
    const inbox = input.agentId
      ? aw.listMessages({ agentId: input.agentId, includeRead: false, limit: 100 })
      : [];
    const inbound = inbox[0];
    return {
      activePlans: status.activePlans,
      readyTasks: status.readyTasks,
      inProgressTasks: status.inProgressTasks,
      verifyTasks: status.verifyTasks,
      lockCount: status.locks,
      workCount: status.work,
      agentCount: status.agents,
      messageCount: status.messages,
      taskActivities,
      ...(newest ? { lastMessage: { from: newest.fromAgentId, to: newest.toAgentId ?? 'all', preview: preview(newest.text || newest.topic || '') } } : {}),
      ...(input.agentId ? { unreadInbox: inbox.length } : {}),
      ...(inbound ? { lastInbound: { from: inbound.fromAgentId, preview: preview(inbound.text || inbound.topic || '') } } : {}),
    };
  } finally {
    aw.close();
  }
}
