// Maintainer feasibility probe, deliberately not a production delivery transport.
import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { isAbsolute } from 'node:path';
import { pathToFileURL } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const historyUpdates = new Set(['user_message_chunk', 'agent_message_chunk', 'agent_thought_chunk']);

/** Owns one stdio ACP process. No session creation, history load, retries or tool grants. */
export class AcpProbeClient {
  constructor(command, { cwd = process.cwd(), env = process.env, timeoutMs = 10000, maxFrameBytes = 1024 * 1024, maxTotalBytes = 8 * 1024 * 1024 } = {}) {
    if (!Array.isArray(command) || !command.length || command.some(x => typeof x !== 'string')) throw Error('command must be a nonempty string array');
    for (const value of [timeoutMs, maxFrameBytes, maxTotalBytes]) if (!Number.isSafeInteger(value) || value <= 0) throw Error('ACP limits must be positive integers');
    this.timeoutMs = timeoutMs;
    this.maxFrameBytes = maxFrameBytes;
    this.maxTotalBytes = maxTotalBytes;
    this.pending = new Map();
    this.sequence = 0;
    this.buffer = Buffer.alloc(0);
    this.stats = { requests: {}, notifications: Object.create(null), permissionDenials: 0, stdoutBytes: 0, stderrBytes: 0, sentBytes: 0, historyReplayChunks: 0, processGroupSignalFallback: false, forcedStdioCleanup: false };
    this.child = spawn(command[0], command.slice(1), { cwd, env, detached: process.platform !== 'win32', stdio: ['pipe', 'pipe', 'pipe'] });
    this.closed = new Promise(resolve => this.child.once('close', () => { this.processClosed = true; resolve(); }));
    this.child.once('error', () => this.fail(Error('ACP process could not start')));
    this.child.once('close', () => this.fail(Error('ACP process exited')));
    this.child.stdin.on('error', () => this.fail(Error('ACP stdin closed')));
    this.child.stdout.on('end', () => this.fail(Error(this.buffer.length ? 'ACP truncated frame at EOF' : 'ACP EOF')));
    this.child.stderr.on('data', chunk => {
      this.stats.stderrBytes += chunk.length;
      if (this.stats.stderrBytes > maxTotalBytes) this.fail(Error('ACP stderr byte budget exceeded'));
    });
    this.child.stdout.on('data', chunk => {
      try { this.consume(chunk); } catch (error) { this.fail(error); }
    });
  }

  signal(signal) {
    if (!this.child.pid) return;
    try {
      if (process.platform === 'win32') this.child.kill(signal);
      else process.kill(-this.child.pid, signal);
    } catch (error) {
      // Some managed runtimes forbid process-group signals but allow owned children.
      if (error.code === 'EPERM') {
        this.stats.processGroupSignalFallback = true;
        this.child.kill(signal);
      }
      else if (error.code !== 'ESRCH') throw error;
    }
  }

  fail(error) {
    if (this.failure) return;
    this.failure = error;
    for (const { reject, timer } of this.pending.values()) { clearTimeout(timer); reject(error); }
    this.pending.clear();
    this.signal('SIGTERM');
    this.killTimer = setTimeout(() => this.signal('SIGKILL'), 250);
    this.killTimer.unref();
  }

