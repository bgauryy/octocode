import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createPiFlowHarness } from '@octocodeai/agent-testing';
import extension from '../src/index.js';
import type { PiContext, PiInstance } from '../src/types.js';
import { renderAgentsProtocolInstructions } from '../src/tools/prompt-capabilities.js';
import { disposeCapabilityAdapters } from '../src/adapters/pi-capability-adapters.js';
import { disposeWorkerCapabilityRuntime } from '../src/tools/worker-capabilities.js';
import { installAuthenticatedWorkerCapabilityView } from './helpers/worker-capabilities.js';
import { discoverSkillCandidates, reviewSkillSource } from '../src/tools/skill-discovery.js';
import { openOctocodeDb } from '../src/tools/storage-policy.js';
import { setSkillEnabled } from '../src/contracts/mcp-state.js';
import type { ToolDefinition, SkillInfo } from '../src/types.js';
import { failedToolResult } from './helpers/failed-tool-result.js';

const roots: string[] = [];
const contexts: PiContext[] = [];
afterEach(async () => {
  for (const context of contexts.splice(0)) disposeCapabilityAdapters(context);
  await disposeWorkerCapabilityRuntime();
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

function fixture() {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'pi-agents-protocol-')));
  roots.push(root);
  const home = path.join(root, 'home');
  const octocodeHome = path.join(root, 'octocode');
  const repository = path.join(root, 'repository');
  const workspace = path.join(repository, 'packages', 'example');
  const piAgentDir = path.join(root, 'pi');
  const files = {
    global: path.join(home, '.agents', 'AGENTS.md'),
    octocode: path.join(octocodeHome, 'AGENTS.md'),
    repository: path.join(repository, '.agents', 'AGENTS.md'),
    workspace: path.join(workspace, '.agents', 'AGENTS.md'),
  };
  for (const [name, file] of Object.entries(files)) {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, `PROTOCOL_${name.toUpperCase()}_MARKER\nPreserve exact ${name} instruction bytes.\n`);
  }
  fs.mkdirSync(path.join(repository, '.git'));
  fs.mkdirSync(piAgentDir, { recursive: true });
  vi.spyOn(os, 'homedir').mockReturnValue(home);
  vi.stubEnv('OCTOCODE_HOME', octocodeHome);
  vi.stubEnv('OCTOCODE_AGENT_DIR', path.join(root, 'agent'));
  vi.stubEnv('PI_CODING_AGENT_DIR', piAgentDir);
  vi.stubEnv('CODEX_HOME', path.join(root, 'codex'));
  vi.stubEnv('OCTOCODE_STORAGE_MODE', 'memory');
  vi.stubEnv('OCTOCODE_PI_SUBAGENT', '');
  const context: PiContext = { cwd: workspace, hasUI: false, isProjectTrusted: () => true, sessionManager: { getSessionId: () => root, getBranch: () => [] } };
  contexts.push(context);
  return { root, home, octocodeHome, repository, workspace, piAgentDir, files, context };
}

const count = (text: string, marker: string): number => text.split(marker).length - 1;

