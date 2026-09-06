import type { DatabaseSync } from 'node:sqlite';
import { attendAwareness } from './attend-query.js';
import { queryAwareness, writeAwarenessView } from './repo-query.js';
import type { AwarenessToolOperation, AwarenessToolOperationContext, AwarenessToolOperationResult } from './tool-operations.js';

export function runRepositoryOperation(
  db: DatabaseSync,
  operation: AwarenessToolOperation,
  request: Record<string, unknown>,
  context: AwarenessToolOperationContext,
): AwarenessToolOperationResult | null {
  const cwd = context.cwd ?? process.cwd();
  switch (operation) {
case 'query': {
      const result = queryAwareness(db, {
        view: request['view'] as string | undefined,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        query: request['query'] as string | undefined,
        limit: request['limit'] as number | undefined,
        agentId: request['agent_id'] as string | undefined,
        state: request['state'] as string | string[] | undefined,
        label: request['label'] as string | string[] | undefined,
        file: request['file'] as string | undefined,
        since: request['since'] as string | undefined,
        includeBodies: request['include_bodies'] as boolean | undefined,
        cwd,
      });
      return { payload: result, exitCode: 0 };
    }
case 'attend': {
      const result = attendAwareness(db, {
        agentId: context.agentId,
        ...(context.runtimeObservation === undefined ? {} : { runtimeObservation: context.runtimeObservation }),
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        query: request['query'] as string | undefined,
        limit: request['limit'] as number | undefined,
        file: request['file'] as string[] | string | undefined,
        includeBodies: request['include_bodies'] as boolean | undefined,
        explainOrgan: request['explain_organ'] as boolean | undefined,
        compact: request['compact'] as boolean | undefined,
        cwd,
      });
      return { payload: result, exitCode: 0 };
    }
case 'view': {
      const result = writeAwarenessView(db, {
        view: request['view'] as string | undefined,
        workspacePath: (request['workspace_path'] as string | undefined) ?? cwd,
        artifact: request['artifact'] as string | undefined,
        repo: request['repo'] as string | undefined,
        ref: request['ref'] as string | undefined,
        query: request['query'] as string | undefined,
        limit: request['limit'] as number | undefined,
        out: request['out'] as string | undefined,
        cwd,
      });
      return { payload: result, exitCode: 0 };
    }
  }
  return null;
}