  consume(chunk) {
    if (this.failure) return;
    this.stats.stdoutBytes += chunk.length;
    if (this.stats.stdoutBytes > this.maxTotalBytes) throw Error('ACP stdout byte budget exceeded');
    this.buffer = Buffer.concat([this.buffer, chunk]);
    let end;
    while ((end = this.buffer.indexOf(10)) >= 0) {
      if (end > this.maxFrameBytes) throw Error('ACP frame too large');
      const frame = this.buffer.subarray(0, end);
      this.buffer = this.buffer.subarray(end + 1);
      let value;
      try { value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(frame)); }
      catch { throw Error('ACP malformed JSON or UTF-8 frame'); }
      this.receive(value);
    }
    if (this.buffer.length > this.maxFrameBytes) throw Error('ACP frame too large');
  }

  receive(value) {
    if (!object(value) || value.jsonrpc !== '2.0') throw Error('ACP invalid JSON-RPC envelope');
    if (typeof value.method === 'string') {
      if ('result' in value || 'error' in value) throw Error('ACP mixed request/response envelope');
      if ('id' in value) {
        if (typeof value.id !== 'string' && !Number.isSafeInteger(value.id)) throw Error('ACP invalid request ID');
        if (value.method === 'session/request_permission') {
          this.stats.permissionDenials++;
          const reject = value.params?.options?.find(option => option.kind === 'reject_once' && typeof option.optionId === 'string');
          this.write({ jsonrpc: '2.0', id: value.id, result: { outcome: reject ? { outcome: 'selected', optionId: reject.optionId } : { outcome: 'cancelled' } } });
        } else this.write({ jsonrpc: '2.0', id: value.id, error: { code: -32601, message: 'Client capability unavailable' } });
      } else {
        const kind = value.method === 'session/update' ? value.params?.update?.sessionUpdate : value.method;
        if (typeof kind !== 'string' || kind.length > 128) throw Error('ACP invalid notification');
        if (this.activeSession && value.method === 'session/update' && value.params?.sessionId !== this.activeSession) throw Error('ACP update belongs to another session');
        // Keep counts, never the conversation, thoughts, tool arguments or credentials.
        const key = Object.hasOwn(this.stats.notifications, kind) || Object.keys(this.stats.notifications).length < 64 ? kind : 'other';
        this.stats.notifications[key] = (this.stats.notifications[key] || 0) + 1;
        if (this.resuming && historyUpdates.has(kind)) {
          this.stats.historyReplayChunks++;
          throw Error('ACP resume replayed conversation history');
        }
      }
      return;
    }
    if (!('id' in value) || ('result' in value) === ('error' in value)) throw Error('ACP invalid response envelope');
    const pending = this.pending.get(value.id);
    if (!pending) throw Error('ACP unsolicited or duplicate response');
    clearTimeout(pending.timer);
    this.pending.delete(value.id);
    if ('error' in value) pending.reject(Error(`ACP ${pending.method} rejected (${value.error?.code ?? 'unknown'})`));
    else pending.resolve(value.result);
  }

  write(value) {
    if (this.failure) throw this.failure;
    const frame = `${JSON.stringify(value)}\n`;
    const bytes = Buffer.byteLength(frame);
    if (bytes > this.maxFrameBytes) throw Error('ACP outgoing frame too large');
    if (this.stats.sentBytes + bytes > this.maxTotalBytes) throw Error('ACP outgoing byte budget exceeded');
    this.stats.sentBytes += bytes;
    this.child.stdin.write(frame);
  }

  request(method, params) {
    if (this.failure) return Promise.reject(this.failure);
    if (this.pending.size >= 8) return Promise.reject(Error('ACP pending request limit exceeded'));
    const id = ++this.sequence;
    this.stats.requests[method] = (this.stats.requests[method] || 0) + 1;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => this.fail(Error(`ACP ${method} timed out; outcome uncertain; do not retry automatically`)), this.timeoutMs);
      this.pending.set(id, { resolve, reject, timer, method });
      try { this.write({ jsonrpc: '2.0', id, method, params }); }
      catch (error) { this.fail(error); }
    });
  }

  async initialize() {
    if (this.initialized) throw Error('ACP already initialized');
    const result = await this.request('initialize', { protocolVersion: 1, clientCapabilities: {}, clientInfo: { name: 'octocode-acp-probe', version: '0.1.0' } });
    if (this.failure) throw this.failure;
    if (!object(result) || result.protocolVersion !== 1 || !object(result.agentCapabilities)) throw Error('ACP incompatible initialization response');
    this.capabilities = result.agentCapabilities;
    this.initialized = true;
    return result;
  }

  async resume({ sessionId, cwd, mcpServers, configurationReviewed = false }) {
    if (!this.initialized) throw Error('ACP initialize required');
    if (!object(this.capabilities.sessionCapabilities?.resume)) throw Error('ACP resume unsupported; refusing history-loading fallback');
    if (!configurationReviewed || !Array.isArray(mcpServers) || !isAbsolute(cwd || '') || typeof sessionId !== 'string' || !sessionId) throw Error('Resume requires reviewed cwd and full MCP configuration; omission can change recipient tools');
    if (this.activeSession) throw Error('ACP session already bound');
    this.activeSession = sessionId;
    this.resuming = true;
    try {
      const result = await this.request('session/resume', { sessionId, cwd, mcpServers });
      if (!object(result)) throw Error('ACP malformed resume result');
      this.resumed = true;
      return result;
    } finally { this.resuming = false; }
  }

  async prompt(text, { allowModelTurn = false, passive = false } = {}) {
    if (passive) throw Error('ACP passive delivery unsupported; no prompt sent');
    if (!this.resumed || !allowModelTurn || typeof text !== 'string' || !text.trim()) throw Error('Prompt requires resumed session, text and explicit allowModelTurn');
    if (this.prompting) throw Error('ACP prompt already in flight');
    this.prompting = true;
    try {
      const result = await this.request('session/prompt', { sessionId: this.activeSession, prompt: [{ type: 'text', text }] });
      if (!object(result) || typeof result.stopReason !== 'string') throw Error('ACP malformed prompt result');
      return result;
    } finally { this.prompting = false; }
  }

  cancel() {
    if (!this.prompting) throw Error('ACP no active prompt to cancel');
    this.write({ jsonrpc: '2.0', method: 'session/cancel', params: { sessionId: this.activeSession } });
  }

  async close() {
    this.fail(Error('ACP client closed'));
    await Promise.race([this.closed, delay(350)]);
    this.signal('SIGKILL');
    clearTimeout(this.killTimer);
    await Promise.race([this.closed, delay(250)]);
    if (!this.processClosed) {
      // A detached descendant can escape the group and retain our pipe handles.
      // Closing our streams bounds cleanup; it does not prove that child was reaped.
      this.stats.forcedStdioCleanup = true;
      this.child.stdin.destroy();
      this.child.stdout.destroy();
      this.child.stderr.destroy();
      await Promise.race([this.closed, delay(100)]);
    }
  }
}

