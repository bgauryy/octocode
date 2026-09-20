import { spawn } from 'node:child_process';
import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const ALLOWED = ['ghSearch', 'ghGetFileContent', 'ghSearchHistory', 'ghGetHistoryItem', 'artifactSearch', 'jev'];
const DISABLED = ['shell_tool', 'apps', 'plugins', 'browser_use', 'browser_use_external', 'computer_use', 'multi_agent', 'hooks', 'image_generation', 'view_image', 'workspace_dependencies', 'skill_search', 'tool_suggest', 'sleep_tool', 'goals', 'memories'];
const toml = value => Array.isArray(value) ? `[${value.map(toml).join(',')}]`
  : value && typeof value === 'object' ? `{${Object.entries(value).map(([key, val]) => `${JSON.stringify(key)}=${toml(val)}`).join(',')}}` : JSON.stringify(value);
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const resourceListKey = tool => ({ list_mcp_resources: 'resources', list_mcp_resource_templates: 'resourceTemplates' })[tool];
function emptyResourceDiscovery(item) {
  const key = resourceListKey(item.tool);
  const result = item.result;
  if (!key || item.error || result?.isError || !Array.isArray(result?.content) || result.content.length !== 1) return false;
  try {
    const value = JSON.parse(result.content[0].text);
    return Object.keys(value).length === 1 && Array.isArray(value[key]) && value[key].length === 0;
  } catch { return false; }
}

// A standard one-call approval, correlated with a live host-generated MCP item.
// Unknown server elicitations, authentication forms and persistent grants are never accepted.
export function approvalFor(params, pending, threadId) {
  if (params.threadId !== threadId || params.serverName !== 'octocode' || params.mode !== 'form' ||
      params._meta?.codex_approval_kind !== 'mcp_tool_call' ||
      params.requestedSchema?.type !== 'object' || Object.keys(params.requestedSchema?.properties ?? {}).length !== 0) return null;
  const matches = [...pending.values()].filter(item => item.server === 'octocode' && ALLOWED.includes(item.tool) &&
    params.message === `Allow the octocode MCP server to run tool "${item.tool}"?` &&
    same(params._meta?.tool_params, item.arguments));
  return matches.length === 1 ? matches[0] : null;
}

