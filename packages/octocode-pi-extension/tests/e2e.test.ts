import fs from 'node:fs';
import path from 'node:path';
import { fauxAssistantMessage, fauxToolCall, type TranscriptContext } from '@earendil-works/pi-ai';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { McpClient, StdioTransport } from '@earendil-works/pi-mcp';
import { octocodeLaunch } from '../src/mcp/octocode.js';
import { OCTOCODE_TOOL_PREFIX } from '../src/shared/octocode-tools.js';
import { Mail } from '../src/team/mail.js';
import { OCTOCODE_OFF, startSession } from './e2e-harness.js';
import { calls, say } from './helpers.js';

/**
 * Asserts a localFetch result (MCP text is the JSON of its structured content) holds one row per path, in order, each
 * with its first page and the continuation that reads the rest of it; the response fits one page, so it carries no
 * whole-response continuation.
 */
function expectRowsWithOwnContinuations(text: string, paths: string[]): void {
  const page: unknown = JSON.parse(text);
  expect(page).not.toHaveProperty('responsePagination');
  expect(page).toMatchObject({
    results: paths.map((path) => ({
      data: {
        path,
        pagination: { unit: 'lines', offset: 0, hasMore: true, nextOffset: expect.any(Number) },
        next: { continueWindow: { kind: 'continue', tool: 'localFetch', input: { cursor: expect.any(String) } } },
      },
    })),
  });
}

/** The tool names a fresh Octocode MCP server, launched as the extension launches it, answers `tools/list` with. */
async function serverToolNames(cwd: string): Promise<string[]> {
  const client = new McpClient({ name: 'e2e-tools-list', version: '0' });
  try {
    await client.connect(new StdioTransport({ ...octocodeLaunch(cwd), stderr: 'pipe' }));
    return (await client.listTools()).map((tool) => tool.name);
  } finally {
    await client.close();
  }
}

