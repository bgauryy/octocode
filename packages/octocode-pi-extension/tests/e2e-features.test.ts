import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { createFauxCore, type fauxAssistantMessage, fauxToolCall, getCurrentSystemPrompt, getCurrentTools, type TranscriptContext } from '@earendil-works/pi-ai';
import { createAgentSession, DefaultResourceLoader, SessionManager, SettingsManager, type ExtensionFactory } from '@earendil-works/pi-coding-agent';
import { DEFAULT_BASH_TIMEOUT } from '../src/files/bash.js';
import { Team } from '../src/team/session.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { calls, E2E_ENTRY, say, tmp } from './helpers.js';

/**
 * README feature check through the real Pi runtime with a scripted model. Runs the source by default;
 * `OCTOCODE_E2E_ENTRY=dist` loads the built extension (`dist/index.js`) instead.
 */
const HERE = path.dirname(fileURLToPath(import.meta.url));
const octocode = (await import(E2E_ENTRY)).default as ExtensionFactory;

const savedEnv = { ...process.env };
const savedArgv = process.argv[1];
const root = fs.realpathSync(tmp('octocode-features-'));
const cwd = path.join(root, 'project');
const home = path.join(root, 'home');
const git = (...args: string[]) => execFileSync('git', args, { cwd, encoding: 'utf8' }).trim();

interface Turn {
  tools: string[];
  context: string;
  results: Array<{ toolName: string; isError: boolean; text: string }>;
}

function snapshot(context: TranscriptContext): Turn {
  const results: Turn['results'] = [];
  for (let index = context.messages.length - 1; index >= 0; index--) {
    const message = context.messages[index] as { role: string; toolName?: string; isError?: boolean; content?: unknown };
    if (message.role === 'system') continue;
    if (message.role !== 'toolResult') break;
    const text = Array.isArray(message.content) ? message.content.map((part: { text?: string }) => part.text ?? '').join('') : '';
    results.unshift({ toolName: String(message.toolName), isError: message.isError === true, text });
  }
  return { tools: getCurrentTools(context.messages).map((tool) => tool.name), context: `${getCurrentSystemPrompt(context.messages)}\n${JSON.stringify(context.messages)}`, results };
}

type Reply = () => ReturnType<typeof fauxAssistantMessage>;

let session: Awaited<ReturnType<typeof createAgentSession>>['session'];
let faux: ReturnType<typeof createFauxCore>;
const turns: Turn[] = [];
/** A run started and Pi has not emitted its `agent_settled` yet: a prompt sent now is deferred until the run settles, and may never reach the model. */
let unsettled = false;
const ui = fakeCtx({ cwd: root }).ui;

/**
 * Waits until no turn runs or settles. A turn started earlier (a delivered team message or a background report wakes
 * one, and a reload can start another) makes `prompt` throw "Agent is already processing" while it runs, and Pi defers
 * a prompt or command sent while it settles (`isStreaming` is already false then).
 */
const settled = () => vi.waitFor(() => expect(session.isStreaming || unsettled).toBe(false), { timeout: 10_000 });

async function run(prompt: string, replies: Reply[]): Promise<Turn[]> {
  const start = turns.length;
  await settled();
  faux.setResponses(replies.map((reply) => (context: TranscriptContext) => (turns.push(snapshot(context)), reply())));
  await session.prompt(prompt);
  return turns.slice(start);
}

const note = (pattern: RegExp) => ui.notes.find((entry) => pattern.test(entry.message))?.message;

