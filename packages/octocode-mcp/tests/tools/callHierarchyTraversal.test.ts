import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createCallItemKey,
  gatherIncomingCallsRecursive,
  gatherOutgoingCallsRecursive,
} from '../../src/tools/lsp/shared/callHierarchyTraversal.js';
import type { CallHierarchyItem } from '../../src/lsp/types.js';

const makeItem = (name: string, uri: string, line = 0): CallHierarchyItem => ({
  name,
  kind: 12,
  uri,
  range: {
    start: { line, character: 0 },
    end: { line, character: name.length },
  },
});

const mockClient = {
  getIncomingCalls: vi.fn(),
  getOutgoingCalls: vi.fn(),
};

describe('createCallItemKey', () => {
  it('returns a stable colon-separated key', () => {
    const item = makeItem('foo', '/src/a.ts', 5);
    expect(createCallItemKey(item)).toBe('/src/a.ts:5:foo');
  });
});

describe('gatherIncomingCallsRecursive', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns [] when client is null', async () => {
    const item = makeItem('fn', '/a.ts');
    const result = await gatherIncomingCallsRecursive(
      null,
      item,
      2,
      new Set(),
      0
    );
    expect(result).toEqual([]);
  });

  it('returns [] when remainingDepth is 0', async () => {
    const item = makeItem('fn', '/a.ts');
    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      0,
      new Set(),
      0
    );
    expect(result).toEqual([]);
    expect(mockClient.getIncomingCalls).not.toHaveBeenCalled();
  });

  it('returns direct calls at depth 1 (no recursion)', async () => {
    const parent = makeItem('parent', '/b.ts', 10);
    const call = { from: parent, fromRanges: [] };
    mockClient.getIncomingCalls.mockResolvedValue([call]);

    const item = makeItem('child', '/a.ts');
    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      0
    );

    expect(result).toEqual([call]);
    expect(mockClient.getIncomingCalls).toHaveBeenCalledTimes(1);
  });

  it('recurses and deduplicates by key', async () => {
    const grandparent = makeItem('gp', '/c.ts', 20);
    const parent = makeItem('parent', '/b.ts', 10);

    const gpCall = { from: grandparent, fromRanges: [] };
    const parentCall = { from: parent, fromRanges: [] };

    // depth=2: first call returns [parentCall], second (for parent) returns [gpCall]
    mockClient.getIncomingCalls
      .mockResolvedValueOnce([parentCall])
      .mockResolvedValueOnce([gpCall]);

    const item = makeItem('child', '/a.ts');
    const visited = new Set<string>();
    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      2,
      visited,
      0
    );

    expect(result).toHaveLength(2);
    expect(result).toContain(parentCall);
    expect(result).toContain(gpCall);
  });

  it('skips already-visited nodes to prevent cycles', async () => {
    const parent = makeItem('parent', '/b.ts', 10);
    const call = { from: parent, fromRanges: [] };
    mockClient.getIncomingCalls.mockResolvedValue([call]);

    const item = makeItem('child', '/a.ts');
    const visited = new Set<string>([createCallItemKey(parent)]);

    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      2,
      visited,
      0
    );

    // parentCall is in enhanced calls but not recursed into (already visited)
    expect(result).toContain(call);
    expect(mockClient.getIncomingCalls).toHaveBeenCalledTimes(1);
  });

  it('returns [] when getIncomingCalls throws', async () => {
    mockClient.getIncomingCalls.mockRejectedValue(new Error('LSP error'));
    const item = makeItem('fn', '/a.ts');
    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      0
    );
    expect(result).toEqual([]);
  });
});

describe('gatherIncomingCallsRecursive with contextLines', () => {
  let tempDir: string;
  let filePath: string;

  beforeEach(async () => {
    vi.clearAllMocks();
    tempDir = await mkdtemp(join(process.cwd(), '.tmp-call-traversal-'));
    filePath = join(tempDir, 'fixture.ts');
    await writeFile(filePath, 'line0\nline1\nline2\nline3\nline4\n');
  });

  afterEach(async () => {
    await rm(tempDir, { recursive: true, force: true });
  });

  it('enhances call items with code snippets when contextLines > 0', async () => {
    const caller = makeItem('callerFn', filePath, 2);
    const call = { from: caller, fromRanges: [] };
    mockClient.getIncomingCalls.mockResolvedValue([call]);

    const item = makeItem('fn', filePath);
    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      1
    );

    expect(result).toHaveLength(1);
    expect((result[0] as { from: CallHierarchyItem }).from.content).toContain(
      'line'
    );
  });

  it('returns item unchanged when file cannot be read (missing file)', async () => {
    const caller = makeItem('callerFn', '/nonexistent/path.ts', 0);
    const call = { from: caller, fromRanges: [] };
    mockClient.getIncomingCalls.mockResolvedValue([call]);

    const item = makeItem('fn', '/a.ts');
    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      2
    );

    expect(result).toHaveLength(1);
    expect(
      (result[0] as { from: CallHierarchyItem }).from.content
    ).toBeUndefined();
  });
});

describe('gatherOutgoingCallsRecursive', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns [] when client is null', async () => {
    const item = makeItem('fn', '/a.ts');
    const result = await gatherOutgoingCallsRecursive(
      null,
      item,
      2,
      new Set(),
      0
    );
    expect(result).toEqual([]);
  });

  it('returns [] when remainingDepth is 0', async () => {
    const item = makeItem('fn', '/a.ts');
    const result = await gatherOutgoingCallsRecursive(
      mockClient as never,
      item,
      0,
      new Set(),
      0
    );
    expect(result).toEqual([]);
  });

  it('returns direct outgoing calls at depth 1', async () => {
    const callee = makeItem('callee', '/b.ts', 5);
    const call = { to: callee, fromRanges: [] };
    mockClient.getOutgoingCalls.mockResolvedValue([call]);

    const item = makeItem('caller', '/a.ts');
    const result = await gatherOutgoingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      0
    );

    expect(result).toEqual([call]);
  });

  it('recurses for outgoing calls at depth > 1', async () => {
    const callee = makeItem('callee', '/b.ts', 5);
    const deepCallee = makeItem('deepCallee', '/c.ts', 8);

    const calleeCall = { to: callee, fromRanges: [] };
    const deepCall = { to: deepCallee, fromRanges: [] };

    mockClient.getOutgoingCalls
      .mockResolvedValueOnce([calleeCall])
      .mockResolvedValueOnce([deepCall]);

    const item = makeItem('caller', '/a.ts');
    const result = await gatherOutgoingCallsRecursive(
      mockClient as never,
      item,
      2,
      new Set(),
      0
    );

    expect(result).toHaveLength(2);
    expect(result).toContain(calleeCall);
    expect(result).toContain(deepCall);
  });

  it('skips already-visited callees', async () => {
    const callee = makeItem('callee', '/b.ts', 5);
    const call = { to: callee, fromRanges: [] };
    mockClient.getOutgoingCalls.mockResolvedValue([call]);

    const item = makeItem('caller', '/a.ts');
    const visited = new Set<string>([createCallItemKey(callee)]);

    const result = await gatherOutgoingCallsRecursive(
      mockClient as never,
      item,
      2,
      visited,
      0
    );

    expect(result).toContain(call);
    expect(mockClient.getOutgoingCalls).toHaveBeenCalledTimes(1);
  });

  it('returns [] when getOutgoingCalls throws', async () => {
    mockClient.getOutgoingCalls.mockRejectedValue(new Error('LSP error'));
    const item = makeItem('fn', '/a.ts');
    const result = await gatherOutgoingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      0
    );
    expect(result).toEqual([]);
  });
});
