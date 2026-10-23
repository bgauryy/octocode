import fs from 'node:fs';
import path from 'node:path';
import { fauxAssistantMessage, fauxToolCall, type TranscriptContext } from '@earendil-works/pi-ai';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { calls, FIXTURE, OCTOCODE_OFF, say, startSession, type Reply } from './e2e-harness.js';

describe('with the user\'s own octocode server in mcp.json', () => {
  let s: Awaited<ReturnType<typeof startSession>>;
  beforeAll(async () => {
    // The user's entry owns `octocode`: Pi runs it, and the extension starts and registers nothing of its own.
    s = await startSession({ octocode: { command: process.execPath, args: [FIXTURE], exposure: 'direct' } });
  }, 60_000);
  afterAll(() => s.dispose());

  it('leaves the server to Pi, follows its tools in the prompt, and round-trips a call', async () => {
    const turns = await s.run('use it', [calls(fauxToolCall('mcp__octocode__shout', { text: 'hi' })), say('done')]);
    const first = turns.at(-2)!;
    expect(first.tools).toContain('mcp__octocode__shout');
    expect(first.tools).not.toContain('mcp__octocode__localSearch');
    expect(first.systemPrompt).toContain('Use Octocode MCP');
    expect(first.systemPrompt).not.toContain('# Octocode MCP server instructions');
    expect(turns.at(-1)!.lastResults[0]).toEqual({ toolName: 'mcp__octocode__shout', isError: false, text: 'HI' });
  });
});

