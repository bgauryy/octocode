import { isAbsolute, resolve } from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { getMemory, insertMemoryWithSimilarityGate } from './memory.js';
import type { InsertMemoryResult } from './types.js';
import { defaultImportance, normalizeSupersedes, optionalQuery, requireText, scopeReferences, stringArray } from './tool-operations-shared.js';
import type { AwarenessToolOperation, AwarenessToolOperationContext, AwarenessToolOperationResult } from './tool-operations.js';

export function runMemoryOperation(
  db: DatabaseSync,
  operation: AwarenessToolOperation,
  request: Record<string, unknown>,
  context: AwarenessToolOperationContext,
): AwarenessToolOperationResult | null {
  const cwd = context.cwd ?? process.cwd();
  const agentId = context.agentId ?? 'agent';
  switch (operation) {
case 'recall': {
      const rawRefs = request['references'];
      const recallRefs = Array.isArray(rawRefs) ? rawRefs as string[] : rawRefs ? [String(rawRefs)] : [];
      const rawRegex = request['regex'];
      const recallRegex = Array.isArray(rawRegex) ? rawRegex as string[] : rawRegex ? [String(rawRegex)] : [];
      const rawFileRegex = request['file_regex'];
      const recallFileRegex = Array.isArray(rawFileRegex) ? rawFileRegex as string[] : rawFileRegex ? [String(rawFileRegex)] : [];
      const rawLabels = request['labels'] ?? request['label'];
      const recallLabels = Array.isArray(rawLabels) ? rawLabels.map(String) : rawLabels ? [String(rawLabels)] : undefined;
      const rawStates = request['states'] ?? request['state'];
      const recallStates = Array.isArray(rawStates) ? rawStates.map(String) : rawStates ? [String(rawStates)] : undefined;
      const result = getMemory(db, {
        query: optionalQuery(request),
        limit: (request['limit'] as number | undefined) ?? 3,
        minImportance: request['min_importance'] as number | undefined,
        label: recallLabels,
        tags: stringArray(request['tags']),
        smart: request['smart'] as boolean | undefined,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        globalOnly: request['global_only'] as boolean | undefined,
        strictScope: request['strict_scope'] as boolean | undefined,
        sort: request['sort'] as string | undefined,
        states: recallStates,
        references: recallRefs.length > 0 ? recallRefs : undefined,
        regex: recallRegex.length > 0 ? recallRegex : undefined,
        fileRegex: recallFileRegex.length > 0 ? recallFileRegex : undefined,
        files: (() => {
          const merged = [...stringArray(request['files'])];
          if (typeof request['file'] === 'string' && request['file']) merged.push(request['file']);
          return merged.length > 0 ? merged : undefined;
        })(),
        asOf: request['as_of'] as string | undefined ?? null,
        explain: Boolean(request['explain']),
        cwd,
      });
      type MemRecord = {
        memory_id: string;
        observation?: string;
        task_context?: string;
        label?: string;
        importance?: number;
        score?: number;
        tags?: string[];
        references?: string[];
        failure_signature?: string;
        repo?: string;
        ref?: string;
      };
      const memories = (result.memories as MemRecord[]).map((m) => {
        const lean: Record<string, unknown> = {
          memory_id: m.memory_id,
          observation: m.observation,
          task_context: m.task_context,
          label: m.label,
          importance: m.importance,
          score: Math.round((m.score ?? 0) * 100) / 100,
        };
        if (m.tags?.length) lean['tags'] = m.tags;
        if (m.references?.length) lean['references'] = m.references;
        if (m.failure_signature) lean['failure_signature'] = m.failure_signature;
        if (m.repo) lean['repo'] = m.repo;
        if (m.ref) lean['ref'] = m.ref;
        const requestedFile = typeof request['file'] === 'string' ? resolve(cwd, request['file']) : null;
        const fileRefs = (m.references ?? [])
          .filter((ref) => ref.startsWith('file:') && !ref.startsWith('file://'))
          .map((ref) => ref.slice('file:'.length));
        const fileRef = requestedFile
          ? fileRefs.find((ref) => (isAbsolute(ref) ? resolve(ref) : resolve(cwd, ref)) === requestedFile) ?? fileRefs[0]
          : fileRefs[0];
        if (fileRef) lean['file'] = isAbsolute(fileRef) ? fileRef : resolve(cwd, fileRef);
        return lean;
      });
      const payload: Record<string, unknown> = { count: result.count, memories };
      if (result.count === 0) return { payload, exitCode: 0 };
      if (result.judgment_required) {
        payload['judgment_required'] = true;
        payload['judgment_reason'] = result.judgment_reason;
      }
      if (result.smart_expanded) {
        payload['smart_expanded'] = true;
        payload['smart_dropped_filters'] = result.smart_dropped_filters ?? [];
      }
      return { payload, exitCode: 0 };
    }
case 'record': {
      const taskContext = requireText(request, 'task_context', operation);
      const observation = requireText(request, 'observation', operation);
      const label = ((request['label'] as string | undefined)?.toUpperCase()) ?? 'OTHER';
      const supersedes = normalizeSupersedes(request['supersedes']);
      const memoryWorkspace = (request['workspace_path'] as string | undefined) ?? cwd;
      const guarded = insertMemoryWithSimilarityGate(db, {
        agentId,
        taskContext,
        observation,
        importance: (request['importance'] as number | undefined) ?? defaultImportance(label),
        label,
        tags: stringArray(request['tags']),
        references: scopeReferences(request),
        supersedes,
        failureSignature: (request['failure_signature'] as string | undefined) ?? null,
        validFrom: (request['valid_from'] as string | undefined) ?? null,
        validTo: (request['valid_to'] as string | undefined) ?? null,
        workspacePath: memoryWorkspace,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        fileTreeFingerprint: request['file_tree_fingerprint'] as string | undefined,
        cwd,
      }, request['allow_similar'] === true);
      if (guarded.skipped) {
        return {
          payload: {
            skipped: true,
            reason: 'similar_memory_exists',
            similar: guarded.similar.map((m) => ({
              memory_id: m.memory_id,
              similarity: Math.round(m.similarity * 100) / 100,
            })),
            next: 'Do not record a duplicate. Pass supersedes with stale id(s), or allow_similar:true only for distinct new evidence.',
          },
          exitCode: 0,
        };
      }
      const { memory, superseded } = guarded.result as InsertMemoryResult;
      const payload: Record<string, unknown> = {
        memory_id: memory.memory_id,
        importance: memory.importance,
        label: memory.label,
      };
      if (typeof memory.novelty_score === 'number') {
        payload['novelty'] = Math.round(memory.novelty_score * 100) / 100;
      }
      if (guarded.similar.length) payload['similar'] = guarded.similar.map((m) => m.memory_id);
      if (superseded.length) payload['superseded'] = superseded;
      return { payload, exitCode: 0 };
    }
  }
  return null;
}
