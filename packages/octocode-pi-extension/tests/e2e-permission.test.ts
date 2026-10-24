import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { afterAll, afterEach, describe, expect, it } from 'vitest';
import { createFauxCore, type fauxAssistantMessage, fauxToolCall, type TranscriptContext } from '@earendil-works/pi-ai';
import { createAgentSession, DefaultResourceLoader, SessionManager, SettingsManager, type ExtensionFactory } from '@earendil-works/pi-coding-agent';
import { NEEDS_APPROVAL } from '../src/permission/dialog.js';
import { calls, E2E_ENTRY, say, tmp } from './helpers.js';

/**
 * Permission modes through the real Pi runtime with a scripted model, bound without a UI as a headless run is (print
 * and JSON mode, every subagent). Runs the source by default; `OCTOCODE_E2E_ENTRY=dist` loads the built extension.
 */
const octocode = (await import(E2E_ENTRY)).default as ExtensionFactory;

const savedEnv = { ...process.env };
const sessions: Array<{ dispose(): void }> = [];

type Result = { toolName: string; isError: boolean; text: string };

/** The tool results the model was sent in the turn after a call. */
function results(context: TranscriptContext): Result[] {
  const found: Result[] = [];
  for (let index = context.messages.length - 1; index >= 0; index--) {
    const message = context.messages[index] as { role: string; toolName?: string; isError?: boolean; content?: unknown };
    if (message.role === 'system') continue;
    if (message.role !== 'toolResult') break;
    const text = Array.isArray(message.content) ? message.content.map((part: { text?: string }) => part.text ?? '').join('') : '';
    found.unshift({ toolName: String(message.toolName), isError: message.isError === true, text });
  }
  return found;
}

/** A headless Pi session in a fresh git repository with this env; returns the repository and a run helper. */
async function headless(env: Record<string, string>) {
  const root = fs.realpathSync(tmp('octocode-e2e-permission-'));
  const cwd = path.join(root, 'project');
  const home = path.join(root, 'home');
  fs.mkdirSync(cwd, { recursive: true });
  fs.mkdirSync(home, { recursive: true });
  // A Pi-protected resource, so Pi resolves project trust without /octocode trust.
  fs.mkdirSync(path.join(cwd, '.pi'), { recursive: true });
  fs.writeFileSync(path.join(cwd, '.pi', 'settings.json'), '{}');
  fs.writeFileSync(path.join(cwd, 'app.ts'), 'export const answer = 41;\n');
  const git = (...args: string[]) => execFileSync('git', args, { cwd, encoding: 'utf8' });
  git('init', '-q', '-b', 'main');
  git('-c', 'user.name=Test', '-c', 'user.email=test@example.com', 'commit', '-q', '--allow-empty', '-m', 'init');

  // The extension reads the environment when the session starts: change the same object, never replace it.
  for (const key of Object.keys(process.env)) if (key.startsWith('OCTOCODE_')) delete process.env[key];
  Object.assign(process.env, { HOME: home, PI_CODING_AGENT_DIR: path.join(root, 'agent'), OCTOCODE_HOME: path.join(home, '.octocode'), OCTOCODE_AGENT_DB: path.join(root, 'team.sqlite'), OCTOCODE_MCP: '0' }, env);
  // The user trusted this project with Pi's /trust (`full` needs a saved decision).
  fs.mkdirSync(path.join(root, 'agent'), { recursive: true });
  fs.writeFileSync(path.join(root, 'agent', 'trust.json'), JSON.stringify({ [fs.realpathSync(root)]: true }));

  const faux = createFauxCore({ provider: 'faux', models: [{ id: 'faux-1', contextWindow: 200_000, maxTokens: 4_096 }] as never });
  const provider: ExtensionFactory = (pi) => {
    pi.registerProvider('faux', { name: 'Faux', api: faux.api, baseUrl: 'http://127.0.0.1:0', apiKey: 'test', models: faux.models.map((model) => ({ ...model, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 } })), streamSimple: faux.streamSimple } as never);
  };
  const settingsManager = SettingsManager.inMemory({ compaction: { enabled: false }, retry: { enabled: false } });
  const resourceLoader = new DefaultResourceLoader({
    cwd,
    agentDir: path.join(root, 'agent'),
    settingsManager,
    extensionFactories: [{ name: 'faux-provider', factory: provider }, { name: 'octocode', factory: octocode }],
    noPromptTemplates: true,
    noThemes: true,
    noContextFiles: true,
  });
  await resourceLoader.reload();
  const { session } = await createAgentSession({ cwd, agentDir: path.join(root, 'agent'), resourceLoader, settingsManager, sessionManager: SessionManager.inMemory(cwd) });
  sessions.push(session);
  // No uiContext: ctx.hasUI is false, as in print and JSON mode.
  await session.bindExtensions({ mode: 'json', shutdownHandler: () => undefined } as never);
  await session.setModel(session.modelRuntime.getModel('faux', 'faux-1')!);

  const turns: Result[][] = [];
  const run = async (prompt: string, replies: Array<() => ReturnType<typeof fauxAssistantMessage>>) => {
    const start = turns.length;
    faux.setResponses(replies.map((reply) => (context: TranscriptContext) => (turns.push(results(context)), reply())));
    await session.prompt(prompt);
    return turns.slice(start);
  };
  return { cwd, home, run, session };
}

