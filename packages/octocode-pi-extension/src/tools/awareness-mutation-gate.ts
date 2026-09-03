import path from 'node:path';
import { extractBashWriteTargets } from './bash-tool.js';

export interface MutationToolEvent { toolName?: string; input?: Record<string, unknown> }
export interface LockQueryResult { blocked: boolean; message?: string }
export interface AwarenessMutationGateDependencies {
  storeExists(workspace: string): boolean;
  queryTarget(target: string, workspace: string, agentId: string): LockQueryResult;
  startWork(target: string, workspace: string, agentId: string): void;
  endWork(target: string, workspace: string, agentId: string): void;
  recordEdit?(target: string, workspace: string, agentId: string): void;
  warn?(message: string): void;
}

const STRUCTURED_MUTATION_TOOLS = new Set([
  'write', 'edit', 'multi_edit', 'multiedit', 'notebookedit', 'notebook_edit',
  'strreplace', 'delete', 'apply_patch', 'applypatch',
]);

function records(input: Record<string, unknown>): Record<string, unknown>[] {
  return Array.isArray(input.queries)
    ? input.queries.filter((query): query is Record<string, unknown> => Boolean(query) && typeof query === 'object')
    : [input];
}

export function extractMutationTargets(event: MutationToolEvent, cwd: string): string[] {
  const toolName = String(event.toolName ?? '').toLowerCase();
  const input = event.input ?? {};
  const targets: string[] = [];
  if (toolName === 'bash') {
    for (const query of records(input)) {
      if (typeof query.command === 'string') targets.push(...extractBashWriteTargets(query.command, cwd));
    }
  } else if (STRUCTURED_MUTATION_TOOLS.has(toolName)) {
    const add = (raw: unknown) => {
      for (const value of Array.isArray(raw) ? raw : [raw]) {
        if (typeof value === 'string' && value.trim()) targets.push(path.isAbsolute(value) ? path.normalize(value) : path.resolve(cwd, value));
      }
    };
    for (const query of records(input)) {
      for (const value of [query.path, query.filePath, query.file_path, query.paths, query.filePaths, query.file_paths]) add(value);
      const patch = typeof query.patch === 'string' ? query.patch : typeof query.command === 'string' ? query.command : '';
      for (const line of patch.split('\n')) {
        const match = line.match(/^\*\*\* (?:Add|Update|Delete) File: (.+)$/) ?? line.match(/^\*\*\* Move to: (.+)$/);
        if (match) add(match[1]);
      }
    }
  }
  return [...new Set(targets.map((target) => path.normalize(target)))];
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function createAwarenessMutationGate(deps: AwarenessMutationGateDependencies) {
  const owned = new Map<string, { target: string; workspace: string; agentId: string; activeCalls: number }>();
  const keyFor = (target: string, workspace: string, agentId: string) => `${workspace}\0${agentId}\0${target}`;
  return {
    preflight(event: MutationToolEvent, workspace: string, agentId: string): { block: true; reason: string } | undefined {
      const targets = extractMutationTargets(event, workspace);
      if (targets.length === 0) return undefined;

      try {
        if (deps.storeExists(workspace)) {
          // Complete the entire lock pass before creating any advisory work row.
          for (const target of targets) {
            const result = deps.queryTarget(target, workspace, agentId);
            if (result.blocked) return { block: true, reason: result.message ?? `Awareness lock conflict: ${target}` };
          }
        }
      } catch (error) {
        return { block: true, reason: `Awareness store query failed: ${errorMessage(error)}` };
      }

      for (const target of targets) {
        try {
          deps.startWork(target, workspace, agentId);
          const key = keyFor(target, workspace, agentId);
          const existing = owned.get(key);
          owned.set(key, {
            target,
            workspace,
            agentId,
            activeCalls: (existing?.activeCalls ?? 0) + 1,
          });
        } catch (error) {
          deps.warn?.(`Awareness presence update failed: ${errorMessage(error)}`);
        }
      }
      return undefined;
    },
    complete(event: MutationToolEvent, workspace: string, agentId: string, succeeded: boolean): void {
      for (const target of extractMutationTargets(event, workspace)) {
        const key = keyFor(target, workspace, agentId);
        const item = owned.get(key);
        if (!item) continue;
        if (succeeded && deps.recordEdit) {
          try { deps.recordEdit(target, workspace, agentId); }
          catch (error) { deps.warn?.(`Awareness edit receipt failed: ${errorMessage(error)}`); }
        }
        if (item.activeCalls > 1) {
          item.activeCalls -= 1;
          continue;
        }
        try {
          deps.endWork(target, workspace, agentId);
          owned.delete(key);
        } catch (error) {
          deps.warn?.(`Awareness presence cleanup failed: ${errorMessage(error)}`);
        }
      }
    },
    cleanup(): void {
      for (const item of owned.values()) {
        try { deps.endWork(item.target, item.workspace, item.agentId); }
        catch (error) { deps.warn?.(`Awareness presence cleanup failed: ${errorMessage(error)}`); }
      }
      owned.clear();
    },
  };
}
