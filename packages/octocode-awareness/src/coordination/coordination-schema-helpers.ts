import type { HandoffNote } from '@octocodeai/agent-contracts/entities';
import { CoordinationMemoryAgents } from './coordination-memory-agents.js';
import { handoffFromRow, type HandoffRow, type AwarenessSchema } from './coordination-shared.js';
import { AWARENESS_CONCEPTS, listAwarenessOperationDescriptors } from '../schema/operation-catalog.js';

function canonicalCommands(): Record<string, string[]> {
  const descriptors = listAwarenessOperationDescriptors();
  return Object.fromEntries(AWARENESS_CONCEPTS.map(concept => [
    concept,
    descriptors.filter(row => row.concept === concept).map(row => row.operation.slice(concept.length + 1)),
  ]));
}

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
      commands: canonicalCommands(),
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
    const row = this.db.prepare(`SELECT signal_id AS handoff_id, from_agent AS agent_id,
      subject AS summary, files_json, created_at, resolved_at AS cleared_at
      FROM signals WHERE workspace_path = ? AND signal_id = ? AND kind = 'handoff'`)
      .get(this.workspace, handoffId) as unknown as HandoffRow | undefined;
    if (!row) throw new Error(`handoff not found: ${handoffId}`);
    return handoffFromRow(row);
  }

  protected countOpenHandoffs(): number {
    return (this.db.prepare("SELECT COUNT(*) AS count FROM signals WHERE workspace_path = ? AND kind = 'handoff' AND status = 'open'").get(this.workspace) as { count: number }).count;
  }

  protected countMemories(): number {
    return (this.db.prepare('SELECT COUNT(*) AS count FROM awareness_memories WHERE workspace_path = ?').get(this.workspace) as { count: number }).count;
  }

  protected countSignals(): number {
    return (this.db.prepare('SELECT COUNT(*) AS count FROM signals WHERE workspace_path = ?').get(this.workspace) as { count: number }).count;
  }
}