describe('agents protocol prompt projection', { concurrent: false }, () => {
  it('refreshes skill revisions between turns and never reimports reviewed inventory as native Pi metadata', async () => {
    const f = fixture();
    vi.stubEnv('OCTOCODE_STORAGE_MODE', 'persistent');
    const file = path.join(f.workspace, '.claude', 'skills', 'reviewed-flow', 'SKILL.md');
    fs.mkdirSync(path.dirname(file), { recursive: true });
    const body = (description: string) => `---\nname: reviewed-flow\ndescription: ${description}\n---\nReviewed instructions.\n`;
    fs.writeFileSync(file, body('Reviewed original description'));
    openOctocodeDb();
    const candidate = discoverSkillCandidates(f.workspace, [], f.home, { trusted: true }).find(skill => skill.name === 'reviewed-flow')!;
    reviewSkillSource(f.workspace, candidate.sourceId, candidate.revision!, 'project', [], { trusted: true, homeDir: f.home });
    const flow = createPiFlowHarness({ cwd: f.workspace, sessionId: 'dynamic-skills' });
    const pi = flow.pi as unknown as PiInstance;
    const registered = new Map<string, ToolDefinition>();
    const register = pi.registerTool!.bind(pi);
    pi.registerTool = definition => { registered.set(definition.name, definition); register(definition); };
    await extension(pi);
    flow.pi.setActiveTools(['skill']);
    const handlers = flow.handlers as unknown as Map<string, Array<(event: unknown, context: unknown) => Promise<unknown>>>;
    let hostSkills: SkillInfo[] = [];
    const turn = async () => await handlers.get('before_agent_start')!.at(-1)!({ systemPrompt: 'Pi prompt', systemPromptOptions: { skills: hostSkills } }, f.context) as { systemPrompt: string };
    expect((await turn()).systemPrompt).toContain('Reviewed original description');
    fs.writeFileSync(file, body('Changed source needs another review'));
    const skillTool = registered.get('skill')!;
    const output = await failedToolResult(skillTool.execute('load', { queries: [{ reasoning: 'Check the current skill', type: 'load', name: 'reviewed-flow', reason: 'Continue this workflow' }] }, undefined, undefined, f.context));
    expect(output.isError).toBe(true);
    expect(JSON.stringify(output)).not.toContain('Changed source needs another review');
    expect((await turn()).systemPrompt).not.toContain('Reviewed original description');
    const nativeFile = path.join(f.workspace, '.agents', 'skills', 'native-flow', 'SKILL.md');
    fs.mkdirSync(path.dirname(nativeFile), { recursive: true });
    fs.writeFileSync(nativeFile, '---\nname: native-flow\ndescription: Native first revision\n---\nWorkflow.\n');
    const first = (await turn()).systemPrompt;
    expect(first).toContain('Native first revision');
    expect((await turn()).systemPrompt).toBe(first);
    fs.writeFileSync(nativeFile, '---\nname: native-flow\ndescription: Native second revision\n---\nChanged workflow.\n');
    expect((await turn()).systemPrompt).toContain('Native second revision');
    hostSkills = [{ name: 'native-flow', description: 'Host metadata for the same file', path: nativeFile, scope: 'project' }];
    await turn();
    const native = discoverSkillCandidates(f.workspace, [], f.home, { trusted: true }).find(skill => skill.name === 'native-flow')!;
    setSkillEnabled(openOctocodeDb(), path.resolve(f.workspace), 'native-flow', false, native.sourceId);
    const disabled = await failedToolResult(skillTool.execute('disabled-load', { queries: [{ reasoning: 'Check changed enablement', type: 'load', name: 'native-flow', reason: 'Continue this workflow' }] }, undefined, undefined, f.context));
    expect(disabled.isError).toBe(true);
    const alias = path.join(f.root, 'host-alias', 'SKILL.md');
    fs.mkdirSync(path.dirname(alias), { recursive: true });
    fs.symlinkSync(nativeFile, alias);
    hostSkills = [{ ...hostSkills[0]!, path: alias }];
    expect((await turn()).systemPrompt).not.toContain('Native second revision');
    fs.rmSync(nativeFile);
    expect((await turn()).systemPrompt).not.toContain('Native second revision');
  });
  it('renders trusted global and repository-to-workspace sources once with exact instruction bytes', () => {
    const f = fixture();
    const result = renderAgentsProtocolInstructions(f.context);
    expect(count(result, '<agents_protocol>')).toBe(1);
    expect(count(result, '</agents_protocol>')).toBe(1);
    const markers = ['GLOBAL', 'OCTOCODE', 'REPOSITORY', 'WORKSPACE'].map(name => `PROTOCOL_${name}_MARKER`);
    for (const marker of markers) expect(count(result, marker)).toBe(1);
    for (const file of Object.values(f.files)) expect(result).toContain(fs.readFileSync(file, 'utf8'));
    expect(markers.map(marker => result.indexOf(marker))).toEqual([...markers.map(marker => result.indexOf(marker))].sort((a, b) => a - b));
    expect(result).not.toContain('Source diagnostics:');
  });

  it('excludes canonical paths supplied by Pi and native Pi aliases without dropping other sources', () => {
    const f = fixture();
    const piContextFile = path.join(f.piAgentDir, 'AGENTS.md');
    fs.writeFileSync(piContextFile, 'PI_NATIVE_INSTRUCTION_MARKER');
    fs.rmSync(f.files.global);
    fs.symlinkSync(piContextFile, f.files.global);
    const suppliedAlias = path.join(f.root, 'provided-by-pi.md');
    fs.symlinkSync(f.files.workspace, suppliedAlias);
    const result = renderAgentsProtocolInstructions(f.context, [{ path: suppliedAlias }, null, 'invalid metadata', { path: 1 }]);
    expect(result).not.toContain('PI_NATIVE_INSTRUCTION_MARKER');
    expect(result).not.toContain('PROTOCOL_WORKSPACE_MARKER');
    expect(count(result, 'PROTOCOL_REPOSITORY_MARKER')).toBe(1);
    expect(count(result, 'PROTOCOL_OCTOCODE_MARKER')).toBe(1);
  });

  it('keeps global instructions while excluding untrusted workspace sources and suppresses all protocol output when requested', () => {
    const f = fixture();
    const result = renderAgentsProtocolInstructions({ ...f.context, isProjectTrusted: () => false });
    expect(result).toContain('PROTOCOL_GLOBAL_MARKER');
    expect(result).toContain('PROTOCOL_OCTOCODE_MARKER');
    expect(result).not.toContain('PROTOCOL_REPOSITORY_MARKER');
    expect(result).not.toContain('PROTOCOL_WORKSPACE_MARKER');
    expect(renderAgentsProtocolInstructions(f.context, [], true)).toBe('');
  });

  it('renders explicit source diagnostics without including oversized or non-file instruction content', () => {
    const f = fixture();
    fs.writeFileSync(f.files.repository, `OVERSIZED_MUST_NOT_RENDER${'x'.repeat(524_288)}`);
    fs.rmSync(f.files.workspace);
    fs.mkdirSync(f.files.workspace);
    const result = renderAgentsProtocolInstructions(f.context);
    expect(result).toContain('Source diagnostics:');
    expect(result).toContain('Instruction source exceeds 524288 bytes');
    expect(result).toContain('Instruction source must be a regular file');
    expect(result).not.toContain('OVERSIZED_MUST_NOT_RENDER');
    expect(result).toContain('PROTOCOL_GLOBAL_MARKER');
    expect(renderAgentsProtocolInstructions(f.context, [], true)).toBe('');
  });

  it('projects protocol instructions once through the real extension turn hook, preserves Pi exclusions, and replaces changed sources', async () => {
    const f = fixture();
    const flow = createPiFlowHarness({ cwd: f.workspace, sessionId: 'protocol-main' });
    await extension(flow.pi as unknown as PiInstance);
    flow.pi.setActiveTools([]);
    const handlers = flow.handlers as unknown as Map<string, Array<(event: unknown, context: unknown) => Promise<unknown>>>;
    const invoke = async (systemPrompt: string) => {
      const result = await handlers.get('before_agent_start')!.at(-1)!({ systemPrompt, systemPromptOptions: { skills: [], contextFiles: [{ path: f.files.workspace }] } }, f.context) as { systemPrompt?: string } | undefined;
      return result?.systemPrompt ?? systemPrompt;
    };
    const first = await invoke('Pi already supplied PROTOCOL_WORKSPACE_MARKER.');
    expect(count(first, '<agents_protocol>')).toBe(1);
    for (const marker of ['GLOBAL', 'OCTOCODE', 'REPOSITORY', 'WORKSPACE']) expect(count(first, `PROTOCOL_${marker}_MARKER`)).toBe(1);
    const second = await invoke(first);
    expect(second).toBe(first);
    fs.writeFileSync(f.files.repository, 'PROTOCOL_REPOSITORY_CHANGED_MARKER');
    const third = await invoke(second);
    expect(count(third, '<agents_protocol>')).toBe(1);
    expect(count(third, 'PROTOCOL_REPOSITORY_CHANGED_MARKER')).toBe(1);
    expect(third).not.toContain('PROTOCOL_REPOSITORY_MARKER');
  });

  for (const mode of ['worker', 'noContext'] as const) {
    it(`suppresses protocol files in the extension integration for ${mode}`, async () => {
      const f = fixture();
      const worker = mode === 'worker';
      if (worker) {
        vi.stubEnv('OCTOCODE_PI_SUBAGENT', '1');
        await installAuthenticatedWorkerCapabilityView([], false);
      }
      const flow = createPiFlowHarness({ cwd: f.workspace, sessionId: `protocol-${mode}` });
      const pi = flow.pi as unknown as PiInstance;
      pi.getFlag = name => name === 'no-context' && mode === 'noContext';
      await extension(pi);
      flow.pi.setActiveTools([]);
      const handlers = flow.handlers as unknown as Map<string, Array<(event: unknown, context: unknown) => Promise<unknown>>>;
      const result = await handlers.get('before_agent_start')!.at(-1)!({ systemPrompt: 'Pi role prompt', systemPromptOptions: { skills: [], contextFiles: [] } }, f.context) as { systemPrompt?: string } | undefined;
      expect(result?.systemPrompt).toEqual(expect.any(String));
      expect(result?.systemPrompt ?? 'Pi role prompt').not.toContain('<agents_protocol>');
      expect(result?.systemPrompt ?? 'Pi role prompt').not.toMatch(/PROTOCOL_(?:GLOBAL|OCTOCODE|REPOSITORY|WORKSPACE)_MARKER/);
    });
  }
});