afterEach(() => {
  for (const session of sessions.splice(0)) session.dispose();
});

afterAll(() => {
  for (const key of Object.keys(process.env)) delete process.env[key];
  Object.assign(process.env, savedEnv);
});

describe('permission modes in a headless run', () => {
  it('denies every ask of a subagent in manual mode and tells it to report the blocker, without running anything', async () => {
    const { cwd, run } = await headless({ OCTOCODE_SUBAGENT: '1', OCTOCODE_AGENT_ID: 'general-e2e001', OCTOCODE_PERMISSION: 'manual' });
    const [, after] = await run('change things', [
      calls(
        fauxToolCall('file', { queries: [{ reasoning: 'add', type: 'write', path: 'new.txt', content: 'x' }] }),
        fauxToolCall('bash', { command: 'echo ran > ran.txt' }),
      ),
      say('blocked'),
    ]);
    expect(after!.map((result) => result.toolName)).toEqual(['file', 'bash']);
    for (const result of after!) {
      expect(result.isError).toBe(true);
      expect(result.text.startsWith(NEEDS_APPROVAL)).toBe(true);
      expect(result.text).toContain('report it to your parent as SBAR');
    }
    expect(fs.existsSync(path.join(cwd, 'new.txt'))).toBe(false);
    expect(fs.existsSync(path.join(cwd, 'ran.txt'))).toBe(false);
  }, 60_000);

  // `mkfs.zzz` matches the catastrophic patterns and is not an installed command: the shell reports it missing and nothing is formatted.
  const DESTRUCTIVE = 'rm app.ts; mkfs.zzz /dev/null';

  it('runs a catastrophic command in full, audited as shell.destructive, and keeps in-repo work unasked', async () => {
    const { cwd, run, session } = await headless({ OCTOCODE_PERMISSION: 'full' });
    const [, after] = await run('go', [
      calls(
        fauxToolCall('bash', { command: DESTRUCTIVE }),
        fauxToolCall('file', { queries: [{ reasoning: 'add', type: 'write', path: 'new.txt', content: 'x' }] }),
      ),
      say('done'),
    ]);
    expect(after![0]!.text).not.toContain(NEEDS_APPROVAL);
    expect(fs.existsSync(path.join(cwd, 'app.ts'))).toBe(false);
    expect(after![1]).toMatchObject({ toolName: 'file', isError: false });
    expect(fs.readFileSync(path.join(cwd, 'new.txt'), 'utf8')).toBe('x');
    const audit = session.sessionManager.getEntries().flatMap((entry) => (entry.type === 'custom' && entry.customType === 'octocode-permission' ? [entry.data as { action: string; allowed: boolean; via: string; mode: string }] : []));
    expect(audit).toContainEqual(expect.objectContaining({ action: 'shell.destructive', allowed: true, via: 'mode', mode: 'full' }));
    expect(audit).toContainEqual(expect.objectContaining({ action: 'file.workspace', allowed: true, via: 'mode', mode: 'full' }));
  }, 60_000);

  it('refuses a catastrophic command in workspace with no one to ask, and runs none of it', async () => {
    const { cwd, run } = await headless({ OCTOCODE_PERMISSION: 'workspace' });
    const [, after] = await run('go', [calls(fauxToolCall('bash', { command: DESTRUCTIVE })), say('blocked')]);
    expect(after![0]!.isError).toBe(true);
    expect(after![0]!.text.startsWith(NEEDS_APPROVAL)).toBe(true);
    expect(after![0]!.text).toContain('OCTOCODE_PERMISSION=full');
    expect(fs.existsSync(path.join(cwd, 'app.ts'))).toBe(true);
  }, 60_000);

  it('denies an ask in a headless main session and names the mode that would allow it', async () => {
    const { run } = await headless({ OCTOCODE_PERMISSION: 'manual' });
    const [, after] = await run('run', [calls(fauxToolCall('bash', { command: 'echo hi' })), say('blocked')]);
    expect(after![0]!.isError).toBe(true);
    expect(after![0]!.text.startsWith(NEEDS_APPROVAL)).toBe(true);
    expect(after![0]!.text).toContain('OCTOCODE_PERMISSION=workspace');
  }, 60_000);
});