beforeAll(async () => {
  fs.mkdirSync(cwd, { recursive: true });
  fs.mkdirSync(path.join(home, '.claude', 'skills', 'demo-skill'), { recursive: true });
  fs.writeFileSync(path.join(home, '.claude', 'skills', 'demo-skill', 'SKILL.md'), '---\nname: demo-skill\ndescription: Demo skill picked up from ~/.claude/skills.\n---\nBody.\n');
  // Claude Code hooks: PreToolUse blocks bash mentioning "forbidden", PostToolUse annotates, SessionStart adds context.
  const hook = path.join(home, 'hook.cjs');
  fs.writeFileSync(
    hook,
    `let input = ''; process.stdin.on('data', (c) => (input += c)).on('end', () => {
      const event = JSON.parse(input);
      if (event.hook_event_name === 'SessionStart') return console.log('hook says: project uses tabs');
      if (event.hook_event_name === 'PostToolUse') return console.log(JSON.stringify({ hookSpecificOutput: { additionalContext: 'post hook saw ' + event.tool_name } }));
      if (JSON.stringify(event.tool_input).includes('forbidden')) { console.error('no forbidden words'); process.exit(2); }
    });`,
  );
  const command = `${JSON.stringify(process.execPath)} ${JSON.stringify(hook)}`;
  const entry = (matcher: string) => [{ matcher, hooks: [{ type: 'command', command }] }];
  fs.writeFileSync(path.join(home, '.claude', 'settings.json'), JSON.stringify({ hooks: { PreToolUse: entry('Bash'), PostToolUse: entry('Bash'), SessionStart: entry('startup') } }));
  // A Pi-protected resource, so Pi resolves project trust without /octocode trust.
  fs.mkdirSync(path.join(cwd, '.pi'), { recursive: true });
  fs.writeFileSync(path.join(cwd, '.pi', 'settings.json'), '{}');
  fs.writeFileSync(path.join(cwd, 'app.ts'), 'export const answer = 41;\n');
  git('init', '-q', '-b', 'main');
  git('config', 'user.name', 'Test');
  git('config', 'user.email', 'test@example.com');
  git('add', '-A');
  git('commit', '-q', '-m', 'init');

  for (const key of Object.keys(process.env)) if (key.startsWith('OCTOCODE_')) delete process.env[key];
  Object.assign(process.env, { HOME: home, PI_CODING_AGENT_DIR: path.join(root, 'agent'), OCTOCODE_HOME: path.join(home, '.octocode'), OCTOCODE_AGENT_DB: path.join(root, 'team.sqlite'), OCTOCODE_HOOKS: '1', OCTOCODE_MCP: '0' });
  // Subagents re-run the current `pi` script: point it at the scripted stand-in.
  const bin = path.join(root, 'bin');
  fs.mkdirSync(bin);
  fs.copyFileSync(path.join(HERE, 'subagent-fake-pi.cjs'), path.join(bin, 'pi'));
  process.argv[1] = path.join(bin, 'pi');

  faux = createFauxCore({ provider: 'faux', models: [{ id: 'faux-1', contextWindow: 200_000, maxTokens: 4_096 }] as never });
  const provider: ExtensionFactory = (pi) => {
    pi.registerProvider('faux', { name: 'Faux', api: faux.api, baseUrl: 'http://127.0.0.1:0', apiKey: 'test', models: faux.models.map((model) => ({ ...model, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 } })), streamSimple: faux.streamSimple } as never);
  };
  const settingsManager = SettingsManager.inMemory({ compaction: { enabled: false }, retry: { enabled: false } });
  const resourceLoader = new DefaultResourceLoader({
    cwd,
    agentDir: path.join(root, 'agent'),
    settingsManager,
    extensionFactories: [
      { name: 'faux-provider', factory: provider },
      { name: 'octocode', factory: octocode },
    ],
    noPromptTemplates: true,
    noThemes: true,
    noContextFiles: true,
  });
  await resourceLoader.reload();
  ({ session } = await createAgentSession({ cwd, agentDir: path.join(root, 'agent'), resourceLoader, settingsManager, sessionManager: SessionManager.inMemory(cwd) }));
  session.subscribe((event) => {
    if (event.type === 'agent_start') unsettled = true;
    else if (event.type === 'agent_settled') unsettled = false;
  });
  await session.bindExtensions({ uiContext: ui, mode: 'rpc', shutdownHandler: () => undefined } as never);
  await session.setModel(session.modelRuntime.getModel('faux', 'faux-1')!);
}, 60_000);

afterAll(async () => {
  await session?.dispose?.();
  process.env = savedEnv;
  process.argv[1] = savedArgv!;
});