export async function runProbe(config) {
  const started = performance.now();
  const client = new AcpProbeClient(config.command, config);
  const report = { schemaVersion: 1, date: new Date().toISOString(), passed: false, senderModelCalls: 0, recipientPromptRequests: 0, productionTransport: false };
  try {
    const initialized = await client.initialize();
    report.protocolVersion = initialized.protocolVersion;
    report.agentInfo = initialized.agentInfo;
    report.capabilities = initialized.agentCapabilities;
    report.initializeMs = performance.now() - started;
    if (config.sessionId) {
      await client.resume(config);
      report.resumed = true;
      if (config.prompt !== undefined) {
        const result = await client.prompt(config.prompt, config);
        report.stopReason = result.stopReason;
      }
    } else if (config.prompt !== undefined) throw Error('Existing sessionId required; probe never creates agents');
    report.passed = true;
  } catch (error) { report.error = error.message; }
  finally { await client.close(); }
  report.recipientPromptRequests = client.stats.requests['session/prompt'] || 0;
  report.stats = client.stats;
  report.elapsedMs = performance.now() - started;
  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    if (process.argv.length !== 3) throw Error('Usage: node scripts/acp-probe.mjs CONFIG.json (see docs/ACP_EVALUATION.md)');
    const contents = await readFile(process.argv[2]);
    if (contents.length > 1024 * 1024) throw Error('Probe configuration too large');
    const report = await runProbe(JSON.parse(contents));
    console.log(JSON.stringify(report, null, 2));
    if (!report.passed) process.exitCode = 1;
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
