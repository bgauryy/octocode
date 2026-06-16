import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  createCallItemKey,
  gatherIncomingCallsRecursive,
  gatherOutgoingCallsRecursive,
} from '../../../octocode-tools-core/src/tools/lsp/shared/callHierarchyTraversal.js';
import type { CallHierarchyItem } from 'octocode-lsp/types';

const makeItem = (name: string, uri: string, line = 0): CallHierarchyItem => ({
  name,
  kind: 'function',
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

  it('returns empty result when client is null', async () => {
    const item = makeItem('fn', '/a.ts');
    const result = await gatherIncomingCallsRecursive(
      null,
      item,
      2,
      new Set(),
      0
    );
    expect(result.calls).toEqual([]);
    expect(result.truncatedByDepth).toBe(false);
    expect(result.cycleCount).toBe(0);
    expect(result.failedRequestCount).toBe(0);
  });

  it('returns empty result when remainingDepth is 0', async () => {
    const item = makeItem('fn', '/a.ts');
    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      0,
      new Set(),
      0
    );
    expect(result.calls).toEqual([]);
    expect(result.truncatedByDepth).toBe(false);
    expect(mockClient.getIncomingCalls).not.toHaveBeenCalled();
  });

  it('returns direct calls at depth 1 with truncatedByDepth=true when calls exist', async () => {
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

    expect(result.calls).toEqual([call]);
    expect(result.truncatedByDepth).toBe(true);
    expect(result.cycleCount).toBe(0);
    expect(result.failedRequestCount).toBe(0);
    expect(mockClient.getIncomingCalls).toHaveBeenCalledTimes(1);
  });

  it('returns truncatedByDepth=false at depth 1 when no calls exist', async () => {
    mockClient.getIncomingCalls.mockResolvedValue([]);
    const item = makeItem('fn', '/a.ts');
    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      0
    );
    expect(result.calls).toEqual([]);
    expect(result.truncatedByDepth).toBe(false);
  });

  it('recurses and deduplicates by key', async () => {
    const grandparent = makeItem('gp', '/c.ts', 20);
    const parent = makeItem('parent', '/b.ts', 10);

    const gpCall = { from: grandparent, fromRanges: [] };
    const parentCall = { from: parent, fromRanges: [] };

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

    expect(result.calls).toHaveLength(2);
    expect(result.calls).toContain(parentCall);
    expect(result.calls).toContain(gpCall);
  });

  it('counts cycleCount when a visited node would have been recursed', async () => {
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

    expect(result.calls).toContain(call);
    expect(result.cycleCount).toBe(1);
    expect(mockClient.getIncomingCalls).toHaveBeenCalledTimes(1);
  });

  it('counts failedRequestCount and returns empty on getIncomingCalls throw', async () => {
    mockClient.getIncomingCalls.mockRejectedValue(new Error('LSP error'));
    const item = makeItem('fn', '/a.ts');
    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      0
    );
    expect(result.calls).toEqual([]);
    expect(result.failedRequestCount).toBe(1);
    expect(result.truncatedByDepth).toBe(false);
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

    expect(result.calls).toHaveLength(1);
    expect(
      (result.calls[0] as { from: CallHierarchyItem }).from.content
    ).toContain('line');
  });

  it('anchors content preview on the call-site line from fromRanges, not the function start', async () => {
    const caller = makeItem('callerFn', filePath, 0); // function starts at line 0
    const callSiteRange = {
      start: { line: 3, character: 0 },
      end: { line: 3, character: 5 },
    };
    const call = { from: caller, fromRanges: [callSiteRange] };
    mockClient.getIncomingCalls.mockResolvedValue([call]);

    const item = makeItem('fn', filePath);
    const result = await gatherIncomingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      1 // contextLines
    );

    expect(result.calls).toHaveLength(1);
    const preview =
      (result.calls[0] as { from: CallHierarchyItem }).from.content ?? '';
    expect(preview).toContain('line3');
    expect(preview).toMatch(/>\s+4\| line3/); // 1-based line 4 = 0-based line 3
    expect(preview).not.toContain('line0');
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

    expect(result.calls).toHaveLength(1);
    expect(
      (result.calls[0] as { from: CallHierarchyItem }).from.content
    ).toBeUndefined();
  });
});

describe('gatherOutgoingCallsRecursive', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns empty result when client is null', async () => {
    const item = makeItem('fn', '/a.ts');
    const result = await gatherOutgoingCallsRecursive(
      null,
      item,
      2,
      new Set(),
      0
    );
    expect(result.calls).toEqual([]);
    expect(result.truncatedByDepth).toBe(false);
    expect(result.cycleCount).toBe(0);
    expect(result.failedRequestCount).toBe(0);
  });

  it('returns empty result when remainingDepth is 0', async () => {
    const item = makeItem('fn', '/a.ts');
    const result = await gatherOutgoingCallsRecursive(
      mockClient as never,
      item,
      0,
      new Set(),
      0
    );
    expect(result.calls).toEqual([]);
  });

  it('returns direct outgoing calls at depth 1 with truncatedByDepth=true when calls exist', async () => {
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

    expect(result.calls).toEqual([call]);
    expect(result.truncatedByDepth).toBe(true);
  });

  it('returns truncatedByDepth=false at depth 1 when no outgoing calls exist', async () => {
    mockClient.getOutgoingCalls.mockResolvedValue([]);
    const item = makeItem('fn', '/a.ts');
    const result = await gatherOutgoingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      0
    );
    expect(result.calls).toEqual([]);
    expect(result.truncatedByDepth).toBe(false);
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

    expect(result.calls).toHaveLength(2);
    expect(result.calls).toContain(calleeCall);
    expect(result.calls).toContain(deepCall);
  });

  it('counts cycleCount when a visited callee would have been recursed', async () => {
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

    expect(result.calls).toContain(call);
    expect(result.cycleCount).toBe(1);
    expect(mockClient.getOutgoingCalls).toHaveBeenCalledTimes(1);
  });

  it('counts failedRequestCount and returns empty on getOutgoingCalls throw', async () => {
    mockClient.getOutgoingCalls.mockRejectedValue(new Error('LSP error'));
    const item = makeItem('fn', '/a.ts');
    const result = await gatherOutgoingCallsRecursive(
      mockClient as never,
      item,
      1,
      new Set(),
      0
    );
    expect(result.calls).toEqual([]);
    expect(result.failedRequestCount).toBe(1);
    expect(result.truncatedByDepth).toBe(false);
  });
});
