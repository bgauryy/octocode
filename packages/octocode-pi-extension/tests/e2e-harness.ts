import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createFauxCore, type fauxAssistantMessage, getCurrentSystemPrompt, getCurrentTools, type TranscriptContext } from '@earendil-works/pi-ai';
import { createAgentSession, createMcpExtension, createToolSearchExtension, DefaultResourceLoader, SessionManager, SettingsManager, type ExtensionFactory } from '@earendil-works/pi-coding-agent';
import { E2E_ENTRY } from './helpers.js';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const octocode: ExtensionFactory = (await import(E2E_ENTRY)).default;

// Hermetic: a run from inside a Pi subagent inherits its identity and settings, which change the prompt, tools and limits.
for (const key of ['OCTOCODE_SUBAGENT', 'OCTOCODE_BROWSER_VISIBLE', 'OCTOCODE_PARENT_ID', 'OCTOCODE_AGENT_ID', 'OCTOCODE_AGENT_GOAL', 'OCTOCODE_AGENT_COLLABORATE', 'OCTOCODE_MAX_SUBAGENTS', 'OCTOCODE_HOME', 'OCTOCODE_AGENT_DB', 'OCTOCODE_MEMORY_AUTO', 'OCTOCODE_MCP']) {
  delete process.env[key];
}

export const FIXTURE = path.join(HERE, 'fixtures', 'echo-mcp.mjs');

// The extension launches its `octocode-mcp` dependency as is: in this monorepo the pinned version resolves to the
// workspace build (`packages/octocode-mcp`), so the e2e runs the server source and release share.

export interface Turn {
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

export type Reply = ReturnType<typeof fauxAssistantMessage>;

/** A real Pi agent session running the Octocode extension against a scripted model. */
export async function startSession(mcpServers: Record<string, unknown>, options: { tools?: string[]; settings?: Record<string, unknown>; agentSettings?: Record<string, unknown>; models?: Array<{ id: string; inputCost: number; maxTokens: number }> } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-e2e-'));
  const cwd = path.join(root, 'project');
  const home = path.join(root, 'home');
  fs.mkdirSync(cwd, { recursive: true });
  fs.mkdirSync(home, { recursive: true });
  const agentDir = path.join(root, 'agent');
  fs.mkdirSync(agentDir, { recursive: true });
  if (options.agentSettings) fs.writeFileSync(path.join(agentDir, 'settings.json'), JSON.stringify(options.agentSettings));
  // The user trusted this project with Pi's /trust.
  fs.writeFileSync(path.join(agentDir, 'trust.json'), JSON.stringify({ [fs.realpathSync(root)]: true }));
  const originalHome = process.env['HOME'];
  const originalAgentDir = process.env['PI_CODING_AGENT_DIR'];
  const originalPermission = process.env['OCTOCODE_PERMISSION'];
  process.env['HOME'] = home; // keep the developer's own config out of the test
  process.env['PI_CODING_AGENT_DIR'] = agentDir; // and their Pi settings
  // Sessions start in manual, and a headless one denies every ask: this user exported workspace.
  process.env['OCTOCODE_PERMISSION'] = 'workspace';
  // Pi's built-in MCP reads the user-level mcp.json; an `octocode` entry there overrides Octocode's registration.
  fs.writeFileSync(path.join(agentDir, 'mcp.json'), JSON.stringify({ mcpServers }));
  const turns: Turn[] = [];
  const models = options.models ?? [{ id: 'faux-1', inputCost: 0, maxTokens: 4_096 }];
  const faux = createFauxCore({ provider: 'faux', models: models.map(({ id, maxTokens }) => ({ id, contextWindow: 200_000, maxTokens })) });
  const provider: ExtensionFactory = (pi) => {
    pi.registerProvider('faux', {
      name: 'Faux',
      api: faux.api,
      baseUrl: 'http://127.0.0.1:0',
      apiKey: 'test',
      models: faux.models.map((model, index) => ({ ...model, cost: { input: models[index].inputCost, output: 0, cacheRead: 0, cacheWrite: 0 } })),
      streamSimple: faux.streamSimple,
    });
  };
  const settingsManager = SettingsManager.inMemory({ compaction: { enabled: false }, retry: { enabled: false }, ...options.settings });
  const resourceLoader = new DefaultResourceLoader({
    cwd,
    agentDir: path.join(root, 'agent'),
    settingsManager,
    extensionFactories: [
      { name: 'faux-provider', factory: provider },
      { name: 'octocode', factory: octocode },
      // The CLI loads Pi's MCP as a built-in extension, after the user's; SDK sessions add it themselves.
      { name: 'mcp', factory: createMcpExtension({ logPath: path.join(root, 'mcp.log') }) },
      // Also built in on the CLI: it loads the tools of a user's deferred servers (Octocode declares all of its own).
      { name: 'tool-search', factory: createToolSearchExtension() },
    ],
    noSkills: true,
    noPromptTemplates: true,
    noThemes: true,
    noContextFiles: true,
  });
  await resourceLoader.reload();
  const { session } = await createAgentSession({ cwd, agentDir: path.join(root, 'agent'), resourceLoader, settingsManager, sessionManager: SessionManager.inMemory(cwd), ...(options.tools ? { tools: options.tools } : {}) });
  await session.bindExtensions({ mode: 'json', shutdownHandler: () => undefined });
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
    session.dispose();
    process.env['HOME'] = originalHome;
    if (originalAgentDir === undefined) delete process.env['PI_CODING_AGENT_DIR'];
    else process.env['PI_CODING_AGENT_DIR'] = originalAgentDir;
    if (originalPermission === undefined) delete process.env['OCTOCODE_PERMISSION'];
    else process.env['OCTOCODE_PERMISSION'] = originalPermission;
    fs.rmSync(root, { recursive: true, force: true });
  };
  return { cwd, session, faux, run, dispose };
}

/** An mcp.json `octocode` entry that keeps the server off (a file entry wins over Octocode's registration). */
export const OCTOCODE_OFF = { octocode: { command: 'false', enabled: false } };
