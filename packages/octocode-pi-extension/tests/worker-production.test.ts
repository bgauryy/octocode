import { expect, it } from 'vitest';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { createWorkerMcpBroker, type WorkerBrokerBinding } from '../src/tools/mcp/broker.js';
import type { CapabilitySnapshot } from '@octocodeai/agent-contracts/capabilities';
import { getAwarenessOperationDescriptor } from '@octocodeai/octocode-awareness';

it('lets an Awareness-only worker discover and execute the native API without shell access or duplicate standing policy', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-awareness-worker-'));
  const snapshot: CapabilitySnapshot = { schemaVersion: 1, revision: 'awareness-only', nativeTools: ['awareness'], skills: [], mcpTools: [] };
  const broker = await createWorkerMcpBroker({ snapshot, dispatchMcp: async () => { throw new Error('No MCP grant'); } });
  try {
    const binding = broker.registerWorker('awareness-child', { nativeTools: ['awareness'], skills: [], mcpTools: [] });
    const { receipt } = await runWorker(root, binding, 'index.js', 'awareness', [
      { name: 'awareness', arguments: { queries: [{ reasoning: 'Inspect unfamiliar fields', operation: 'message.send', describe: true }] } },
      { name: 'awareness', arguments: { queries: [{ reasoning: 'Read bound context', operation: 'context.orient' }] } },
    ], 'high');
    expect(receipt.activeTools).toEqual(['awareness']);
    expect(receipt.reasoning).toBe('high');
    expect(receipt.awarenessPolicyCopies).toBe(1);
    expect(receipt.results).toHaveLength(2);
    expect(receipt.results.map((result: { isError: boolean }) => result.isError), JSON.stringify(receipt.results)).toEqual([false, false]);
    expect(JSON.parse(receipt.results[0].text).inputSchema).toEqual(getAwarenessOperationDescriptor('message.send')!.inputSchema);
    expect(JSON.parse(receipt.results[1].text).self.actorId).toBeTruthy();
  } finally {
    await broker.dispose();
    fs.rmSync(root, { recursive: true, force: true });
  }
}, 35_000);

/** Installed Pi executes the packaged extension; the provider is deterministic and offline. */
it('enforces the parent grant through a real installed Pi worker turn', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-worker-production-'));
  const skillPath = path.join(root, 'granted-skill', 'SKILL.md');
  fs.mkdirSync(path.dirname(skillPath));
  fs.writeFileSync(skillPath, '---\nname: granted-skill\ndescription: The granted worker fixture.\n---\nGRANTED_SKILL_BODY\n');
  const snapshot: CapabilitySnapshot = {
    schemaVersion: 1, revision: 'production-grant-1', nativeTools: ['MCPTool', 'skill', 'bash'],
    skills: [{ id: 'granted-id', name: 'granted-skill', path: skillPath, description: 'The granted worker fixture.' }],
    mcpTools: [{ server: 'octocode', tool: 'localSearch', description: 'Granted source research.', inputSchema: { type: 'object' } }, { server: 'private', tool: 'readSecret', description: 'Unrelated parent resource.' }],
  };
  const calls: unknown[] = [];
  const broker = await createWorkerMcpBroker({ snapshot, dispatchMcp: async params => { calls.push(params); return { content: [{ type: 'text', text: 'PARENT_MCP_RESULT' }] }; } });
  try {
    const binding = broker.registerWorker('production-child', { nativeTools: ['MCPTool', 'skill'], skills: ['granted-id'], mcpTools: [{ server: 'octocode', tool: 'localSearch' }] });
    const marker = path.join(root, 'forbidden-native-ran');
    const script = [
      { name: 'MCPTool', arguments: { queries: [{ reasoning: 'Exercise granted parent transport.', action: 'call', server: 'octocode', tool: 'localSearch', arguments: {} }] } },
      { name: 'MCPTool', arguments: { queries: [{ reasoning: 'Verify broker rejects ungranted identity.', action: 'call', server: 'private', tool: 'readSecret', arguments: {} }] } },
      { name: 'skill', arguments: { queries: [{ reasoning: 'Load granted instructions.', type: 'load', action: 'load', name: 'granted-skill', reason: 'Verify the concrete granted file.' }] } },
      { name: 'skill', arguments: { queries: [{ reasoning: 'Verify skill identity denial.', type: 'load', action: 'load', name: 'private-skill', reason: 'Verify missing access is rejected.' }] } },
      { name: 'bash', arguments: { command: `touch '${marker}'` } },
    ];
    const { output, receipt } = await runWorker(root, binding, 'index.js', 'MCPTool,skill', script);
    expect(receipt).toMatchObject({ secretInEnvironment: false, catalogPrivate: false, skillVisible: true });
    expect(receipt.activeTools.sort()).toEqual(['MCPTool', 'skill']);
    expect(receipt.results).toHaveLength(5);
    expect(receipt.results[0]).toMatchObject({ isError: false, text: expect.stringContaining('PARENT_MCP_RESULT') });
    expect(receipt.results[1]).toMatchObject({ isError: true, text: expect.stringContaining('not granted') });
    expect(receipt.results[2]).toMatchObject({ isError: false, text: expect.stringContaining('GRANTED_SKILL_BODY') });
    expect(receipt.results[3]).toMatchObject({ isError: true, text: expect.stringContaining('Unknown skill') });
    expect(receipt.results[4].isError).toBe(true);
    expect(calls).toHaveLength(1);
    expect(fs.existsSync(marker)).toBe(false);
    expect(output).not.toContain(binding.token);
  } finally {
    await broker.dispose();
    fs.rmSync(root, { recursive: true, force: true });
  }
}, 35_000);

