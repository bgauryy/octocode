import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { carriedSavedResults, SAVED_RESULTS_DETAILS, SAVED_RESULTS_INLINE_CHARS, savedResultPaths, savedResultsBlock, withoutModelSavedResults, withoutSavedResults } from '../src/compaction/saved-results.js';
import { splitSummary, userMessagesSection, writeSummary } from '../src/compaction/summary.js';
import { JOB_TYPE } from '../src/files/bash-jobs.js';
import { jobReport } from '../src/files/bash-report.js';
import { CLEARED_RESULTS_TYPE } from '../src/files/reads.js';
import { sessionOutputDir } from '../src/shared/home.js';
import { capOutputToFile, saveContentCopy } from '../src/shared/spill.js';
import { tmp } from './helpers.js';

const model = { provider: 'p', id: 'm', api: 'anthropic-messages', maxTokens: 10_000, contextWindow: 200_000 };
const at = 1_700_000_000_000;
const usage = { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2 };

let savedHome: string | undefined;
let home: string;
beforeEach(() => {
  savedHome = process.env['OCTOCODE_HOME'];
  home = tmp();
  process.env['OCTOCODE_HOME'] = home;
});
afterEach(() => {
  vi.restoreAllMocks();
  if (savedHome === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = savedHome;
});

const result = (id: string, text: string) => ({ role: 'toolResult', toolCallId: id, toolName: 'bash', content: [{ type: 'text', text }], isError: false, timestamp: at });
/** The model-facing text of a result larger than Pi's budget: a preview whose closing note names the file. */
const trimmed = (label: string, mark: string) => capOutputToFile(`${mark}\n${'line of output\n'.repeat(6_000)}`, { label });
const fileOf = (note: string) => /Full output: (\S+);/.exec(note)![1]!;

/** `writeSummary` over a preparation holding `messages`, on a model that answers with `## Goal`. */
function summarize(messages: unknown[], options: { branch?: unknown[]; previousSummary?: string; standalone?: boolean; answer?: string } = {}) {
  const pi = { getThinkingLevel: () => 'off', getActiveTools: () => [], getSettings: () => ({}) } as never;
  const ctx = {
    model,
    modelRegistry: { streamSimple: () => ({ result: async () => ({ role: 'assistant', content: [{ type: 'text', text: options.answer ?? '## Goal\nwork' }], stopReason: 'stop', usage, timestamp: at }) }) },
    sessionManager: { getSessionId: () => 's', getSessionFile: () => undefined, getBranch: () => [], buildSessionProjection: () => ({ entries: [], messages: [] }) },
    hasUI: false,
  } as never;
  const event = {
    preparation: {
      firstKeptEntryId: 'k',
      tokensBefore: 1,
      messagesToSummarize: messages,
      turnPrefixMessages: [],
      previousSummary: options.previousSummary,
      isSplitTurn: false,
      fileOps: { read: new Set<string>(), written: new Set<string>(), edited: new Set<string>() },
      settings: { reserveTokens: 10_000 },
    },
    branchEntries: options.branch ?? [],
    signal: { aborted: false },
  } as never;
  // A summary on another model (a downshift) is the standalone request; on the session's own it is the cache-sharing one.
  return writeSummary(pi, event, ctx, options.standalone ? ({ ...model, id: 'previous' } as never) : undefined);
}

describe('saved results carried into the compaction summary', () => {
  it('names the file of every trimmed result once the result itself is summarized away', async () => {
    const first = trimmed('bash-first', 'FIRST');
    const second = trimmed('mcp-second', 'SECOND');
    const out = await summarize([result('a', first), result('b', second), result('c', second), result('d', 'short output')]);
    const files = [fileOf(first), fileOf(second)];
    const summary = out!.compaction!.summary;
    for (const file of files) {
      expect(fs.existsSync(path.resolve(file))).toBe(true);
      // Once each, though two results name the second file.
      expect(summary.split(file)).toHaveLength(2);
    }
    const block = /<saved-results>\n([\s\S]*?)\n<\/saved-results>/.exec(summary)![1]!;
    expect(block.split('\n').slice(1)).toEqual(files);
    expect(out!.compaction!.details).toMatchObject({ [SAVED_RESULTS_DETAILS]: files.map((file) => path.resolve(file)) });
    // After the file lists and before the user messages, where Pi's `<read-files>` blocks sit in the summary.
    expect(summary.startsWith('## Goal\nwork\n\n<saved-results>')).toBe(true);
  });

  it('carries them the same way when the summary is the standalone request', async () => {
    const note = trimmed('bash-standalone', 'STANDALONE');
    const out = await summarize([result('a', note)], { standalone: true });
    expect(out!.compaction!.summary).toContain(`<saved-results>\nTool results this summary leaves out are saved whole in these files; read or search them:\n${fileOf(note)}\n</saved-results>`);
    expect(out!.compaction!.details).toMatchObject({ [SAVED_RESULTS_DETAILS]: [path.resolve(fileOf(note))] });
  });

  it('names the saved copy of a result cleared after the cache went cold, and keeps the file lists in the details', async () => {
    const copy = saveContentCopy('whole cleared text', 'cleared-read')!;
    const branch = [{ type: 'custom', customType: CLEARED_RESULTS_TYPE, data: { results: [{ id: 'old', saved: copy }] } }];
    const out = await summarize([result('old', 'whole cleared text'), result('other', 'nothing saved')], { branch });
    const file = copy.split(' (sha256')[0]!;
    expect(out!.compaction!.summary).toContain(`<saved-results>\nTool results this summary leaves out are saved whole in these files; read or search them:\n${file}\n</saved-results>`);
    expect(out!.compaction!.summary).not.toContain('sha256');
  });

  it('adds nothing without a saved file, and ignores paths outside the sessions folder', async () => {
    const outside = path.join(home, 'elsewhere', 'output', 'x.txt');
    const out = await summarize([result('a', `Full output: ${outside}; also /tmp/sessions/s/output/y.txt`)]);
    expect(out!.compaction!.summary).not.toContain('saved-results');
    expect(out!.compaction!.details).not.toHaveProperty(SAVED_RESULTS_DETAILS);
  });

  it('reads a path however the note spells it (absolute, or under ~)', () => {
    const file = path.join(sessionOutputDir('output'), 'bash-1.txt');
    vi.spyOn(os, 'homedir').mockReturnValue(home);
    const tilde = `~${path.sep}${path.relative(home, file)}`;
    expect(tilde.startsWith('~')).toBe(true);
    const found = savedResultPaths([result('a', `Full output: ${tilde}; search it.`), result('b', `Saved to ${file}.`)], new Map());
    expect(found).toEqual([file]);
  });

  it('carries earlier paths on, once, and strips the block from the summary the model updates', async () => {
    const earlier = trimmed('bash-earlier', 'EARLIER');
    const later = trimmed('bash-later', 'LATER');
    const firstOut = (await summarize([result('a', earlier)]))!.compaction!;
    const branch = [{ type: 'compaction', fromHook: true, summary: firstOut.summary, details: firstOut.details }];
    expect(carriedSavedResults(branch as never)).toEqual([path.resolve(fileOf(earlier))]);
    const secondOut = (await summarize([result('b', later), result('c', earlier)], { branch, previousSummary: firstOut.summary }))!.compaction!;
    const lines = /<saved-results>\n([\s\S]*?)\n<\/saved-results>/.exec(secondOut.summary)![1]!.split('\n').slice(1);
    expect(lines).toEqual([fileOf(earlier), fileOf(later)]);
    expect(secondOut.details).toMatchObject({ [SAVED_RESULTS_DETAILS]: [path.resolve(fileOf(earlier)), path.resolve(fileOf(later))] });
    // The model's part of an earlier summary comes back without the block, so it is not written twice. The user messages
    // sit before the Recovery section, where `writeSummary` puts them.
    const section = userMessagesSection([], 0, ['ask']);
    expect(splitSummary(firstOut.summary.replace('\n\n## Recovery', `${section}\n\n## Recovery`)).body).toBe('## Goal\nwork');
    expect(withoutSavedResults('## Goal\nx')).toBe('## Goal\nx');
  });

  it('writes a long list whole to a file in the session output folder and names that file', async () => {
    const files = Array.from({ length: 80 }, (_, index) => path.join(sessionOutputDir('output'), `cleared-read-${String(index).padStart(16, '0')}.txt`));
    const block = savedResultsBlock(files);
    expect(block.length).toBeLessThan(SAVED_RESULTS_INLINE_CHARS);
    expect(block).toMatch(/^\n\n<saved-results count="80">\n/);
    const list = /listed one per line in (\S+);/.exec(block)![1]!;
    expect(path.dirname(path.resolve(list))).toBe(sessionOutputDir('output'));
    expect(fs.readFileSync(path.resolve(list), 'utf8').trimEnd().split('\n')).toEqual(files);
    // The pointer block is stripped from a summary like the inline one.
    expect(withoutSavedResults(`## Goal\nx${block}`)).toBe('## Goal\nx');
  });

  it('keeps the whole list inline when the list file cannot be written', () => {
    const files = Array.from({ length: 80 }, (_, index) => path.join(sessionOutputDir('output'), `cleared-read-${String(index).padStart(16, '0')}.txt`));
    vi.spyOn(fs, 'writeFileSync').mockImplementation(() => {
      throw new Error('read-only');
    });
    const block = savedResultsBlock(files);
    for (const file of files) expect(block).toContain(file);
    expect(block.length).toBeGreaterThan(SAVED_RESULTS_INLINE_CHARS);
  });

  it('lists a short list inline and nothing for none', () => {
    expect(savedResultsBlock([])).toBe('');
    const file = path.join(sessionOutputDir('output'), 'bash-1.txt');
    expect(savedResultsBlock([file])).toContain(`\n${file}\n</saved-results>`);
  });

  it('removes only the block code appended: the last one, and only at the end of the body', () => {
    const quoted = '<saved-results>\nthe model quoted this tag\n</saved-results>';
    const own = savedResultsBlock([path.join(sessionOutputDir('output'), 'bash-1.txt')]);
    // A quote earlier in the model's text survives, whatever follows it.
    expect(withoutSavedResults(`## Goal\nsee\n\n${quoted}\n\nmore work${own}`)).toBe(`## Goal\nsee\n\n${quoted}\n\nmore work`);
    // An open tag with no close, or a block that is not last, is the model's.
    expect(withoutSavedResults('## Goal\nx\n\n<saved-results>\nnever closed')).toBe('## Goal\nx\n\n<saved-results>\nnever closed');
    expect(withoutSavedResults(`x\n\n${quoted}\ntrailing text`)).toBe(`x\n\n${quoted}\ntrailing text`);
  });

  it('strips a block the model wrote from its summary before appending the one list, and keeps its other text', async () => {
    const earlier = trimmed('bash-own', 'OWN');
    const answer = `## Goal\nwork\n\n<saved-results>\n/made/up/path\n</saved-results>\n\n## Next\nkeep this`;
    const out = (await summarize([result('a', earlier)], { answer }))!.compaction!;
    expect(out.summary.match(/<saved-results/g)).toHaveLength(1);
    expect(out.summary).not.toContain('/made/up/path');
    expect(out.summary).toContain('## Goal\nwork\n\n## Next\nkeep this');
    expect(out.summary).toContain(fileOf(earlier));
    // With nothing saved the model's block is still not left behind.
    const bare = (await summarize([], { answer }))!.compaction!;
    expect(bare.summary).not.toContain('saved-results');
    expect(withoutModelSavedResults('a\n\n<saved-results count="3">\nx\n</saved-results>\n\n<saved-results>\ny\n</saved-results>')).toBe('a');
  });

  it('keeps carrying the paths when a compaction in between fell back to Pi\'s default summary', async () => {
    const earlier = trimmed('bash-chain', 'CHAIN');
    const first = (await summarize([result('a', earlier)]))!.compaction!;
    const branch = [
      { type: 'compaction', fromHook: true, summary: first.summary, details: first.details },
      // Pi wrote this one: no details.savedResults, and a summary that does not list the files.
      { type: 'compaction', fromHook: false, summary: '## Goal\ndefault', details: { readFiles: [], modifiedFiles: [] } },
      { type: 'message', message: { role: 'user', content: 'go on' } },
    ];
    expect(carriedSavedResults(branch as never)).toEqual([path.resolve(fileOf(earlier))]);
    const next = (await summarize([result('b', 'nothing saved')], { branch, previousSummary: '## Goal\ndefault' }))!.compaction!;
    expect(next.summary).toContain(`\n${fileOf(earlier)}\n</saved-results>`);
    expect(next.details).toMatchObject({ [SAVED_RESULTS_DETAILS]: [path.resolve(fileOf(earlier))] });
    // The newest compaction that carries paths wins; none carries none.
    expect(carriedSavedResults([{ type: 'compaction', details: { [SAVED_RESULTS_DETAILS]: ['/old'] } }, { type: 'compaction', details: { [SAVED_RESULTS_DETAILS]: ['/new'] } }] as never)).toEqual(['/new']);
    expect(carriedSavedResults([{ type: 'compaction', summary: 'x' }] as never)).toEqual([]);
  });

  it('recovers the files saved during a stretch that Pi\'s default summary covered, and drops swept ones', async () => {
    const earlier = trimmed('bash-before', 'BEFORE');
    const during = trimmed('bash-during', 'DURING');
    const kept = trimmed('bash-kept', 'KEPT');
    const first = (await summarize([result('a', earlier)]))!.compaction!;
    const message = (id: string, text: string) => ({ type: 'message', id, message: result(id, text) });
    const branch = [
      { type: 'compaction', id: 'c1', firstKeptEntryId: 'k1', fromHook: true, summary: first.summary, details: first.details },
      // Summarized by the default compaction below: no Octocode record holds this file.
      { type: 'message', id: 'k1', message: { role: 'user', content: 'go on' } },
      message('r1', during),
      // Kept by the default compaction, so the next summary's own messages hold it.
      message('k2', kept),
      { type: 'compaction', id: 'c2', firstKeptEntryId: 'k2', fromHook: false, summary: '## Goal\ndefault', details: { readFiles: [], modifiedFiles: [] } },
    ];
    expect(carriedSavedResults(branch as never)).toEqual([path.resolve(fileOf(earlier)), path.resolve(fileOf(during))]);
    const next = (await summarize([result('b', kept)], { branch, previousSummary: '## Goal\ndefault' }))!.compaction!;
    expect(next.details).toMatchObject({ [SAVED_RESULTS_DETAILS]: [path.resolve(fileOf(earlier)), path.resolve(fileOf(during)), path.resolve(fileOf(kept))] });
    // A file the session sweep removed is no longer listed.
    fs.rmSync(path.resolve(fileOf(earlier)));
    const swept = (await summarize([], { branch, previousSummary: '## Goal\ndefault' }))!.compaction!;
    expect(swept.details).toMatchObject({ [SAVED_RESULTS_DETAILS]: [path.resolve(fileOf(during))] });
    expect(swept.summary).not.toContain(fileOf(earlier));
  });

  it('finds the log a finished background job report names, and its rotated part', () => {
    const log = path.join(sessionOutputDir('bash'), 'job-1.log');
    const job = { id: 'job-1', command: 'make', cwd: tmp(), log, timeout: undefined, stopped: undefined };
    fs.writeFileSync(log, 'tail line\n');
    const text = jobReport(job as never, { code: 0, signal: null, seconds: 3, logRotations: 1 });
    const message = { role: 'custom', customType: JOB_TYPE, content: text, display: true, timestamp: at };
    expect(savedResultPaths([message], new Map())).toEqual([log, `${log}.1`]);
    expect(savedResultPaths([{ ...message, customType: 'other' }], new Map())).toEqual([]);
  });
});