export async function runAppServer({ cwd, env, model, effort = 'medium', prompt, outputSchema, runDir, proxyPath,
  deadlineMs = 300000, codexPath = 'codex', modelProvider, providerConfig }) {
  mkdirSync(runDir, { recursive: true });
  const started = Date.now();
  const secrets = Object.entries(env).filter(([key, value]) => /(?:TOKEN|SECRET|PASSWORD|(?:^|_)KEY)$/.test(key) && value).map(([, value]) => value);
  const redact = text => secrets.reduce((out, secret) => out.split(secret).join('[REDACTED]'), text);
  const log = (name, value) => appendFileSync(join(runDir, name), redact(JSON.stringify(value)) + '\n', { mode: 0o600 });
  const config = { mcp_servers: { octocode: { command: process.execPath, args: [proxyPath], required: true,
    env_vars: ['JEV_BENCH_CONFIG', 'OCTOCODE_HOME', 'OCTOCODE_NATIVE_BINDING', 'OCTOCODE_REGEX_WORKER', 'OCTOCODE_JEV_KEY', 'OCTOCODE_JEV_MODEL', 'OCTOCODE_JEV_BASE_URL', 'GITHUB_TOKEN', 'GH_TOKEN', 'ENABLE_LOCAL', 'ENABLE_CLONE', 'MAX_RETRIES', 'OCTOCODE_ENABLE_STATS', 'OCTOCODE_STORAGE_MODE'],
    enabled_tools: ALLOWED, default_tools_approval_mode: 'prompt', startup_timeout_sec: 30, tool_timeout_sec: 120 } },
    project_doc_max_bytes: 0, developer_instructions: '', web_search: 'disabled', tool_output_token_limit: 12000,
    features: { ...Object.fromEntries(DISABLED.map(key => [key, false])), skip_host_skill_discovery: true, tool_call_mcp_elicitation: true },
    model_provider: modelProvider ?? 'openai', model_reasoning_effort: effort, approval_policy: 'on-request', approvals_reviewer: 'user', sandbox_mode: 'read-only' };
  if (modelProvider) {
    config.model_provider = modelProvider;
    config.model_providers = { [modelProvider]: providerConfig };
  }
  const args = ['app-server', '--stdio'];
  for (const [key, value] of Object.entries(config)) args.push('-c', `${key}=${toml(value)}`);
  const child = spawn(codexPath, args, { cwd, env, detached: true, stdio: ['pipe', 'pipe', 'pipe'] });
  const pendingRequests = new Map();
  const pendingTools = new Map();
  let sequence = 0, buffer = '', threadId, actualModel, actualModelProvider, tokenUsage, finalTurn, timedOut = false, approvals = 0, declined = 0;
  const itemTypes = new Set();
  let prohibitedToolEvents = 0;
  let finish;
  const finished = new Promise(resolve => { finish = resolve; });
  const kill = () => { try { process.kill(-child.pid, 'SIGTERM'); } catch {} };
  const timer = setTimeout(() => { timedOut = true; finish();
    for (const pending of pendingRequests.values()) pending.reject(new Error('Benchmark deadline reached'));
    kill(); }, deadlineMs);
  const send = message => child.stdin.write(JSON.stringify(message) + '\n');
  const request = (method, params) => new Promise((resolve, reject) => {
    const id = ++sequence; pendingRequests.set(id, { resolve, reject, method }); send({ jsonrpc: '2.0', id, method, params });
  });
  const processMessage = message => {
    const isConfigRead = pendingRequests.get(message.id)?.method === 'config/read';
    log('events.jsonl', isConfigRead ? { id: message.id, configRead: true,
      serverNames: Object.keys(message.result?.config?.mcp_servers ?? {}), error: message.error } : message);
    if (message.id !== undefined && !message.method) {
      const pending = pendingRequests.get(message.id);
      pendingRequests.delete(message.id);
      if (message.error) pending?.reject(new Error(`App-server RPC failed: ${message.error.code}`));
      else pending?.resolve(message.result);
      return;
    }
    const params = message.params ?? {};
    if (message.id !== undefined && message.method) {
      if (message.method === 'mcpServer/elicitation/request') {
        const approved = approvalFor(params, pendingTools, threadId);
        if (approved) approvals++; else declined++;
        log('approvals.jsonl', { requestId: message.id, approved: !!approved, server: params.serverName, tool: approved?.tool ?? null });
        send({ jsonrpc: '2.0', id: message.id, result: approved ? { action: 'accept', content: {} } : { action: 'decline' } });
      } else {
        declined++;
        send({ jsonrpc: '2.0', id: message.id, error: { code: -32601, message: 'Request is outside benchmark authorization' } });
      }
      return;
    }
    if (message.method === 'thread/tokenUsage/updated' && params.threadId === threadId) tokenUsage = params.tokenUsage.total;
    if (message.method === 'item/started' || message.method === 'item/completed') {
      const item = params.item;
      if (!item) return;
      itemTypes.add(item.type);
      const resourceDiscovery = item.type === 'mcpToolCall' && item.server === 'codex' && resourceListKey(item.tool);
      if (['commandExecution', 'fileChange', 'webSearch', 'collabAgentToolCall'].includes(item.type) ||
          item.type === 'mcpToolCall' && !resourceDiscovery && (item.server !== 'octocode' || !ALLOWED.includes(item.tool)) ||
          resourceDiscovery && message.method === 'item/completed' && !emptyResourceDiscovery(item)) prohibitedToolEvents++;
      if (item.type === 'mcpToolCall') {
        if (message.method === 'item/started') pendingTools.set(item.id, item); else pendingTools.delete(item.id);
      }
      if (item.type === 'agentMessage' && message.method === 'item/completed' && item.phase === 'final_answer') writeFileSync(join(runDir, 'answer.json'), item.text);
    }
    if (message.method === 'turn/completed' && params.threadId === threadId) { finalTurn = params.turn; finish(); }
  };
  child.stdout.on('data', chunk => {
    buffer += chunk;
    for (;;) {
      const at = buffer.indexOf('\n'); if (at < 0) break;
      const line = buffer.slice(0, at); buffer = buffer.slice(at + 1);
      try { processMessage(JSON.parse(line)); } catch { finish(); kill(); }
    }
  });
  child.stderr.on('data', chunk => appendFileSync(join(runDir, 'stderr.log'), redact(chunk.toString()), { mode: 0o600 }));
  let processError;
  child.on('error', error => { processError = error.code; finish(); for (const pending of pendingRequests.values()) pending.reject(error); });
  child.on('close', () => { finish(); for (const pending of pendingRequests.values()) pending.reject(new Error('App-server closed')); });
  try {
    await request('initialize', { clientInfo: { name: 'jev-tool-terra-benchmark', version: '1' }, capabilities: { experimentalApi: true } });
    send({ jsonrpc: '2.0', method: 'initialized' });
    const effective = await request('config/read', { includeLayers: false });
    const otherServers = Object.fromEntries(Object.entries(effective.config.mcp_servers ?? {})
      .filter(([name]) => name !== 'octocode').map(([name]) => [name, { enabled: false }]));
    config.mcp_servers = { ...otherServers, octocode: config.mcp_servers.octocode };
    const startedThread = await request('thread/start', { cwd, ephemeral: true, model,
      modelProvider: modelProvider ?? 'openai', approvalPolicy: 'on-request', approvalsReviewer: 'user', sandbox: 'read-only',
      config, developerInstructions: '', allowProviderModelFallback: false });
    threadId = startedThread.thread.id; actualModel = startedThread.model; actualModelProvider = startedThread.modelProvider;
    log('thread-settings.jsonl', { actualModel, modelProvider: startedThread.modelProvider, approvalPolicy: startedThread.approvalPolicy,
      approvalsReviewer: startedThread.approvalsReviewer, sandbox: startedThread.sandbox, instructionSources: startedThread.instructionSources });
    if (actualModel !== model || actualModelProvider !== (modelProvider ?? 'openai') || startedThread.approvalPolicy !== 'on-request' || startedThread.approvalsReviewer !== 'user' ||
        startedThread.sandbox?.type !== 'readOnly' || startedThread.instructionSources?.length) throw new Error('Unexpected effective thread configuration');
    const catalog = await request('mcpServerStatus/list', { threadId });
    const octocode = catalog.data?.find(server => server.name === 'octocode');
    const names = Object.keys(octocode?.tools ?? {});
    if (catalog.nextCursor || octocode?.runtimeStatus !== 'connected' || octocode.toolsError ||
        names.length !== ALLOWED.length || ALLOWED.some(name => !names.includes(name)) ||
        catalog.data.some(server => server.name !== 'octocode' && server.runtimeStatus === 'connected')) throw new Error('Unexpected effective MCP catalog');
    await request('turn/start', { threadId, input: [{ type: 'text', text: prompt }], model, effort, outputSchema });
    await finished;
    if (!timedOut) await new Promise(resolve => setTimeout(resolve, 100));
  } catch (error) { log('runner-error.jsonl', { error: error.message }); processError ??= 'adapterError'; }
  finally {
    clearTimeout(timer); kill();
    const escalation = setTimeout(() => { try { process.kill(-child.pid, 'SIGKILL'); } catch {} }, 1500);
    escalation.unref();
  }
  const usage = tokenUsage ? [{ input_tokens: tokenUsage.inputTokens, cached_input_tokens: tokenUsage.cachedInputTokens, output_tokens: tokenUsage.outputTokens }] : [];
  return { exitCode: !processError && finalTurn?.status === 'completed' && !timedOut ? 0 : 1, signal: timedOut ? 'SIGTERM' : null,
    timedOut, elapsedMs: Date.now() - started, usage, itemTypes: [...itemTypes], prohibitedToolEvents, actualModel, actualModelProvider,
    approvals, declinedApprovals: declined, turnStatus: finalTurn?.status ?? null,
    modelAttestation: 'App-server thread/start reported actual model; no independent provider identity receipt claimed.' };
}