it('keeps the lean guard private and rejects native calls after live parent revocation', async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-lean-production-'));
  const allowedMarker = path.join(root, 'allowed-native-ran');
  const forbiddenMarker = path.join(root, 'revoked-native-ran');
  const snapshot: CapabilitySnapshot = { schemaVersion: 1, revision: 'lean-1', nativeTools: ['bash'], skills: [], mcpTools: [] };
  const broker = await createWorkerMcpBroker({
    snapshot,
    refreshSnapshot: async () => fs.existsSync(allowedMarker) ? { ...snapshot, revision: 'lean-revoked', nativeTools: [] } : snapshot,
    dispatchMcp: async () => { throw new Error('Lean workers have no MCP transport.'); },
  });
  try {
    const binding = broker.registerWorker('lean-child', { nativeTools: ['bash'], skills: [], mcpTools: [] });
    const { receipt, output } = await runWorker(root, binding, 'worker-guard.js', 'bash', [
      { name: 'bash', arguments: { command: `test -z "$OCTOCODE_WORKER_CAPABILITY_BINDING" && touch '${allowedMarker}' && printf LEAN_NATIVE_OK` } },
      { name: 'bash', arguments: { command: `touch '${forbiddenMarker}'` } },
    ]);
    expect(receipt).toMatchObject({ activeTools: ['bash'], secretInEnvironment: false, catalogPrivate: false, skillVisible: false });
    expect(receipt.results).toHaveLength(2);
    expect(receipt.results[0]).toMatchObject({ isError: false, text: expect.stringContaining('LEAN_NATIVE_OK') });
    expect(receipt.results[1]).toMatchObject({ isError: true, text: expect.stringContaining('not granted') });
    expect(fs.existsSync(allowedMarker)).toBe(true);
    expect(fs.existsSync(forbiddenMarker)).toBe(false);
    expect(output).not.toContain(binding.token);
  } finally {
    await broker.dispose();
    fs.rmSync(root, { recursive: true, force: true });
  }
}, 35_000);

