import type { HandoffNote } from '@octocodeai/octocode-shared/entities';
import { CoordinationMemoryAgents } from './coordination-memory-agents.js';
import { handoffFromRow, type HandoffRow, type AwarenessSchema } from './coordination-shared.js';
import { CANONICAL_CLI_COMMANDS } from '../schema/command-catalog.js';

export abstract class AwarenessSchemaHelpers extends CoordinationMemoryAgents {
  schema(): AwarenessSchema {
    return {
      entities: {
        plan: ['planId', 'title', 'goal', 'status', 'sourceKind', 'sourceKey', 'rfcPath', 'rfcRevision', 'createdAt', 'updatedAt'],
        task: ['taskId', 'planId', 'title', 'filePath', 'paths', 'reasoning', 'acceptance', 'checkCommand', 'status', 'priority', 'dependencies', 'agentId', 'claimedAt', 'leaseExpiresAt', 'doneAt', 'verifiedAt', 'verifiedBy', 'verificationMessage', 'sourceStepKey'],
        lock: ['filePath', 'agentId', 'reason', 'acquiredAt', 'expiresAt'],
        work: ['filePath', 'agentId', 'reason', 'startedAt', 'updatedAt', 'expiresAt'],
        handoff: ['handoffId', 'agentId', 'summary', 'files', 'createdAt', 'clearedAt'],
        memory: ['memoryId', 'label', 'text', 'tags', 'createdAt', 'similarity?', 'verifiedAt?', 'validUntil?', 'scope?', 'sourceDigest?', 'explanation?'],
        agent: ['agentId', 'name', 'role', 'status', 'metadata', 'createdAt', 'lastSeenAt'],
        message: ['messageId', 'fromAgentId', 'toAgentId', 'topic', 'text', 'files', 'createdAt', 'readAt'],
      },
      commands: Object.fromEntries(Object.entries(CANONICAL_CLI_COMMANDS).map(([noun, actions]) => [noun, [...actions]])),
    };
  }

  schemaCommand(command?: string): unknown {
    const commands = this.schema().commands;
    if (!command || command === 'commands') return commands;
    if (command === 'list') return Object.keys(commands);
    const actions = commands[command];
    if (!actions) throw new Error(`unknown schema command: ${command}`);
    return { command, actions };
  }

  protected getHandoff(handoffId: string): HandoffNote {
    const row = this.db.prepare('SELECT * FROM handoffs WHERE workspace_path = ? AND handoff_id = ?').get(this.workspace, handoffId) as unknown as HandoffRow | undefined;
    if (!row) throw new Error(`handoff not found: ${handoffId}`);
    return handoffFromRow(row);
  }

  protected countOpenHandoffs(): number {
    return (this.db.prepare('SELECT COUNT(*) AS count FROM handoffs WHERE workspace_path = ? AND cleared_at IS NULL').get(this.workspace) as { count: number }).count;
  }

  protected countMemories(): number {
    return (this.db.prepare('SELECT COUNT(*) AS count FROM awareness_memories WHERE workspace_path = ?').get(this.workspace) as { count: number }).count;
  }

  protected countSignals(): number {
    return (this.db.prepare('SELECT COUNT(*) AS count FROM signals WHERE workspace_path = ?').get(this.workspace) as { count: number }).count;
  }
}
