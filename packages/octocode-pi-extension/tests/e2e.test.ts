import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createFauxCore, fauxAssistantMessage, fauxToolCall, getCurrentSystemPrompt, getCurrentTools, type TranscriptContext } from '@earendil-works/pi-ai';
import { createAgentSession, createMcpExtension, createToolSearchExtension, DefaultResourceLoader, SessionManager, SettingsManager, type ExtensionFactory } from '@earendil-works/pi-coding-agent';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ENTRY = path.join(HERE, '..', process.env['OCTOCODE_E2E_ENTRY'] === 'dist' ? 'dist/index.js' : 'src/index.ts');
const octocode = (await import(ENTRY)).default as ExtensionFactory;

// Hermetic: a run from inside a Pi subagent inherits its identity and settings, which change the prompt, tools and limits.
for (const key of ['OCTOCODE_SUBAGENT', 'OCTOCODE_BROWSER_VISIBLE', 'OCTOCODE_PARENT_ID', 'OCTOCODE_AGENT_ID', 'OCTOCODE_AGENT_TASK', 'OCTOCODE_AGENT_COLLABORATE', 'OCTOCODE_SUBAGENT_COLLABORATE', 'OCTOCODE_MAX_SUBAGENTS', 'OCTOCODE_DIRECT_TOOLS', 'OCTOCODE_HOME', 'OCTOCODE_AGENT_DB', 'OCTOCODE_MEMORY_AUTO']) {
  delete process.env[key];
}

const FIXTURE = path.join(HERE, 'fixtures', 'echo-mcp.mjs');

interface Turn {
  systemPrompt: string;
  tools: string[];
  toolContracts: ReturnType<typeof getCurrentTools>;
  lastResults: Array<{ toolName: string; isError: boolean; text: string }>;
}

function snapshot(context: TranscriptContext): Turn {
  const results: Turn['lastResults'] = [];
  for (let index = context.messages.length - 1; index >= 0; index--) {
    const message = context.messages[index] as { role: string; toolName?: string; isError?: boolean; content?: unknown };
    if (message.role === 'system') continue; // tool/prompt deltas are recorded as system messages
    if (message.role !== 'toolResult') break;
    const text = Array.isArray(message.content) ? message.content.map((part: { text?: string }) => part.text ?? '').join('') : '';
    results.unshift({ toolName: String(message.toolName), isError: message.isError === true, text });
  }
  const toolContracts = getCurrentTools(context.messages);
  return { systemPrompt: getCurrentSystemPrompt(context.messages), tools: toolContracts.map((tool) => tool.name), toolContracts, lastResults: results };
}

type Reply = ReturnType<typeof fauxAssistantMessage>;