async function runWorker(root: string, binding: WorkerBrokerBinding, entry: string, nativeTools: string, script: Array<{ name: string; arguments: Record<string, unknown> }>, thinking = 'off') {
  const providerPath = path.join(root, 'provider.mjs');
  fs.writeFileSync(providerPath, providerSource(script), { mode: 0o600 });
  const sdkEntry = fileURLToPath(import.meta.resolve('@earendil-works/pi-coding-agent'));
  const output = await new Promise<string>((resolve, reject) => {
    const child = spawn(process.execPath, [path.join(path.dirname(sdkEntry), 'cli.js'), '--mode', 'json', '--print', '--no-extensions', '--extension', path.resolve('dist', entry), '--extension', providerPath, '--no-session', '--no-skills', '--no-prompt-templates', '--no-themes', '--no-context-files', '--tools', nativeTools, '--provider', 'worker-grant-probe', '--model', 'deterministic', '--thinking', thinking, 'Exercise the supplied grant.'], {
      cwd: root,
      env: { ...process.env, OCTOCODE_HOME: path.join(root, 'octocode'), PI_CODING_AGENT_DIR: path.join(root, 'pi'), OCTOCODE_PI_SUBAGENT: '1', OCTOCODE_WORKER_CAPABILITY_BINDING: JSON.stringify(binding) },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    const timer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error(`Pi worker probe timed out: ${stderr.slice(-2000)}`)); }, 25_000);
    child.stdout.on('data', chunk => { stdout += String(chunk); if (stdout.length > 2_000_000) { child.kill('SIGKILL'); reject(new Error('Pi worker probe exceeded its output limit.')); } });
    child.stderr.on('data', chunk => { stderr += String(chunk); });
    child.once('error', error => { clearTimeout(timer); reject(error); });
    child.once('close', code => { clearTimeout(timer); code === 0 ? resolve(stdout) : reject(new Error(`Pi worker probe exited ${code}: ${stderr.slice(-4000)}`)); });
  });
  const events = output.split('\n').filter(Boolean).flatMap(line => { try { return [JSON.parse(line)]; } catch { return []; } });
  const persistedGrant = events.find(event => event.type === 'entry_appended' && event.entry?.customType === 'octocode-worker-capabilities')?.entry.data;
  expect(persistedGrant).toMatchObject({ schemaVersion: 1, grant: { workerId: binding.workerId, revision: 1 }, capabilityRevision: expect.any(String) });
  const finalText = events.flatMap(event => event.type === 'message_end' && event.message?.role === 'assistant' ? event.message.content : []).find(block => block.type === 'text' && block.text.startsWith('WORKER_GRANT_RECEIPT:'))?.text;
  expect(finalText, output.slice(-4000)).toBeDefined();
  return { output, receipt: JSON.parse(finalText.slice('WORKER_GRANT_RECEIPT:'.length)) };
}

function providerSource(script: Array<{ name: string; arguments: Record<string, unknown> }>): string {
  return `
const script = ${JSON.stringify(script)};
let index = 0;
let initial;
export default function provider(pi) {
  pi.registerProvider('worker-grant-probe', {
    name: 'Offline worker grant probe', api: 'worker-grant-probe-api', baseUrl: 'http://127.0.0.1:0', apiKey: 'local-fixture',
    models: [{id:'deterministic',name:'Deterministic',reasoning:true,input:['text'],cost:{input:0,output:0,cacheRead:0,cacheWrite:0},contextWindow:131072,maxTokens:2048}],
    streamSimple: (_model, context, options) => {
      initial ??= {reasoning:options?.reasoning,activeTools:(context.tools ?? []).map(tool => tool.name),catalogPrivate:context.systemPrompt.includes('readSecret'),skillVisible:context.systemPrompt.includes('granted-skill'),secretInEnvironment:process.env.OCTOCODE_WORKER_CAPABILITY_BINDING !== undefined,awarenessPolicyCopies:context.systemPrompt.split('<awareness>').length - 1};
      const call = script[index++];
      const content = call ? [{type:'toolCall',id:'grant-call-' + index,...call}] : [{type:'text',text:'WORKER_GRANT_RECEIPT:' + JSON.stringify({...initial,results:context.messages.filter(message => message.role === 'toolResult').map(message => ({isError:message.isError === true,text:message.content.filter(block => block.type === 'text').map(block => block.text).join('\\n')}))})}];
      const final = {role:'assistant',content,api:'worker-grant-probe-api',provider:'worker-grant-probe',model:'deterministic',usage:{input:1,output:1,cacheRead:0,cacheWrite:0,totalTokens:2,cost:{input:0,output:0,cacheRead:0,cacheWrite:0,total:0}},stopReason:call?'toolUse':'stop',timestamp:Date.now()};
      const partial = {...final,stopReason:'pending'};
      const events = [{type:'start',partial},...(call ? [{type:'toolcall_start',contentIndex:0,partial},{type:'toolcall_delta',contentIndex:0,delta:JSON.stringify(call.arguments),partial},{type:'toolcall_end',contentIndex:0,toolCall:content[0],partial}] : [{type:'text_start',contentIndex:0,partial},{type:'text_delta',contentIndex:0,delta:content[0].text,partial},{type:'text_end',contentIndex:0,content:content[0].text,partial}]),{type:'done',reason:final.stopReason,message:final}];
      return {final:Promise.resolve(final),result:()=>Promise.resolve(final),async *[Symbol.asyncIterator](){for(const event of events) yield event;}};
    },
  });
}
`;
}