describe('with Octocode MCP (default)', () => {
  let s: Awaited<ReturnType<typeof startSession>>;
  beforeAll(async () => {
    s = await startSession({});
    fs.writeFileSync(path.join(s.cwd, 'app.ts'), 'export const answer = 41;\n');
    fs.writeFileSync(path.join(s.cwd, 'unread.txt'), 'never read\n');
  }, 60_000);
  afterAll(() => s.dispose());

  it('exposes MCP tools and Octocode guidance from the first request', async () => {
    const turn = (await s.run('hello', [say('ready')])).at(-1)!;
    // Real Pi + the local MCP, before provider-specific encoding. Core owns the server's surface size; the
    // extension-owned prose is guarded by prompt-size.test.ts.
    expect(turn.tools).toContain('file');
    expect(turn.tools).toContain('read');
    for (const name of ['edit', 'write']) expect(turn.tools).not.toContain(name);
    expect(turn.systemPrompt).toContain('<octocode>');
    // Every tool the server lists, GitHub and package registry ones included, is declared from the first request.
    const declared = turn.tools.filter((name) => name.startsWith(OCTOCODE_TOOL_PREFIX)).map((name) => name.slice(OCTOCODE_TOOL_PREFIX.length));
    expect(declared.length).toBeGreaterThan(0);
    expect(declared.toSorted()).toEqual((await serverToolNames(s.cwd)).toSorted());
    expect(turn.tools).not.toContain('tool_search');
    // The extension's client connects before the first request is built, so the guidance and the server's own
    // instructions (which Pi puts in no prompt for direct tools) are there from the start, unchanged on the next request.
    expect(turn.systemPrompt).toContain('Use Octocode MCP (`mcp__octocode__*`');
    expect(turn.systemPrompt).toMatch(/# Octocode MCP server instructions\n\S/);
    const next = (await s.run('continue', [say('ready')])).at(-1)!;
    expect(next.tools).toContain('mcp__octocode__localSearch');
    expect(next.systemPrompt).toBe(turn.systemPrompt);
    expect(next.toolContracts).toEqual(turn.toolContracts);
  }, 60_000);

  it('records passive team notices during streaming without an extra model request', async () => {
    const mail = new Mail({ sendMessage: s.session.sendCustomMessage.bind(s.session) });
    let requests = 0;
    const unsubscribe = s.session.agent.subscribe((event) => { if (event.type === 'turn_start') requests++; });
    try {
      await s.run('finish this step', [() => {
        expect(mail.notice('passive notice: earlier mail was not delivered')).toBe(true);
        return fauxAssistantMessage('done');
      }]);
    } finally { unsubscribe(); }
    expect(requests).toBe(1);
    expect(JSON.stringify(s.session.sessionManager.getBranch())).toContain('passive notice: earlier mail was not delivered');
  });

  it('researches through Octocode, reads with read, then edits, writes and deletes with batched file calls', async () => {
    const appPath = path.join(s.cwd, 'app.ts');
    const turns = await s.run('fix the answer', [
      calls(fauxToolCall('mcp__octocode__localFetch', { queries: [{ path: appPath }] }), fauxToolCall('read', { path: 'app.ts' })),
      calls(
        fauxToolCall('file', {
          queries: [
            { reasoning: 'answer is off by one', type: 'edit', path: 'app.ts', edits: [{ oldText: '41', newText: '42' }] },
            { reasoning: 'document the constant', type: 'write', path: 'NOTES.md', content: '# Notes\n' },
            { reasoning: 'replace a whole file', type: 'write', path: 'unread.txt', content: 'replaced' },
          ],
        }),
      ),
      calls(fauxToolCall('file', { queries: [{ reasoning: 'no longer needed', type: 'delete', path: 'NOTES.md' }] })),
      say('done'),
    ]);
    const [afterRead, afterBatch, afterDelete] = turns.slice(-3);
    expect(afterRead!.lastResults[0]!.text).toContain('export const answer = 41');
    const batch = afterBatch!.lastResults[0]!;
    expect(batch.isError).toBe(false);
    expect(batch.text).toMatch(/1\. OK edit app\.ts/);
    expect(batch.text).toMatch(/2\. OK write NOTES\.md/);
    // An existing file never read in this session is refused; the rest of the batch still applies.
    expect(batch.text).toMatch(/3\. FAILED write unread\.txt: unread\.txt already exists and was not read in this session/);
    expect(afterDelete!.lastResults[0]!.text).toMatch(/OK delete NOTES\.md/);
    expect(fs.readFileSync(appPath, 'utf8')).toBe('export const answer = 42;\n');
    expect(fs.readFileSync(path.join(s.cwd, 'unread.txt'), 'utf8')).toBe('never read\n');
    expect(fs.existsSync(path.join(s.cwd, 'NOTES.md'))).toBe(false);
  }, 60_000);

  it('refuses to edit a file that changed on disk after it was read', async () => {
    const appPath = path.join(s.cwd, 'app.ts');
    const turns = await s.run('edit again', [
      () => {
        fs.writeFileSync(appPath, 'export const answer = 7;\n'); // someone else edits the file
        return fauxAssistantMessage([fauxToolCall('file', { queries: [{ reasoning: 'bump', type: 'edit', path: 'app.ts', edits: [{ oldText: '42', newText: '43' }] }] })], { stopReason: 'toolUse' });
      },
      say('done'),
    ]);
    const result = turns.at(-1)!.lastResults[0]!;
    expect(result.isError).toBe(true);
    expect(result.text).toMatch(/changed on disk since you last read it/);
  }, 60_000);

  it('leaves the file guard to Pi read: an Octocode MCP read does not unlock a change', async () => {
    const seen = path.join(s.cwd, 'seen.ts');
    fs.writeFileSync(seen, 'export const seen = 1;\n');
    const turns = await s.run('read then edit', [
      calls(fauxToolCall('mcp__octocode__localFetch', { queries: [{ path: seen }] })),
      calls(fauxToolCall('file', { queries: [{ reasoning: 'bump', type: 'edit', path: 'seen.ts', edits: [{ oldText: 'seen = 1', newText: 'seen = 2' }] }] })),
      say('done'),
    ]);
    const [afterRead, afterEdit] = turns.slice(-2);
    expect(afterRead!.lastResults[0]!.text).toContain('export const seen = 1;');
    expect(afterEdit!.lastResults[0]!.text).toMatch(/seen\.ts already exists and was not read in this session/);
    expect(fs.readFileSync(seen, 'utf8')).toBe('export const seen = 1;\n');
  }, 60_000);

  it('gives the model a page over 20 KB of UTF-8 whole, with no truncation marker', async () => {
    // Octocode budgets pages in characters, so a page of non-ASCII source is far over 20 KB of UTF-8.
    const files = ['one', 'two'].map((name) => {
      const file = path.join(s.cwd, `${name}.ts`);
      fs.writeFileSync(file, Array.from({ length: 3_000 }, (_, index) => `export const ${name}${index} = '${'日本語のテキスト'.repeat(4)}';\n`).join(''));
      return file;
    });
    const turns = await s.run('read both', [calls(fauxToolCall('mcp__octocode__localFetch', { queries: files.map((file) => ({ path: file, ranges: [{ startLine: 1, endLine: 600 }] })) })), say('done')]);
    const text = turns.at(-1)!.lastResults[0]!.text;
    expect(Buffer.byteLength(text)).toBeGreaterThan(20 * 1024);
    expect(text).not.toMatch(/truncated|fullOutputPath|pi-mcp/i);
    // Both rows arrive whole: each one's first returned line and its own continuation.
    for (const name of ['one', 'two']) expect(text).toContain(`export const ${name}0 = `);
    expectRowsWithOwnContinuations(text, ['one.ts', 'two.ts']);
  }, 60_000);
});

describe('compaction', () => {
  let s: Awaited<ReturnType<typeof startSession>>;
  const compactionEntries = () => s.session.sessionManager.getEntries().filter((entry) => entry.type === 'compaction') as Array<{ fromHook?: boolean; summary: string; details?: Record<string, unknown> }>;
  /** Answers every summary request, recording the model and prompt text of each. */
  const script = (reply: string) => {
    const log: Array<{ model: string; text: string }> = [];
    s.faux.setResponses(
      Array.from({ length: 10 }, () => (context: TranscriptContext, _options: unknown, _state: unknown, model: { id: string }) => {
        log.push({ model: model.id, text: JSON.stringify(context.messages) });
        return fauxAssistantMessage(reply);
      }),
    );
    return log;
  };

  beforeAll(async () => {
    s = await startSession(
      OCTOCODE_OFF,
      {
        // A priced session model plus a cheaper small-tier one: compaction must stay on the session model.
        models: [
          { id: 'faux-1', inputCost: 3, maxTokens: 32_000 },
          { id: 'faux-haiku', inputCost: 1, maxTokens: 16_000 },
        ],
        settings: { compaction: { enabled: false, keepRecentTokens: 50 } },
      },
    );
  }, 60_000);
  afterAll(() => s.dispose());

  it("summarizes on the user's model with Octocode's handoff sections, listing files the file tool changed", async () => {
    fs.writeFileSync(path.join(s.cwd, 'notes.txt'), 'hello\n');
    await s.run('Please read notes.txt and create todo.txt', [
      calls(fauxToolCall('read', { path: 'notes.txt' })),
      calls(fauxToolCall('file', { queries: [{ reasoning: 'create', type: 'write', path: 'todo.txt', content: 'x' }] })),
      say(`Done. ${'detail '.repeat(300)}`),
    ]);
    await s.run('Now keep going with the plan', [say(`More work. ${'detail '.repeat(300)}`)]);
    const log = script('## Goal\nPi summary');
    await s.session.compact('keep the notes');
    expect(log.length).toBeGreaterThan(0);
    expect(new Set(log.map((call) => call.model))).toEqual(new Set(['faux-1']));
    expect(log.some((call) => call.text.includes('keep the notes'))).toBe(true);
    // The summary is asked for inside the session's own request (its messages, then one more), not as a standalone prompt.
    expect(log[0]!.text).toContain('Please read notes.txt and create todo.txt');
    expect(log[0]!.text).toContain('Stop here and write a context checkpoint');
    expect(log[0]!.text).not.toContain('<conversation>');
    for (const section of ['## Errors & Fixes', '## Where We Stopped', '## Follow-ups', 'only record']) expect(log.some((call) => call.text.includes(section))).toBe(true);
    const entry = compactionEntries().at(-1)!;
    expect(entry.fromHook).toBe(true);
    expect(entry.details).toMatchObject({ readFiles: ['notes.txt'], modifiedFiles: ['todo.txt'] });
    expect(entry.summary).toContain('Pi summary');
    expect(entry.summary).toMatch(/<read-files>\nnotes\.txt\n<\/read-files>[\s\S]*<modified-files>\ntodo\.txt\n<\/modified-files>/);
    // Code, not the model, keeps the user's own words, oldest first.
    expect(entry.summary).toMatch(/## User Messages \(verbatim, oldest first\)\n<user-message>\nPlease read notes\.txt and create todo\.txt\n<\/user-message>\n<user-message>\nNow keep going with the plan\n<\/user-message>/);
  }, 60_000);

  it('reads the files worked on back into the context after compaction, so they can be changed without a fresh read', async () => {
    const turns = await s.run('overwrite', [calls(fauxToolCall('file', { queries: [{ reasoning: 'replace', type: 'write', path: 'notes.txt', content: 'x' }] })), say('done')]);
    expect(turns.at(-1)!.lastResults[0]!.text).toMatch(/OK write notes\.txt/);
    const restored = s.session.sessionManager.getEntries().filter((entry) => entry.type === 'custom_message' && (entry as { customType?: string }).customType === 'octocode-restored-files') as Array<{ content: string; details: { files: string[] } }>;
    expect(restored).toHaveLength(1);
    expect(restored[0]!.content).toMatch(/<file path="todo\.txt">\nx\n<\/file>/);
    expect(restored[0]!.content).toMatch(/<file path="notes\.txt">\nhello\n<\/file>/);
  }, 60_000);

  it('carries file lists forward through the next compaction, and puts the summary first in the next request', async () => {
    await s.run('more', [say(`Even more. ${'detail '.repeat(300)}`)]);
    script('## Goal\nNext summary');
    await s.session.compact();
    expect(compactionEntries().at(-1)!.details!['modifiedFiles']).toEqual(expect.arrayContaining(['todo.txt', 'notes.txt']));
    const log: string[] = [];
    s.faux.setResponses([(context: TranscriptContext) => {
      const messages = context.messages.filter((message) => (message as { role: string }).role !== 'system');
      log.push(JSON.stringify(messages[0]));
      return fauxAssistantMessage('ok');
    }]);
    await s.session.prompt('continue');
    expect(log[0]).toContain('Next summary');
  }, 60_000);

  it("falls back to Pi's default summary when Octocode's fails", async () => {
    await s.run('more', [say(`Again. ${'detail '.repeat(300)}`)]);
    let calls = 0;
    s.faux.setResponses(Array.from({ length: 6 }, () => () => {
      calls += 1;
      // The cache-sharing request and Octocode's standalone one both fail; Pi's own summary then answers.
      if (calls <= 2) throw new Error('summary provider down');
      return fauxAssistantMessage('## Goal\nDefault summary');
    }));
    await s.session.compact();
    const entry = compactionEntries().at(-1)!;
    expect(entry.fromHook).not.toBe(true);
    expect(entry.summary).toContain('Default summary');
    expect(entry.details!['modifiedFiles']).toEqual(expect.arrayContaining(['todo.txt']));
  }, 60_000);
});