/** A real Pi agent session running the Octocode extension against a scripted model. */
async function startSession(mcpServers: Record<string, unknown>, options: { tools?: string[]; settings?: Record<string, unknown>; agentSettings?: Record<string, unknown>; models?: Array<{ id: string; inputCost: number; maxTokens: number }> } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-e2e-'));
  const cwd = path.join(root, 'project');
  const home = path.join(root, 'home');
  fs.mkdirSync(cwd, { recursive: true });
  fs.mkdirSync(home, { recursive: true });
  const agentDir = path.join(root, 'agent');
  fs.mkdirSync(agentDir, { recursive: true });
  if (options.agentSettings) fs.writeFileSync(path.join(agentDir, 'settings.json'), JSON.stringify(options.agentSettings));
  const originalHome = process.env['HOME'];
  const originalAgentDir = process.env['PI_CODING_AGENT_DIR'];
  process.env['HOME'] = home; // keep the developer's own config out of the test
  process.env['PI_CODING_AGENT_DIR'] = agentDir; // and their Pi settings
  // Pi's built-in MCP reads the user-level mcp.json; an `octocode` entry there overrides Octocode's registration.
  fs.writeFileSync(path.join(agentDir, 'mcp.json'), JSON.stringify({ mcpServers }));
  const turns: Turn[] = [];
  const models = options.models ?? [{ id: 'faux-1', inputCost: 0, maxTokens: 4_096 }];
  const faux = createFauxCore({ provider: 'faux', models: models.map(({ id, maxTokens }) => ({ id, contextWindow: 200_000, maxTokens })) as never });
  const provider: ExtensionFactory = (pi) => {
    pi.registerProvider('faux', {
      name: 'Faux',
      api: faux.api,
      baseUrl: 'http://127.0.0.1:0',
      apiKey: 'test',
      models: faux.models.map((model, index) => ({ ...model, cost: { input: models[index]!.inputCost, output: 0, cacheRead: 0, cacheWrite: 0 } })),
      streamSimple: faux.streamSimple,
    } as never);
  };
  const settingsManager = SettingsManager.inMemory({ compaction: { enabled: false }, retry: { enabled: false }, ...options.settings });
  const resourceLoader = new DefaultResourceLoader({
    cwd,
    agentDir: path.join(root, 'agent'),
    settingsManager,
    extensionFactories: [
      { name: 'faux-provider', factory: provider },
      { name: 'octocode', factory: octocode as ExtensionFactory },
      // The CLI loads Pi's MCP as a built-in extension, after the user's; SDK sessions add it themselves.
      { name: 'mcp', factory: createMcpExtension({ logPath: path.join(root, 'mcp.log') }) },
      // Also built in on the CLI: it loads Octocode's deferred GitHub and npm tools.
      { name: 'tool-search', factory: createToolSearchExtension() },
    ],
    noSkills: true,
    noPromptTemplates: true,
    noThemes: true,
    noContextFiles: true,
  });
  await resourceLoader.reload();
  const { session } = await createAgentSession({ cwd, agentDir: path.join(root, 'agent'), resourceLoader, settingsManager, sessionManager: SessionManager.inMemory(cwd), ...(options.tools ? { tools: options.tools } : {}) });
  await session.bindExtensions({ mode: 'json', shutdownHandler: () => undefined } as never);
  await session.setModel(session.modelRuntime.getModel('faux', 'faux-1')!);
  const run = async (prompt: string, replies: Array<(turn: Turn) => Reply>) => {
    faux.setResponses(
      replies.map((reply) => (context: TranscriptContext) => {
        const turn = snapshot(context);
        turns.push(turn);
        return reply(turn);
      }),
    );
    await session.prompt(prompt);
    return turns;
  };
  const dispose = async () => {
    await session.dispose?.();
    process.env['HOME'] = originalHome;
    if (originalAgentDir === undefined) delete process.env['PI_CODING_AGENT_DIR'];
    else process.env['PI_CODING_AGENT_DIR'] = originalAgentDir;
    fs.rmSync(root, { recursive: true, force: true });
  };
  return { cwd, session, faux, run, dispose };
}

/** An mcp.json `octocode` entry that keeps the server off (a file entry wins over Octocode's registration). */
const OCTOCODE_OFF = { octocode: { command: 'false', enabled: false } };

