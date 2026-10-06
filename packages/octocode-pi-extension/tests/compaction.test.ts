import { describe, expect, it } from 'vitest';
import { KEEP_RECENT_MIN, KEEP_RECENT_RESULTS, TRIM_STEP, addFileToolOps, planContextTrims, registerCompaction } from '../src/compaction/register.js';

describe('compaction', () => {
  const entry = (index: number, size: number) => ({
    id: `e${index}`,
    message: { role: 'toolResult', toolCallId: `c${index}`, content: [{ type: 'text', text: 'x'.repeat(size) }] },
  });

  it('waits for a batch that saves enough, then trims it with context edits', () => {
    // Medium results: the 12-result window holds them all; 9 older ones are not a batch yet.
    const few = Array.from({ length: KEEP_RECENT_RESULTS + TRIM_STEP - 1 }, (_, index) => entry(index, 3_500));
    expect(planContextTrims(few)).toEqual([]);
    // Ten barely-large results save too little to be worth a cache rewrite.
    expect(planContextTrims(Array.from({ length: KEEP_RECENT_RESULTS + TRIM_STEP }, (_, index) => entry(index, 3_100)))).toEqual([]);

    const many = Array.from({ length: KEEP_RECENT_RESULTS + TRIM_STEP }, (_, index) => entry(index, 3_500));
    const edits = planContextTrims(many);
    expect(edits.map((edit) => edit.targetId)).toEqual(Array.from({ length: TRIM_STEP }, (_, index) => `e${index}`));
    const text = (edits[0]!.replacement as { content: Array<{ text: string }> }).content[0]!.text;
    expect(text.length).toBeLessThan(2_000);
    expect(text).toMatch(/were trimmed/);
  });

  it('shrinks the large arguments of old successful calls, keeping failed and recent ones', () => {
    const content = 'w'.repeat(40_000);
    const call = (index: number) => ({
      id: `a${index}`,
      message: { role: 'assistant', content: [{ type: 'thinking', thinking: 'plan' }, { type: 'toolCall', id: `c${index}`, name: 'file', arguments: { queries: [{ type: 'write', path: `f${index}.ts`, content }] } }] },
    });
    const result = (index: number, isError = false) => ({ id: `r${index}`, message: { role: 'toolResult', toolCallId: `c${index}`, isError, content: [{ type: 'text', text: 'ok' }] } });
    const entries = [call(0), result(0), call(1), result(1, true), call(2), result(2), ...Array.from({ length: KEEP_RECENT_RESULTS }, (_, index) => [call(10 + index), result(10 + index)]).flat()];
    const edits = planContextTrims(entries);
    expect(edits.map((edit) => edit.targetId)).toEqual(['a0', 'a2']);
    const parts = (edits[0]!.replacement as unknown as { content: Array<Record<string, unknown>> }).content;
    expect(parts[0]).toEqual({ type: 'thinking', thinking: 'plan' });
    const query = (parts[1]!['arguments'] as { queries: Array<Record<string, string>> }).queries[0]!;
    expect(query['path']).toBe('f0.ts');
    expect(query['content']!.length).toBeLessThan(300);
    expect(query['content']).toMatch(/earlier, successful call omitted/);
    // Idempotent: once applied, the shrunk calls are not proposed again.
    const replaced = (id: string) => edits.find((edit) => edit.targetId === id)?.replacement as { content: unknown[] } | undefined;
    const applied = entries.map((entry) => (replaced(entry.id) ? { id: entry.id, message: { ...entry.message, content: replaced(entry.id)!.content } } : entry));
    expect(planContextTrims(applied)).toEqual([]);
  });

  it('shrinks successful file changes even in the recent window, since their text is on disk', () => {
    const content = 'w'.repeat(70_000);
    const call = (index: number, name: string) => ({
      id: `a${index}`,
      message: { role: 'assistant', content: [{ type: 'toolCall', id: `c${index}`, name, arguments: { queries: [{ type: 'write', path: `f${index}.ts`, content }] } }] },
    });
    const result = (index: number, toolName: string, isError = false) => ({ id: `r${index}`, message: { role: 'toolResult', toolName, toolCallId: `c${index}`, isError, content: [{ type: 'text', text: 'ok' }] } });
    const entries = [call(0, 'file'), result(0, 'file'), call(1, 'file'), result(1, 'file', true), call(2, 'bash'), result(2, 'bash')];
    expect(planContextTrims(entries).map((edit) => edit.targetId)).toEqual(['a0']);
  });

  it('bounds the recent window by size, so a few huge results are trimmed without waiting for a batch', () => {
    const huge = Array.from({ length: KEEP_RECENT_MIN + 2 }, (_, index) => entry(index, 50_000));
    expect(planContextTrims(huge).map((edit) => edit.targetId)).toEqual(['e0', 'e1']);
    // Only the minimum window is ever kept when each result alone exceeds the budget.
    expect(planContextTrims(huge.slice(0, KEEP_RECENT_MIN))).toEqual([]);
  });

  it('sizes a result by all its text parts together', () => {
    const split = (index: number) => ({ id: `s${index}`, message: { role: 'toolResult', content: Array.from({ length: 20 }, () => ({ type: 'text', text: 'z'.repeat(2_999) })) } });
    const edits = planContextTrims(Array.from({ length: KEEP_RECENT_MIN + 2 }, (_, index) => split(index)));
    expect(edits.map((edit) => edit.targetId)).toEqual(['s0', 's1']);
    expect((edits[0]!.replacement as { content: unknown[] }).content).toHaveLength(1);
  });

  it('keeps user answers and subagent reports intact while trimming recoverable tool output', () => {
    const protectedResult = (toolName: string) => ({ id: toolName, message: { role: 'toolResult', toolName, content: [{ type: 'text', text: 'Decision and evidence '.repeat(5_000) }] } });
    const entries = [protectedResult('askUser'), protectedResult('agent'), ...Array.from({ length: 20 }, (_, index) => entry(index, 10_000))];
    const edits = planContextTrims(entries);
    expect(edits.length).toBeGreaterThan(0);
    expect(edits.some((edit) => edit.targetId === 'askUser' || edit.targetId === 'agent')).toBe(false);
  });

  it('adds the paths the file tool changed to Pi file ops (not MCP tool shapes)', () => {
    const preparation = {
      fileOps: { read: new Set(['a.ts']), written: new Set<string>(), edited: new Set(['b.ts']) },
      messagesToSummarize: [
        { role: 'assistant', content: [{ type: 'toolCall', name: 'file', arguments: { queries: [{ type: 'edit', path: 'c.ts' }, { type: 'delete', path: 'a.ts' }] } }] },
        { role: 'assistant', content: [{ type: 'toolCall', name: 'mcp__octocode__localGetFileContent', arguments: { queries: [{ path: 'd.ts' }] } }] },
      ],
      turnPrefixMessages: [{ role: 'assistant', content: [{ type: 'toolCall', name: 'file', arguments: { queries: [{ type: 'write', path: 'e.ts' }] } }] }],
    } as never as Parameters<typeof addFileToolOps>[0];
    addFileToolOps(preparation);
    expect([...preparation.fileOps.edited].sort()).toEqual(['a.ts', 'b.ts', 'c.ts', 'e.ts']);
    expect([...preparation.fileOps.read]).toEqual(['a.ts']);
  });

  it('over a long simulated session edits each entry at most once, in rare batches, and never touches failed or recent calls', () => {
    type Message = Record<string, unknown>;
    const entries: Array<{ id: string; message: Message }> = [];
    const edits = new Map<string, number>();
    const editTurns: number[] = [];
    const turns = 120;
    const body = 'w'.repeat(30_000);
    let peakChars = 0;
    for (let turn = 0; turn < turns; turn++) {
      const isFile = turn % 3 === 0;
      const failed = turn % 17 === 5;
      entries.push({
        id: `a${turn}`,
        message: { role: 'assistant', content: [{ type: 'toolCall', id: `c${turn}`, name: isFile ? 'file' : 'bash', arguments: isFile ? { queries: [{ type: 'write', path: `f${turn}.ts`, content: body }] } : { command: `echo ${turn}` } }] },
      });
      entries.push({ id: `r${turn}`, message: { role: 'toolResult', toolName: isFile ? 'file' : 'bash', toolCallId: `c${turn}`, isError: failed, content: [{ type: 'text', text: isFile ? 'ok' : 'y'.repeat(6_000) }] } });
      const planned = planContextTrims(entries);
      if (planned.length > 0) editTurns.push(turn);
      for (const edit of planned) {
        edits.set(edit.targetId, (edits.get(edit.targetId) ?? 0) + 1);
        const target = entries.find((candidate) => candidate.id === edit.targetId)!;
        const replacement = edit.replacement as { content: unknown[] };
        // Failed calls keep their arguments: a failed file write is never edited.
        if (target.id.startsWith('a')) expect(entries.find((candidate) => candidate.id === `r${target.id.slice(1)}`)!.message['isError']).toBe(false);
        // Only the newest results stay verbatim (the two entries per turn are this turn's pair).
        if (target.id.startsWith('r') && Number(target.id.slice(1)) > turn - KEEP_RECENT_MIN) throw new Error(`recent result ${target.id} was trimmed at turn ${turn}`);
        target.message = { ...target.message, content: replacement.content };
      }
      peakChars = Math.max(peakChars, JSON.stringify(entries).length);
    }
    // Append-only and idempotent: an entry is edited once, and a settled context plans nothing more.
    expect([...edits.values()].every((count) => count === 1)).toBe(true);
    expect(planContextTrims(entries)).toEqual([]);
    // Prefix stability: edits come in batches (at most every sixth turn here), not on most turns (each batch rewrites the cached prefix).
    expect(editTurns.length).toBeGreaterThan(0);
    expect(editTurns.length).toBeLessThanOrEqual(turns / 6);
    // Successful file writes are shrunk to a head plus a note, keeping their structure; failed ones stay whole.
    const write = (id: string) => (entries.find((candidate) => candidate.id === id)!.message['content'] as Array<{ arguments: { queries: Array<{ type: string; path: string; content: string }> } }>)[0]!.arguments.queries[0]!;
    expect(write('a0')).toMatchObject({ type: 'write', path: 'f0.ts' });
    expect(write('a0').content.length).toBeLessThan(400);
    expect(write('a0').content).toMatch(/omitted\]$/);
    expect(write('a39').content).toBe(body); // turn 39 is a file write whose result failed (39 % 17 === 5)
    // The context stays far below the untrimmed size (120 turns x 36k characters).
    expect(JSON.stringify(entries).length).toBeLessThan(120 * 36_000 * 0.3);
    expect(peakChars).toBeLessThan(120 * 36_000 * 0.4);
  });

  it('never trims subagent reports', () => {
    const results = Array.from({ length: KEEP_RECENT_RESULTS + TRIM_STEP }, (_, index) => entry(index, 10_000));
    results[0]!.message = { ...results[0]!.message, toolName: 'agent' } as typeof results[0]['message'];
    const edits = planContextTrims(results);
    expect(edits.map((edit) => edit.targetId)).not.toContain('e0');
  });

  it('ignores small results, images count as large, and user messages are never touched', () => {
    const small = [{ id: 'u', message: { role: 'user', content: 'hi' } }, ...Array.from({ length: 40 }, (_, index) => entry(index, 100))];
    expect(planContextTrims(small)).toEqual([]);
    const images = Array.from({ length: KEEP_RECENT_RESULTS + TRIM_STEP }, (_, index) => ({
      id: `i${index}`,
      message: { role: 'toolResult', content: [{ type: 'image', data: 'AA', mimeType: 'image/png' }] },
    }));
    expect((planContextTrims(images)[0]!.replacement as { content: unknown }).content).toEqual([{ type: 'text', text: '[image from an earlier tool call omitted]' }]);
  });
});

describe('summaries', () => {
  it('leaves compaction and /tree summaries to Pi (the user model and prompt), only adding file-tool paths', async () => {
    const handlers = new Map<string, (event: unknown, ctx: unknown) => Promise<unknown>>();
    registerCompaction({ on: (name: string, handler: never) => void handlers.set(name, handler) } as never, { reset: () => undefined } as never);
    expect(handlers.has('session_before_tree')).toBe(false);
    const preparation = {
      fileOps: { read: new Set<string>(), written: new Set<string>(), edited: new Set<string>() },
      messagesToSummarize: [{ role: 'assistant', content: [{ type: 'toolCall', name: 'file', arguments: { queries: [{ type: 'write', path: 'todo.txt' }] } }] }],
      turnPrefixMessages: [],
    };
    expect(await handlers.get('session_before_compact')!({ type: 'session_before_compact', preparation }, {})).toBeUndefined();
    expect([...preparation.fileOps.edited]).toEqual(['todo.txt']);
  });
});