describe('with another MCP server and Octocode disabled', () => {
  let s: Awaited<ReturnType<typeof startSession>>;
  beforeAll(async () => {
    s = await startSession({ ...OCTOCODE_OFF, echo: { command: process.execPath, args: [FIXTURE], exposure: 'direct' } });
  }, 60_000);
  afterAll(() => s.dispose());

  it('keeps Pi read, leaves MCP to Pi, and drops the Octocode research line', async () => {
    const turn = (await s.run('hello', [say('ready')])).at(-1)!;
    for (const name of ['read', 'file', 'bash', 'web', 'agent', 'askUser', 'coordinate', 'sendMessage', 'backlog', 'memory', 'mcp__echo__shout']) expect(turn.tools).toContain(name);
    for (const name of ['edit', 'write', 'mcp']) expect(turn.tools).not.toContain(name);
    expect(turn.tools.some((name) => name.startsWith('mcp__octocode__'))).toBe(false);
    // The mcp.json entry replaces the registration, so no request is told to prefer Octocode.
    expect(turn.systemPrompt).not.toContain('Use Octocode MCP');
    const next = (await s.run('again', [say('ok')])).at(-1)!;
    expect(next.systemPrompt).not.toContain('Use Octocode MCP');
  });

  it('calls another server\'s tools through Pi\'s MCP, surfacing remote errors', async () => {
    const turns = await s.run('use echo', [calls(fauxToolCall('mcp__echo__shout', { text: 'hi' }), fauxToolCall('mcp__echo__fail', {})), say('done')]);
    const [ok, failed] = turns.at(-1)!.lastResults;
    expect(ok).toEqual({ toolName: 'mcp__echo__shout', isError: false, text: 'HI' });
    expect(failed.isError).toBe(true);
    expect(failed.text).toContain('boom');
  });

  it('writes new files, refuses an unread existing one, and a read then write works', async () => {
    fs.writeFileSync(path.join(s.cwd, 'existing.txt'), 'original\n');
    const turns = await s.run('write files', [
      calls(fauxToolCall('file', { queries: [{ reasoning: 'new', type: 'write', path: 'new.txt', content: 'fresh' }, { reasoning: 'clobber', type: 'write', path: 'existing.txt', content: 'x' }] })),
      calls(fauxToolCall('read', { path: 'existing.txt' })),
      calls(fauxToolCall('file', { queries: [{ reasoning: 'replace', type: 'write', path: 'existing.txt', content: 'replaced' }] })),
      say('done'),
    ]);
    const [writes, , rewrite] = turns.slice(-3);
    expect(writes.lastResults[0].text).toMatch(/1\. OK write new\.txt[\s\S]*2\. FAILED write existing\.txt: existing\.txt already exists and was not read/);
    expect(rewrite.lastResults[0].text).toMatch(/OK write existing\.txt/);
    expect(fs.readFileSync(path.join(s.cwd, 'existing.txt'), 'utf8')).toBe('replaced');
  });

  it('answers a repeated read of an unchanged file with a note, and the file can still be rewritten after it', async () => {
    fs.writeFileSync(path.join(s.cwd, 'twice.txt'), 'same line\n'.repeat(200));
    const turns = await s.run('read twice', [
      calls(fauxToolCall('read', { path: 'twice.txt' })),
      calls(fauxToolCall('read', { path: 'twice.txt' })),
      calls(fauxToolCall('file', { queries: [{ reasoning: 'replace', type: 'write', path: 'twice.txt', content: 'replaced' }] })),
      say('done'),
    ]);
    const [first, second, rewrite] = turns.slice(-3);
    expect(first.lastResults[0].text).toContain('same line');
    expect(second.lastResults[0].text).toMatch(/^\[Unchanged since your earlier read of twice\.txt \(same range\)/);
    expect(rewrite.lastResults[0].text).toMatch(/OK write twice\.txt/);
  });

  it('clears old results at request time once the cache is cold inside a run, to saved files and without any context edit', async () => {
    fs.writeFileSync(path.join(s.cwd, 'big.txt'), 'line of text\n'.repeat(1_000));
    const reads = Array.from({ length: 22 }, () => fauxToolCall('read', { path: 'big.txt' }));
    const realNow = Date.now.bind(Date);
    // The tools ran longer than the prompt-cache lifetime: the next request would rewrite the cache anyway.
    const later = vi.spyOn(Date, 'now').mockImplementation(() => realNow() + 60 * 60_000 + 1);
    const cold = () => fauxAssistantMessage(reads, { stopReason: 'toolUse', timestamp: realNow() });
    const results = await s.run('read a lot', [cold, say('done')]).finally(() => later.mockRestore()).then((turns) => turns.at(-1)!.lastResults);
    expect(results).toHaveLength(22);
    const cleared = results.filter((result) => result.text.startsWith('[Old tool result cleared'));
    // The newest five stay whole; every older one is a placeholder that names the file holding its whole text.
    expect(cleared).toHaveLength(17);
    expect(results.slice(-5).every((result) => result.text.length > 10_000)).toBe(true);
    for (const result of cleared) expect(result.text).toMatch(/Full output: \S+ \(sha256/);
    const entries = s.session.sessionManager.getEntries();
    expect(entries.filter((entry) => entry.type === 'context_edit')).toHaveLength(0);
    expect(entries.filter((entry) => entry.type === 'custom' && entry.customType === 'octocode-cleared-results')).toHaveLength(1);
  });

  it('reports unavailable askUser without inventing an answer or approval', async () => {
    const question = { question: 'Which?', header: 'Pick', options: [{ label: 'A (Recommended)', description: 'a' }, { label: 'B', description: 'b' }] };
    const turns = await s.run('ask', [calls(fauxToolCall('askUser', { questions: [question] })), say('done')]);
    expect(turns.at(-1)!.lastResults[0].text).toMatch(/No interactive user/);
    expect(turns.at(-1)!.lastResults[0].text).toContain('No answer or approval was supplied.');
  });
});

/** How many image parts a request carries. */
const images = (context: TranscriptContext) =>
  context.messages.reduce((sum, message) => {
    const content = (message as { content?: unknown }).content;
    return sum + (Array.isArray(content) ? content.filter((part: { type?: string }) => part.type === 'image').length : 0);
  }, 0);

describe('request too large (HTTP 413)', () => {
  let s: Awaited<ReturnType<typeof startSession>>;
  beforeAll(async () => {
    s = await startSession(OCTOCODE_OFF);
    // A 1x1 PNG: Pi's `read` returns it as an image part.
    const png = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64');
    for (const name of ['a.png', 'b.png', 'c.png']) fs.writeFileSync(path.join(s.cwd, name), png);
  }, 60_000);
  afterAll(() => s.dispose());

  it('strips older images and retries on its own when a gateway rejects the request with a bodiless 413', async () => {
    const seen: number[] = [];
    // The gateway accepts at most one image per request.
    const gateway = (context: TranscriptContext, reply: Reply) => {
      seen.push(images(context));
      return images(context) > 1 ? fauxAssistantMessage([], { stopReason: 'error', errorMessage: '413 status code (no body)' }) : reply;
    };
    s.faux.setResponses([
      (context) => gateway(context, fauxAssistantMessage([fauxToolCall('read', { path: 'a.png' }), fauxToolCall('read', { path: 'b.png' }), fauxToolCall('read', { path: 'c.png' })], { stopReason: 'toolUse' })),
      (context) => gateway(context, fauxAssistantMessage('rejected')),
      (context) => gateway(context, fauxAssistantMessage('looked at the newest image')),
    ]);
    await s.session.prompt('look at the screenshots');
    // Request 2 carries three images and is rejected; the retry keeps only the newest and succeeds, without a new prompt.
    expect(seen).toEqual([0, 3, 1]);
    const last = s.session.messages.at(-1);
    expect(last?.role).toBe('assistant');
    if (last?.role !== 'assistant') throw new Error('expected the assistant reply last');
    expect(last.content.map((part) => (part.type === 'text' ? part.text : '')).join('')).toBe('looked at the newest image');
  }, 60_000);
});

describe('with a tool allowlist that leaves out file', () => {
  let s: Awaited<ReturnType<typeof startSession>>;
  beforeAll(async () => {
    s = await startSession(OCTOCODE_OFF, { tools: ['read', 'bash', 'edit', 'write'] });
  }, 60_000);
  afterAll(() => s.dispose());

  it('keeps Pi edit and write, so the agent can still change files', async () => {
    const turn = (await s.run('hi', [say('ok')])).at(-1)!;
    expect(turn.tools).toEqual(expect.arrayContaining(['read', 'bash', 'edit', 'write']));
    expect(turn.tools).not.toContain('file');
  });
});

describe('a subagent session with visible browser and MCP enabled', () => {
  let s: Awaited<ReturnType<typeof startSession>>;
  const previous = {
    web: process.env['OCTOCODE_BROWSER_VISIBLE'],
    sub: process.env['OCTOCODE_SUBAGENT'],
  };

  beforeAll(async () => {
    process.env['OCTOCODE_BROWSER_VISIBLE'] = '1';
    process.env['OCTOCODE_SUBAGENT'] = '1';
    s = await startSession({});
    fs.writeFileSync(path.join(s.cwd, 'app.ts'), 'export const answer = 41;\n');
  }, 200_000);

  afterAll(async () => {
    await s?.dispose();
    for (const [key, value] of [['OCTOCODE_BROWSER_VISIBLE', previous.web], ['OCTOCODE_SUBAGENT', previous.sub]] as const) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  });

  it('has Octocode and browser tools and can call Octocode', async () => {
    const turn = (await s.run('hello', [say('ready')])).at(-1)!;
    expect(turn.tools).toContain('mcp__octocode__localFetch');
    expect(turn.tools).toContain('browser');
    expect(turn.tools.filter((name) => name.includes('cua'))).toEqual([]);
    expect(turn.tools).not.toContain('agent');
    expect(turn.tools).not.toContain('askUser');
    expect(turn.toolContracts.find((tool) => tool.name === 'memory')?.parameters).toMatchObject({
      properties: { op: { enum: ['search', 'get', 'list'] } },
    });
    expect(turn.toolContracts.find((tool) => tool.name === 'backlog')?.parameters).toMatchObject({
      properties: { op: { enum: ['list', 'get', 'add', 'update'] } },
    });
    for (const name of ['coordinate', 'sendMessage', 'backlog', 'memory']) expect(turn.tools).toContain(name);
    expect(turn.systemPrompt).toContain('You are a subagent');
    expect(turn.systemPrompt).toContain('reserve files');
    const appPath = path.join(s.cwd, 'app.ts');
    const turns = await s.run('read', [
      calls(fauxToolCall('mcp__octocode__localFetch', { queries: [{ path: appPath, fullContent: true }] })),
      say('done'),
    ]);
    const results = turns.at(-1)!.lastResults;
    expect(results.find((result) => result.toolName === 'mcp__octocode__localFetch')?.text).toContain('answer = 41');
  }, 60_000);
});
