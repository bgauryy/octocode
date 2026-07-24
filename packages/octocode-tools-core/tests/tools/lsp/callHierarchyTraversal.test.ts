import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';

import type { LSPClient } from '@octocodeai/octocode-engine/lsp/client';
import type {
  CallHierarchyItem,
  IncomingCall,
} from '@octocodeai/octocode-engine/lsp/types';
import {
  createCallItemKey,
  gatherIncomingCallsRecursive,
} from '../../../src/tools/lsp/shared/callHierarchyTraversal.js';

let tempDir: string | undefined;

afterEach(async () => {
  if (tempDir) {
    await rm(tempDir, { recursive: true, force: true });
    tempDir = undefined;
  }
});

function item(name: string, uri: string): CallHierarchyItem {
  return {
    name,
    kind: 'function' as never,
    uri,
    range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
    selectionRange: {
      start: { line: 0, character: 0 },
      end: { line: 0, character: 1 },
    },
  };
}

function fakeClient(branching: number, uri: string): Pick<LSPClient, 'getIncomingCalls'> {
  return {
    async getIncomingCalls(parent: CallHierarchyItem): Promise<IncomingCall[]> {
      return Array.from({ length: branching }, (_, index) => {
        const child = item(`${parent.name}-${index}`, uri);
        return {
          from: child,
          fromRanges: [child.range],
        };
      });
    },
  };
}

describe('call hierarchy traversal budgets', () => {
  it('caps recursive traversal by request/node budgets and reports truncation', async () => {
    tempDir = await mkdtemp(join(process.cwd(), '.tmp-octocode-callhier-'));
    const filePath = join(tempDir, 'fixture.ts');
    await writeFile(filePath, 'function root() {}\n');
    const root = item('root', filePath);

    const result = await gatherIncomingCallsRecursive(
      fakeClient(3, filePath) as LSPClient,
      root,
      5,
      new Set([createCallItemKey(root)]),
      1,
      {
        maxNodes: 2,
        maxRequests: 2,
        visitedNodeCount: 0,
        requestCount: 0,
        truncatedByBudget: false,
      }
    );

    expect(result.truncatedByBudget).toBe(true);
    expect(result.requestCount).toBeLessThanOrEqual(2);
    expect(result.visitedNodeCount).toBeLessThanOrEqual(2);
  });
});