const calls = (...toolCalls: ReturnType<typeof fauxToolCall>[]) => () => fauxAssistantMessage(toolCalls, { stopReason: 'toolUse' });
const say = (text: string) => () => fauxAssistantMessage(text);

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
    expect(turn.tools).toContain('file');
    expect(turn.tools).toContain('read');
    expect(turn.tools).toContain('mcp__octocode__localGetFileContent');
    expect(turn.tools).toContain('mcp__octocode__localSearch');
    for (const name of ['edit', 'write']) expect(turn.tools).not.toContain(name);
    expect(turn.systemPrompt).toContain('<octocode>');
    // GitHub and npm tools are deferred behind tool_search.
    expect(turn.tools).toContain('tool_search');
    expect(turn.tools.some((name) => name.startsWith('mcp__octocode__gh') || name === 'mcp__octocode__npmSearch')).toBe(false);
    // Pi snapshots the selected tools before its MCP host connects servers on the first request, so the guidance
    // follows the registration and is there from the start, unchanged on the next request.
    expect(turn.systemPrompt).toContain('Prefer Octocode MCP (`mcp__octocode__*`) for code search and research');
    const next = (await s.run('continue', [say('ready')])).at(-1)!;
    expect(next.tools).toContain('mcp__octocode__localSearch');
    expect(next.systemPrompt).toBe(turn.systemPrompt);
  }, 60_000);

  it('loads the deferred GitHub and npm tools through tool_search', async () => {
    const turns = await s.run('find a github tool', [calls(fauxToolCall('tool_search', { query: 'GitHub code search repositories' })), say('found')]);
    const after = turns.at(-1)!;
    expect(after.lastResults[0]!.isError).toBe(false);
    expect(after.tools.some((name) => name.startsWith('mcp__octocode__gh'))).toBe(true);
  }, 60_000);

  it('reads through Octocode, then edits, writes and deletes with batched file calls', async () => {
    const appPath = path.join(s.cwd, 'app.ts');
    const turns = await s.run('fix the answer', [
      calls(fauxToolCall('mcp__octocode__localGetFileContent', { queries: [{ goal: 'see value', reasoning: 'need current code', path: appPath, fullContent: true }] })),
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
    expect(batch.text).toMatch(/3\. OK write unread\.txt/);
    expect(afterDelete!.lastResults[0]!.text).toMatch(/OK delete NOTES\.md/);
    expect(fs.readFileSync(appPath, 'utf8')).toBe('export const answer = 42;\n');
    expect(fs.readFileSync(path.join(s.cwd, 'unread.txt'), 'utf8')).toBe('replaced');
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
});

describe('with another MCP server and Octocode disabled', () => {
  let s: Awaited<ReturnType<typeof startSession>>;
  beforeAll(async () => {
    s = await startSession({ ...OCTOCODE_OFF, echo: { command: process.execPath, args: [FIXTURE], exposure: 'direct' } });
  }, 60_000);
  afterAll(() => s.dispose());

  it('keeps Pi read, leaves MCP to Pi, and drops the Octocode research line', async () => {
    const turn = (await s.run('hello', [say('ready')])).at(-1)!;
    for (const name of ['read', 'file', 'bash', 'web', 'browser', 'agent', 'askUser', 'coordinate', 'sendMessage', 'backlog', 'memory', 'mcp__echo__shout']) expect(turn.tools).toContain(name);
    for (const name of ['edit', 'write', 'mcp']) expect(turn.tools).not.toContain(name);
    expect(turn.tools.some((name) => name.startsWith('mcp__octocode__'))).toBe(false);
    // The mcp.json entry replaces the registration, so no request is told to prefer Octocode.
    expect(turn.systemPrompt).not.toContain('Prefer Octocode MCP');
    const next = (await s.run('again', [say('ok')])).at(-1)!;
    expect(next.systemPrompt).not.toContain('Prefer Octocode MCP');
  });

  it('calls another server\'s tools through Pi\'s MCP, surfacing remote errors', async () => {
    const turns = await s.run('use echo', [calls(fauxToolCall('mcp__echo__shout', { text: 'hi' }), fauxToolCall('mcp__echo__fail', {})), say('done')]);
    const [ok, failed] = turns.at(-1)!.lastResults;
    expect(ok).toEqual({ toolName: 'mcp__echo__shout', isError: false, text: 'HI' });
    expect(failed!.isError).toBe(true);
    expect(failed!.text).toContain('boom');
  });

  it('writes new and existing files, and a read then write works', async () => {
    fs.writeFileSync(path.join(s.cwd, 'existing.txt'), 'original\n');
    const turns = await s.run('write files', [
      calls(fauxToolCall('file', { queries: [{ reasoning: 'new', type: 'write', path: 'new.txt', content: 'fresh' }, { reasoning: 'clobber', type: 'write', path: 'existing.txt', content: 'x' }] })),
      calls(fauxToolCall('read', { path: 'existing.txt' })),
      calls(fauxToolCall('file', { queries: [{ reasoning: 'replace', type: 'write', path: 'existing.txt', content: 'replaced' }] })),
      say('done'),
    ]);
    const [writes, , rewrite] = turns.slice(-3);
    expect(writes!.lastResults[0]!.text).toMatch(/1\. OK write new\.txt[\s\S]*2\. OK write existing\.txt/);
    expect(rewrite!.lastResults[0]!.text).toMatch(/OK write existing\.txt/);
    expect(fs.readFileSync(path.join(s.cwd, 'existing.txt'), 'utf8')).toBe('replaced');
  });

  it('trims old large tool results with persistent context edits at the turn boundary', async () => {
    fs.writeFileSync(path.join(s.cwd, 'big.txt'), 'line of text\n'.repeat(1_000));
    const reads = Array.from({ length: 22 }, () => fauxToolCall('read', { path: 'big.txt' }));
    // The reads ran past any prompt-cache lifetime (the call message is an hour old), so the cache is cold and trims are due.
    const cold = () => fauxAssistantMessage(reads, { stopReason: 'toolUse', timestamp: Date.now() - 60 * 60_000 - 1 });
    const results = (await s.run('read a lot', [cold, say('done')])).at(-1)!.lastResults;
    expect(results).toHaveLength(22);
    // 13k-char results: six fit the 80k verbatim budget, the older 16 are trimmed.
    expect(results.slice(0, 16).every((result) => /were trimmed/.test(result.text))).toBe(true);
    expect(results.slice(16).every((result) => result.text.length > 10_000)).toBe(true);
    expect(s.session.sessionManager.getEntries().filter((entry) => entry.type === 'context_edit')).toHaveLength(16);
  });

  it('reports unavailable askUser without inventing an answer or approval', async () => {
    const question = { question: 'Which?', header: 'Pick', options: [{ label: 'A (Recommended)', description: 'a' }, { label: 'B', description: 'b' }] };
    const turns = await s.run('ask', [calls(fauxToolCall('askUser', { questions: [question] })), say('done')]);
    expect(turns.at(-1)!.lastResults[0]!.text).toMatch(/No interactive user/);
    expect(turns.at(-1)!.lastResults[0]!.text).toContain('No answer or approval was supplied.');
  });
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

  it("lets Pi summarize on the user's model, listing files the file tool changed", async () => {
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
    const entry = compactionEntries().at(-1)!;
    expect(entry.fromHook).not.toBe(true);
    expect(entry.details).toMatchObject({ readFiles: ['notes.txt'], modifiedFiles: ['todo.txt'] });
    expect(entry.summary).toContain('Pi summary');
    expect(entry.summary).toMatch(/<read-files>\nnotes\.txt\n<\/read-files>[\s\S]*<modified-files>\ntodo\.txt\n<\/modified-files>/);
  }, 60_000);

  it('makes a file read before compaction need a fresh read before it is changed', async () => {
    const write = () => calls(fauxToolCall('file', { queries: [{ reasoning: 'replace', type: 'write', path: 'notes.txt', content: 'x' }] }));
    const refused = await s.run('overwrite', [write(), say('done')]);
    expect(refused.at(-1)!.lastResults[0]!.text).toMatch(/FAILED write notes\.txt: .*Read it again/);
    const turns = await s.run('read, then overwrite', [calls(fauxToolCall('read', { path: 'notes.txt' })), write(), say('done')]);
    expect(turns.at(-1)!.lastResults[0]!.text).toMatch(/OK write notes\.txt/);
  }, 60_000);

  it('carries file-tool paths forward through the next Pi compaction', async () => {
    await s.run('more', [say(`Even more. ${'detail '.repeat(300)}`)]);
    script('## Goal\nNext summary');
    await s.session.compact();
    expect(compactionEntries().at(-1)!.details!['modifiedFiles']).toEqual(expect.arrayContaining(['todo.txt', 'notes.txt']));
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
    expect(turn.tools).toContain('mcp__octocode__localGetFileContent');
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
    expect(turn.systemPrompt).toContain('reserve shared files');
    const appPath = path.join(s.cwd, 'app.ts');
    const turns = await s.run('read', [
      calls(fauxToolCall('mcp__octocode__localGetFileContent', { queries: [{ goal: 'see value', reasoning: 'need current code', path: appPath, fullContent: true }] })),
      say('done'),
    ]);
    const results = turns.at(-1)!.lastResults;
    expect(results.find((result) => result.toolName === 'mcp__octocode__localGetFileContent')?.text).toContain('answer = 41');
  }, 60_000);
});
