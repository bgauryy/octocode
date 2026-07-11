import type { DatabaseSync } from 'node:sqlite';
import { exportHarness } from './maintenance.js';
import { forgetMemory, mineWeakness } from './memory.js';
import { reflect as reflectMemory } from './reflect.js';
import type { ReflectionOutcome } from './types.js';
import { requireText } from './tool-operations-shared.js';
import type { AwarenessToolOperation, AwarenessToolOperationContext, AwarenessToolOperationResult } from './tool-operations.js';

export function runLearningOperation(
  db: DatabaseSync,
  operation: AwarenessToolOperation,
  request: Record<string, unknown>,
  context: AwarenessToolOperationContext,
): AwarenessToolOperationResult | null {
  const cwd = context.cwd ?? process.cwd();
  const agentId = context.agentId ?? 'agent';
  switch (operation) {
case 'reflect': {
      if (
        !request['lesson'] &&
        !request['didnt_work'] &&
        !request['fix_repo'] &&
        !request['fix_harness'] &&
        !request['fix_instructions'] &&
        !request['failure_signature']
      ) {
        throw new Error(
          'reflect needs a reusable lesson, failure, fix_repo, fix_harness, fix_instructions, or failure_signature; skip routine status',
        );
      }
      const rawOutcome = request['outcome'];
      const outcome = (rawOutcome ?? 'partial') as ReflectionOutcome;
      const result = reflectMemory(db, {
        agentId,
        task: requireText(request, 'task', operation),
        outcome,
        lesson: request['lesson'] as string | undefined,
        worked: request['worked'] as string | undefined,
        didntWork: request['didnt_work'] as string | undefined,
        fixRepo: request['fix_repo'] as string | undefined,
        fixHarness: request['fix_harness'] as string | undefined,
        fixInstructions: request['fix_instructions'] as string | undefined,
        failureSignature: request['failure_signature'] as string | undefined,
        importance: request['importance'] as number | undefined,
        judgmentNote: request['judgment_note'] as string | undefined,
        duo: Boolean(request['duo']),
        allowSimilar: Boolean(request['allow_similar']),
        evalFailures: Array.isArray(request['eval_failures'])
          ? request['eval_failures'] as Array<{ id: string; dimension?: string; failure_signature?: string; suggested_lesson?: string }>
          : undefined,
        references: request['references'] as string[] | undefined,
        file: request['file'] as string | undefined,
        files: request['files'] as string[] | undefined,
        folders: request['folders'] as string[] | undefined,
        validFrom: (request['valid_from'] as string | undefined) ?? null,
        validTo: (request['valid_to'] as string | undefined) ?? null,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        cwd,
      }) as unknown as {
        outcome: string;
        learning_memory_id: string;
        learning_memory_skipped?: true;
        novelty_score?: number;
        similar_memory_ids?: string[];
        repo_fix_refinement_id?: string;
        harness_fix?: boolean;
        instructions_feedback?: boolean;
        developer_review_refinement_id?: string;
        eval_failure_count?: number;
        eval_failure_ids?: string[];
        reflection_duo?: unknown;
        next: string;
      };
      const payload: Record<string, unknown> = {
        outcome: result.outcome,
        memory_id: result.learning_memory_id,
      };
      if (result.learning_memory_skipped) payload['memory_skipped'] = true;
      if (typeof result.novelty_score === 'number' && result.novelty_score < 0.75) {
        payload['novelty'] = Math.round(result.novelty_score * 100) / 100;
      }
      if (result.similar_memory_ids?.length) payload['similar'] = result.similar_memory_ids;
      if (result.eval_failure_count) {
        payload['eval_failure_count'] = result.eval_failure_count;
        payload['eval_failure_ids'] = result.eval_failure_ids;
      }
      if (result.reflection_duo) payload['reflection_duo'] = result.reflection_duo;
      if (result.repo_fix_refinement_id) {
        payload['refinement_id'] = result.repo_fix_refinement_id;
      }
      if (result.harness_fix) {
        payload['harness_fix'] = true;
      }
      if (result.instructions_feedback) {
        payload['instructions_feedback'] = true;
        payload['developer_review_refinement_id'] = result.developer_review_refinement_id;
      }
      payload['next'] = result.next;
      return { payload, exitCode: 0 };
    }
case 'forget': {
      const rawIds = request['memory_id'] ?? request['memory_ids'];
      const memIds = Array.isArray(rawIds) ? rawIds as string[] : rawIds ? [String(rawIds)] : [];
      const rawTags = request['tags'];
      const forgTags = Array.isArray(rawTags) ? rawTags as string[] : rawTags ? [String(rawTags)] : [];
      const result = forgetMemory(db, {
        memoryIds: memIds,
        tags: forgTags,
        before: request['before'] as string | undefined,
        maxImportance: request['max_importance'] as number | undefined,
        workspacePath: request['workspace_path'] as string | undefined,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        dryRun: Boolean(request['dry_run']),
      });
      return { payload: result, exitCode: 0 };
    }
case 'mine_weakness': {
      const result = mineWeakness(db, {
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        agentId: request['agent_id'] ? String(request['agent_id']) : undefined,
        minCount: (request['min_count'] as number | undefined) ?? 2,
        limit: (request['limit'] as number | undefined) ?? 10,
        cwd,
      });
      const clusters = result.clusters.map(c => ({
        signature: c.failure_signature,
        count: c.count,
        avg_importance: c.avg_importance,
        score: c.score,
        memory_ids: c.memory_ids,
        representative: c.representative,
        labels: c.labels,
      }));
      return {
        payload: {
          total_signatures: result.total_signatures,
          total_memories: result.total_memories,
          count: clusters.length,
          clusters,
          next: result.next,
        },
        exitCode: 0,
      };
    }
case 'export_harness': {
      const result = exportHarness(db, {
        limit: (request['limit'] as number | undefined) ?? 10,
        min_importance: (request['min_importance'] as number | undefined) ?? 7,
        workspace_path: (request['workspace_path'] as string | undefined) ?? cwd,
        harness_only: Boolean(request['harness_only']),
      }) as unknown as { count: number; harness_count?: number; markdown: string; memories: Array<{ memory_id: string; label: string; importance: number; tier?: number; observation: string }>; next: string };
      const payload: Record<string, unknown> = {
        count: result.count,
        harness_count: result.harness_count,
        memories: result.memories.map(m => ({
          memory_id: m.memory_id,
          label: m.label,
          importance: m.importance,
          tier: m.tier,
          observation: m.observation.slice(0, 200),
        })),
        markdown: result.markdown,
        next: result.next,
      };
      return { payload, exitCode: 0 };
    }
  }
  return null;
}