describe(`Octocode features through Pi (${path.relative(path.join(HERE, '..'), E2E_ENTRY)})`, () => {
  it('registers every tool and command, keeps the banner out of the session and picks up ~/.claude skills', async () => {
    const [turn] = await run('hello', [say('ready')]);
    for (const name of ['read', 'file', 'bash', 'web', 'agent', 'askUser', 'coordinate', 'sendMessage', 'backlog', 'memory']) expect(turn!.tools).toContain(name);
    // The bundled web profiles carry browser, so the main session leaves its schema out (it stays registered).
    expect(turn!.tools).not.toContain('browser');
    expect(session.getAllTools().map((tool) => tool.name)).toContain('browser');
    const commands = session.extensionRunner.getRegisteredCommands().map((command) => command.name);
    for (const name of ['octocode', 'agents', 'hooks', 'sessions', 'backlog', 'memory']) expect(commands).toContain(name);
    // Registering `/mcp` would make Pi drop its built-in MCP (builtin:mcp); ours is `/octocode mcp`.
    expect(commands).not.toContain('mcp');
    // Everything else is an /octocode subcommand; the old top-level commands are gone.
    for (const name of ['tell', 'kill', 'merge', 'rewind', 'review', 'api']) expect(commands).not.toContain(name);
    expect(session.systemPrompt).toContain('demo-skill');
    expect(turn!.context).toMatch(/Delegation/);
    // The banner is the TUI header (none over RPC), never a session entry.
    expect(session.sessionManager.getEntries().some((entry) => entry.type === 'custom')).toBe(false);
    expect((ui as { header?: unknown }).header).toBeUndefined();
    expect((ui as { title?: string }).title).toBeUndefined();
    // SessionStart hook output reaches the model once, as data.
    expect(turn!.context).toContain('hook says: project uses tabs');
  });

  it('runs /octocode and its subcommands', async () => {
    await session.prompt('/octocode');
    expect(note(/^Octocode /)).toMatch(/Octocode MCP: off/);
    expect(note(/^Octocode /)).toMatch(/Subagents: .*researcher/);
    expect(note(/^Octocode /)).toMatch(/Skills: [1-9]\d*/);
    expect(note(/^Octocode /)).toMatch(/Hooks: (off|on)/);
    await session.prompt('/hooks');
    expect(note(/^Hooks: /)).toMatch(/Hook files[\s\S]*settings\.json/);
    await session.prompt('/octocode help');
    expect(note(/^Usage: \/octocode/)).toMatch(/api \[on[\s\S]*permission \[manual\|workspace\|full[\s\S]*rewind/);
    expect(note(/^Usage: \/octocode/)).not.toMatch(/cua/);
    // /agents is a shortcut for /octocode agents, listed with the rest.
    expect(note(/^Usage: \/octocode/)).toMatch(/agents/);
    for (const command of ['/octocode agents', '/agents']) {
      const before = ui.notes.length;
      await session.prompt(command);
      expect(ui.notes.length).toBeGreaterThan(before);
    }
    // A mode change reaches the model as one hidden message, never through the cached system prompt.
    const prompt = session.systemPrompt;
    await session.prompt('/octocode permission manual');
    expect(session.systemPrompt).toBe(prompt);
    await session.prompt('/octocode permission');
    expect(note(/^Permission mode: manual —/)).toMatch(/not a sandbox/);
    await session.prompt('/octocode permission workspace');
    const notes = session.sessionManager.getEntries().filter((entry) => entry.type === 'custom_message' && entry.customType === 'octocode-permission');
    expect(notes.length).toBeGreaterThan(0);
    expect(notes.every((entry) => entry.type === 'custom_message' && entry.display === false)).toBe(true);
    expect(session.systemPrompt).toBe(prompt);
  });

  it('guards bash: a destructive command asks, a hook-blocked command is refused, hooks annotate results', async () => {
    ui.selects.push('Deny');
    const [, second] = await run('bash', [calls(fauxToolCall('bash', { command: 'rm -rf /' }), fauxToolCall('bash', { command: 'echo forbidden' }), fauxToolCall('bash', { command: 'echo fine' })), say('done')]);
    const [danger, blocked, fine] = second!.results;
    expect(danger).toMatchObject({ isError: true });
    expect(danger!.text).toMatch(/The user denied the destructive shell command: rm -rf \//);
    expect(blocked).toMatchObject({ isError: true });
    expect(blocked!.text).toMatch(/PreToolUse hook: no forbidden words/);
    expect(fine!.text).toMatch(/fine[\s\S]*\[PostToolUse hook\] post hook saw Bash/);
  });

  it('replaces Pi bash: a deadline in its schema, and a background job wakes the session with its result', async () => {
    expect(JSON.stringify(session.getAllTools().find((tool) => tool.name === 'bash')?.parameters)).toContain(`foreground default ${DEFAULT_BASH_TIMEOUT}s`);
    const start = turns.length;
    await run('bash in the background', [calls(fauxToolCall('bash', { command: 'echo bg-output', background: true })), say('started'), say('saw it')]);
    const deadline = Date.now() + 5_000;
    while (turns.length < start + 3 && Date.now() < deadline) await new Promise((resolve) => setTimeout(resolve, 20));
    const [, started, woken] = turns.slice(start);
    expect(started!.results[0]!.text).toMatch(/^Started bash-\d+ in the background/);
    expect(woken!.context).toMatch(/Background bash bash-\d+ finished after \d+s: echo bg-output[\s\S]*bg-output/);
  });

  it('asks before web fetches a loopback URL, and fetches nothing when denied', async () => {
    ui.selects.push('Deny');
    const [, second] = await run('web', [calls(fauxToolCall('web', { url: 'http://127.0.0.1:9/secret' })), say('done')]);
    expect(second!.results[0]).toMatchObject({ toolName: 'web', isError: true });
    expect(second!.results[0]!.text).toMatch(/The user denied opening a private or local address: http:\/\/127\.0\.0\.1:9\/secret/);
  });

  it('rejects ambiguous web requests through Pi instead of choosing one operation', async () => {
    const [, second] = await run('web', [calls(fauxToolCall('web', { url: 'https://example.com', query: 'example' })), say('done')]);
    expect(second!.results[0]).toMatchObject({ toolName: 'web', isError: true });
    expect(second!.results[0]!.text).toContain('Provide exactly one of url or query.');
  });

  it('checkpoints file changes and /octocode rewind restores them', async () => {
    await run('edit', [calls(fauxToolCall('read', { path: 'app.ts' })), calls(fauxToolCall('file', { queries: [{ reasoning: 'bump', type: 'edit', path: 'app.ts', edits: [{ oldText: '41', newText: '42' }] }] })), say('done')]);
    expect(fs.readFileSync(path.join(cwd, 'app.ts'), 'utf8')).toBe('export const answer = 42;\n');
    await session.prompt('/octocode rewind');
    expect(fs.readFileSync(path.join(cwd, 'app.ts'), 'utf8')).toBe('export const answer = 41;\n');
    expect(note(/app\.ts/)).toBeDefined();
  });

  it('asks the user through RPC dialogs', async () => {
    ui.selects.push('2. B — b');
    const question = { question: 'Which?', header: 'Pick', options: [{ label: 'A (Recommended)', description: 'a' }, { label: 'B', description: 'b' }] };
    const [, second] = await run('ask', [calls(fauxToolCall('askUser', { questions: [question] })), say('done')]);
    expect(second!.results[0]!.text).toMatch(/Which\?[\s\S]*B/);
  });

  it('coordinates with a peer: locks, refused edits on held paths and messages both ways', async () => {
    const peerPi = fakePi();
    const peer = new Team(peerPi.pi);
    peer.start(fakeCtx({ cwd }));
    peer.join('peer reviewer');
    expect(peer.lock(['held.txt'], 'review').ok).toBe(true);
    const replies = [
      calls(fauxToolCall('coordinate', { action: 'join', goal: 'main' })),
      calls(fauxToolCall('coordinate', { action: 'lock', paths: ['held.txt', 'mine.txt'] })),
      calls(fauxToolCall('file', { queries: [{ reasoning: 'try', type: 'write', path: 'held.txt', content: 'x' }] })),
      calls(fauxToolCall('coordinate', { action: 'list' })),
      calls(fauxToolCall('sendMessage', { to: peer.id!, message: 'please release held.txt', replyRequired: false })),
      say('done'),
    ];
    const t = await run('team', replies);
    expect(t[1]!.results[0]!.text).toMatch(/Joined/i);
    expect(t[2]!.results[0]).toMatchObject({ isError: true });
    expect(t[2]!.results[0]!.text).toContain(peer.id);
    expect(t[3]!.results[0]!.text).toMatch(/held\.txt/);
    expect(t[4]!.results[0]!.text).toContain(peer.id);
    expect(t[5]!.results[0]!.isError).toBe(false);
    await vi.waitFor(() => expect(JSON.stringify(peerPi.sent)).toContain('please release held.txt'), { timeout: 5_000 });
    const requests = turns.length;
    await session.prompt(`/agents tell ${peer.id} ${'q'.repeat(9_000)}`);
    // Over the limit it is delivered, not refused: the head and the path of a file holding all 9,000 characters.
    await vi.waitFor(() => expect(JSON.stringify(peerPi.sent)).toContain('[Full message (9000 characters): '), { timeout: 5_000 });
    expect(turns).toHaveLength(requests);
    peer.leave();
  });

  it('runs a subagent and an isolated subagent whose changes /agents merge brings in', async () => {
    const t = await run('delegate', [calls(fauxToolCall('agent', { goal: 'check things', task: 'report facts', profile: 'researcher' }), fauxToolCall('agent', { goal: 'check things', task: 'edit in a worktree', isolate: true })), say('done')]);
    const [plain, isolated] = t.at(-1)!.results;
    expect(plain!.text).toMatch(/report \{/);
    expect(plain!.text).toMatch(/"subagent":"1"/);
    expect(isolated!.text).toMatch(/refs\/octocode\/pi\//);
    expect(fs.existsSync(path.join(cwd, 'child.txt'))).toBe(false);
    const id = /refs\/octocode\/pi\/([\w-]+)/.exec(isolated!.text)![1]!;
    await session.prompt(`/agents merge ${id}`);
    expect(fs.readFileSync(path.join(cwd, 'child.txt'), 'utf8')).toBe('from the child\n');
  }, 60_000);

  it('bounds a long child report through the real Pi tool flow and preserves it on disk', async () => {
    const t = await run('long delegation', [calls(fauxToolCall('agent', { goal: 'check things', task: 'long report', profile: 'researcher' })), say('done')]);
    const result = t.at(-1)!.results[0]!;
    expect(result.isError).toBe(false);
    expect(Buffer.byteLength(result.text)).toBeLessThan(9 * 1024);
    const file = /Full report: (.+) \(read only the parts you need\)/.exec(result.text)![1]!;
    expect(fs.readFileSync(file, 'utf8')).toMatch(/the end$/);
  }, 60_000);

  it('runs background subagents: answers arrive as messages and /agents kill stops one', async () => {
    const before = turns.length;
    const t = await run('background', [calls(fauxToolCall('agent', { goal: 'check things', task: 'report later', background: true }), fauxToolCall('agent', { goal: 'check things', task: 'slow job', background: true })), say('waiting'), say('got it'), say('ok'), say('ok')]);
    const [later, slow] = t[1]!.results;
    const laterId = /^Started (\S+) .* in the background/.exec(later!.text)![1]!;
    const slowId = /^Started (\S+) .* in the background/.exec(slow!.text)![1]!;
    // The finished child's own report reaches the model in a turn of this test (it wakes the session when it arrives
    // after the run). Earlier tests' reports are in the transcript too: only this child's id proves its report arrived.
    const report = new RegExp(`Background subagent ${laterId} \\([^)]*\\) (reported \\w+|finished) \\(`);
    await vi.waitFor(() => expect(turns.slice(before).some((turn) => report.test(turn.context))).toBe(true), { timeout: 20_000 });
    await settled();
    await session.prompt('/agents kill');
    expect(note(/Running: /)).toBeDefined();
    // The reported child is done: the slow one alone still runs.
    expect(/Running: (.+)$/.exec(note(/Running: /)!)![1]!.split(', ')).toEqual([slowId]);
    await session.prompt(`/agents kill ${slowId}`);
    expect(note(new RegExp(`^Stopping ${slowId}`))).toBeDefined();
    // A stop the user asked for wakes no turn: its report is delivered as a session message.
    const cancelled = new RegExp(`Background subagent ${slowId} \\([^)]*\\) cancelled by the user`);
    const delivered = () => session.sessionManager.getEntries().flatMap((entry) => (entry.type === 'custom_message' && entry.customType === 'octocode-agent-result' ? [JSON.stringify(entry.content)] : []));
    await vi.waitFor(() => expect(delivered().some((content) => cancelled.test(content))).toBe(true), { timeout: 20_000 });
  }, 60_000);

  it('tracks backlog items and memories with the tools, refuses secrets, and recalls memories in later prompts', async () => {
    const t = await run('track work', [
      calls(fauxToolCall('backlog', { op: 'add', title: 'Raise the answer to 43', goal: 'The answer constant is 43; tests pass.', body: 'app.ts holds it', priority: 'p1' })),
      calls(fauxToolCall('backlog', { op: 'list', states: ['backlog'] })),
      calls(fauxToolCall('backlog', { op: 'update', id: 'B1', state: 'ongoing' })),
      calls(fauxToolCall('backlog', { op: 'update', id: 'B1', state: 'done' })),
      calls(fauxToolCall('backlog', { op: 'update', id: 'B1', state: 'done', note: 'Changed app.ts; verified by reading it back.' })),
      calls(fauxToolCall('memory', { op: 'set', title: 'The answer constant lives in app.ts', body: 'Exported as answer from app.ts at the repository root.', keywords: 'answer constant export' })),
      calls(fauxToolCall('memory', { op: 'search', query: 'answer constant' })),
      calls(fauxToolCall('memory', { op: 'set', title: 'Deploy token', body: `use ghp_${'a'.repeat(36)}` })),
      say('done'),
    ]);
    const [added, listed, claimed, noNote, done, saved, found, secret] = t.slice(1).map((turn) => turn.results[0]!);
    expect(added!.text).toMatch(/^Added B1\b/);
    expect(listed!.text).toContain('B1');
    expect(claimed!.text).toMatch(/^Updated B1\b/);
    expect(noNote).toMatchObject({ isError: true });
    expect(noNote!.text).toMatch(/needs a note/);
    expect(done).toMatchObject({ isError: false });
    expect(saved!.text).toMatch(/^Saved M\d+/);
    expect(found!.text).toContain('The answer constant lives in app.ts');
    expect(secret).toMatchObject({ isError: true });
    expect(secret!.text).toMatch(/GitHub token/);
    // The next prompt that mentions the topic gets the memory injected as data.
    const [next] = await run('where is the answer constant exported?', [say('in app.ts')]);
    expect(next!.context).toContain('[octocode-memory');
    expect(next!.context).toContain('The answer constant lives in app.ts');
  });

  it('prints /octocode backlog, memory and sessions as text without a UI', async () => {
    const runner = session.extensionRunner;
    runner.setUIContext(undefined, 'print');
    const notify = vi.spyOn(runner.getUIContext(), 'notify');
    let out: string[];
    try {
      await session.prompt('/octocode backlog add Document the board');
      await session.prompt('/octocode backlog B2 todo');
      await session.prompt('/octocode backlog');
      await session.prompt('/octocode memory add Tabs everywhere — the project indents with tabs');
      await session.prompt('/octocode memory');
      await session.prompt('/octocode memory search answer');
      await session.prompt('/octocode sessions');
      out = notify.mock.calls.map((call) => String(call[0]));
    } finally {
      notify.mockRestore();
      runner.setUIContext(ui, 'rpc');
    }
    const board = out.find((text) => /^Todo \(|^Done \(|^Ongoing \(/m.test(text) && text.includes('B1'))!;
    expect(board).toMatch(/Todo \(1\)[\s\S]*B2[\s\S]*Document the board[\s\S]*Done \(1\)[\s\S]*B1/);
    expect(out.some((text) => /^Moved B2\b/.test(text))).toBe(true);
    expect(out.some((text) => /^Saved M\d+ .*Tabs everywhere/.test(text))).toBe(true);
    const listed = out.find((text) => text.startsWith('Memories (auto on)'))!;
    expect(listed).toMatch(/^Project:$/m);
    expect(listed).toContain('Tabs everywhere');
    expect(listed).toContain('The answer constant lives in app.ts');
    expect(out.find((text) => text.startsWith('Memories matching "answer"'))).toContain('The answer constant lives in app.ts');
    expect(out.find((text) => text.startsWith('Sessions ('))).toContain(session.sessionManager.getSessionId());
  });

  it('restarts cleanly on reload (shutdown, then a fresh session_start)', async () => {
    await session.reload();
    const [turn] = await run('after reload', [say('ok')]);
    expect(turn!.tools).toContain('agent');
  });
});
