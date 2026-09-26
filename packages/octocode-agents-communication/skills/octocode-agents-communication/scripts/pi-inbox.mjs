import { execFile, execFileSync } from 'node:child_process';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { openSync, readSync, closeSync, fstatSync, realpathSync } from 'node:fs';
import { relative, isAbsolute, sep } from 'node:path';
import { registerBoundTools } from './pi-extension.mjs';
import { checkHostWrite, plainHostPath } from './hooks/lease-check.mjs';

const exec = promisify(execFile);
const hosts = new WeakMap();
const peerType = 'octocode-peer';
const identityType = 'octocode-identity';
const pause = () => new Promise(resolve => setImmediate(resolve));
// Idle polls back off from 1 s to 10 s; presence (60 s) stays renewed by each hook call.
const idleMin = 1000, idleMax = 10000;

export default function (pi) {
  return registerPiInbox(pi, JSON.parse(process.env.OCTOCODE_COMMUNICATION_BINDING || '{}'));
}

// A host may embed this bridge without changing the process environment.
export function registerPiInbox(pi, options = {}) {
  if (hosts.has(pi)) return hosts.get(pi);
  const binary = options.binary || fileURLToPath(new URL('./agents-communication', import.meta.url));
  let lastHeartbeat = 0, diskCache;
  const pending = new Map();
  let binding, context, timer, polling, active = false, starting = false, stopped = true, generation = 0, idle = idleMin, deliveries = 0;
  let lifecycle = Promise.resolve(), lifecycleRevision = 0;
  const cancelled = new Error('Pi communication lifecycle superseded');
  const serial = run => { const next = lifecycle.then(run); lifecycle = next.catch(() => {}); return next; };
  const invalidate = () => { stopped = true; generation += 1; clearTimeout(timer); };
  const invoke = async (args, target = binding) => {
    if (!target) throw new Error('Communication is disabled or no session is bound');
    const { stdout } = await exec(binary, [...args, '--workspace', target.workspace,
      ...(target.database ? ['--database', target.database] : []),
      ...(target.session ? ['--session', target.session] : [])], { timeout: 10000, maxBuffer: 1024 * 1024 });
    return JSON.parse(stdout);
  };
  const call = (command, input = {}) => invoke([command, JSON.stringify(input)]);
  const enabled = () => !options.enabled || options.enabled(context);
  const isBoundContext = ctx => {
    try {
      if (!binding?.session || ctx?.sessionManager?.getSessionId?.() !== binding.vendorSession) return false;
      const child = relative(binding.workspace, realpathSync(ctx.cwd));
      return child !== '..' && !child.startsWith(`..${sep}`) && !isAbsolute(child);
    } catch { return false; }
  };
  const currentBinding = () => !stopped && enabled() && isBoundContext(context) ? binding : null;

  const entries = () => {
    if (!context?.sessionManager?.getEntries) throw new Error('Complete Pi session entries are required for durable communication receipts');
    return context.sessionManager.getEntries();
  };
  const matches = (entry, type, predicate) => entry.type === 'custom_message' && entry.customType === type && predicate(entry.details || {});
  const receiptSet = rows => new Set(rows.filter(e => matches(e, peerType, d => d.session === binding.session && d.database === binding.database))
    .flatMap(e => e.details.receipts || []).map(r => `${r.id}:${r.dispatchToken}`));
  // The ledger is append-only JSONL: parse only bytes added since the last scan.
  // A replaced or shrunken file is rescanned from its header.
  const diskReceipts = () => {
    const file = context.sessionManager.getSessionFile?.();
    if (!file) return null;
    let fd;
    try { fd = openSync(file, 'r'); } catch (error) { if (error.code === 'ENOENT') return null; throw error; }
    try {
      const metadata = fstatSync(fd);
      const identity = `${file}:${metadata.dev}:${metadata.ino}`;
      if (diskCache?.identity !== identity || metadata.size < diskCache.offset) diskCache = { identity, offset: 0, rest: Buffer.alloc(0), receipts: new Set(), header: false };
      const size = metadata.size - diskCache.offset;
      if (!size) return diskCache.header ? diskCache.receipts : null;
      // Fail closed on oversized growth; never turn a partial scan into absence.
      if (size > 64 * 1024 * 1024) throw new Error('Pi ledger grew beyond the 64 MiB receipt scan bound; inspect pending dispatch manually');
      const bytes = Buffer.alloc(size);
      let read = 0;
      while (read < size) {
        const count = readSync(fd, bytes, read, size - read, diskCache.offset + read);
        if (!count) break;
        read += count;
      }
      const chunk = Buffer.concat([diskCache.rest, bytes.subarray(0, read)]);
      const end = chunk.lastIndexOf(0x0a) + 1;
      const rows = chunk.subarray(0, end).toString('utf8').split('\n').filter(Boolean).map(line => JSON.parse(line));
      if (!diskCache.header && rows.length && (rows[0].type !== 'session' || rows[0].id !== binding.vendorSession)) throw new Error('Pi ledger header does not match the bound session');
      for (const receipt of receiptSet(rows)) diskCache.receipts.add(receipt);
      diskCache.header ||= rows.length > 0;
      diskCache.offset += read;
      diskCache.rest = Buffer.from(chunk.subarray(end));
      // No complete header yet: receipts are unknown, not absent.
      return diskCache.header ? diskCache.receipts : null;
    } finally { closeSync(fd); }
  };
  const persist = async (message, found, expectedGeneration, triggerTurn = false) => {
    if (expectedGeneration !== generation || !currentBinding()) return false;
    if (!found()) {
      pi.sendMessage(message, { triggerTurn, deliverAs: 'steer' });
      await Promise.resolve();
      await pause();
    }
    if (expectedGeneration !== generation || !currentBinding()) return false;
    return found();
  };
  const confirmPending = async () => {
    if (!pending.size) return;
    const seen = diskReceipts();
    if (!seen) return; // Fresh Pi sessions may buffer entries until the first assistant message.
    const items = [...pending.values()].filter(r => seen.has(`${r.id}:${r.dispatchToken}`));
    for (let offset = 0; offset < items.length; offset += confirmationLimit) {
      const batch = items.slice(offset, offset + confirmationLimit);
      await call('confirm_delivery', { items: batch });
      for (const item of batch) pending.delete(`${item.id}:${item.dispatchToken}`);
    }
  };
  const recover = async () => {
    const seen = diskReceipts();
    const memory = receiptSet(entries());
    let after;
    do {
      const page = await invoke(['entity', 'list', 'dispatch', JSON.stringify(after ? { after } : {})]);
      for (const item of page.items) {
        if (item.recipient !== binding.session || item.transport !== `raw:pi:${binding.vendorSession}` || item.state !== 'staged') continue;
        if (seen?.has(`${item.message}:${item.token}`)) {
          await call('confirm_delivery', { items: [{ id: item.message, dispatchToken: item.token }] });
        } else if (memory.has(`${item.message}:${item.token}`)) {
          pending.set(`${item.message}:${item.token}`, { id: item.message, dispatchToken: item.token });
        } else {
          // Complete ledger inspection is safe only after the previous lifecycle owner stopped.
          await call('retry_delivery', { message: item.message, reason: 'Pi lifecycle recovery: complete session ledger has no receipt for this staged attempt' });
        }
      }
      after = page.next;
    } while (after);
  };
  const drain = (allowWake = true) => {
    if (!currentBinding() || active || polling || (allowWake && context.isIdle?.() === false)) return polling;
    const expectedGeneration = generation;
    polling = (async () => {
      // Heartbeat and recovery are deterministic host work, never model calls.
      await confirmPending();
      if (expectedGeneration !== generation || !currentBinding()) return;
      const { items, context: content, action } = await call('hook', { format: 'json', deferConfirm: true, consumer: `pi:${binding.vendorSession}` });
      if (!items.length || expectedGeneration !== generation || !currentBinding()) return;
      deliveries += 1;
      if (typeof content !== 'string' || !content) throw new Error('Communication CLI must provide canonical hook context; rebuild the skill bundle');
      const wanted = items.map(({ id, dispatchToken }) => ({ id, dispatchToken }));
      const found = () => {
        const seen = receiptSet(entries());
        return wanted.every(r => seen.has(`${r.id}:${r.dispatchToken}`));
      };
      await persist({ customType: peerType,
        content,
        display: true, details: { session: binding.session, database: binding.database, receipts: wanted } }, found, expectedGeneration,
        allowWake && !starting && !active && context.isIdle?.() !== false && action === true);
      for (const item of wanted) pending.set(`${item.id}:${item.dispatchToken}`, item);
      await confirmPending();
    })().catch(error => { console.error(`Communication inbox: ${error.message}`); })
      .finally(() => { polling = undefined; });
    return polling;
  };
  if (options.tools !== undefined && typeof options.tools !== 'string') throw new Error('Communication tools must be comma-separated catalog names');
  const schema = args => JSON.parse(execFileSync(binary, ['schema', ...args], { encoding: 'utf8', timeout: 10000, maxBuffer: 1024 * 1024 }));
  const tools = schema(['tools', ...(options.tools === undefined ? [] : ['--tools', options.tools])]);
  const confirmationLimit = schema(['confirm_delivery']).inputSchema?.properties?.items?.maxItems;
  if (!Number.isSafeInteger(confirmationLimit) || confirmationLimit < 1) throw new Error('Invalid confirmation batch limit in communication catalog');
  registerBoundTools(pi, { tools, getBinding: currentBinding });
  if (options.requireLeases !== undefined && typeof options.requireLeases !== 'boolean') throw new Error('requireLeases must be boolean');
  if (options.requireLeases) pi.on('tool_call', async (event, ctx) => {
    // Covers Pi's structured edit/write tools only. Shell/custom tools and later
    // extension rewrites remain outside this admission check, not silently fenced.
    if (!['write', 'edit'].includes(event.toolName)) return;
    const target = currentBinding();
    if (!target || !isBoundContext(ctx)) return {block: true, reason: 'File edit requires an active communication binding and owned lease.'};
    const path = event.input?.path;
    if (!plainHostPath(path) || !plainHostPath(ctx.cwd)) return {block: true, reason: 'File edit requires a plain path without host aliases or parent traversal.'};
    try {
      // The same Rust admission call as the Claude and OpenCode guards.
      const covered = await checkHostWrite(target, {vendorSession: ctx.sessionManager.getSessionId(), cwd: ctx.cwd, path});
      if (currentBinding() !== target || !isBoundContext(ctx) || event.input?.path !== path) return {block: true, reason: 'Session or file path changed during lease validation; retry in the current session.'};
      if (!covered) return {block: true, reason: 'File edit blocked: acquire or renew your own covering lease before retrying.'};
    } catch {
      return {block: true, reason: 'File edit blocked because live lease ownership could not be verified.'};
    }
  });
  if (options.disableCacheWarming) pi.on('cache_warming_decision', () => ({ action: 'stop' }));
  const stop = async () => {
    invalidate();
    await polling;
    pending.clear();
    diskCache = undefined;
    try { if (binding?.session) await call('leave'); }
    finally { binding = undefined; options.onBinding?.(null); }
  };
  const shutdown = () => { lifecycleRevision += 1; invalidate(); return serial(stop); };
  pi.on('session_start', (_event, ctx) => {
    const revision = ++lifecycleRevision;
    invalidate();
    return serial(async () => {
      await stop();
      if (revision !== lifecycleRevision) return;
      const assertCurrent = () => { if (revision !== lifecycleRevision) throw cancelled; };
      context = ctx;
      active = false;
      starting = false;
      idle = idleMin;
      if (options.enabled && !options.enabled(ctx)) return;
      try {
        const vendorSession = ctx.sessionManager.getSessionId();
        if (!vendorSession) throw new Error('Pi session identity is unavailable');
        binding = { binary, workspace: options.workspace || ctx.cwd, database: options.database, vendorSession };
        const info = await invoke(['db', 'info']);
        assertCurrent();
        binding.database = info.path;
        binding.workspace = info.workspace;
        // Inspect the complete ledger, including compacted branches, before reusing an identity.
        const prior = entries().findLast(e => matches(e, identityType, d => d.vendorSession === vendorSession
          && d.workspace === binding.workspace && d.database === binding.database));
        binding.session = options.session || prior?.details.session;
        if (binding.session) {
          const identity = await invoke(['entity', 'get', 'session', binding.session]);
          assertCurrent();
          if (!identity || identity.vendor !== 'pi' || (identity.vendorSession && identity.vendorSession !== vendorSession)) throw new Error('Pi binding requires this vendor session identity');
          if (!identity.active) await call('resume', { vendor: 'pi' });
        } else {
          binding.session = (await call('join', { name: `pi-${vendorSession.slice(0, 8)}`, vendor: 'pi', vendorSession })).id;
        }
        assertCurrent();
        await call('attach', { transport: 'raw', vendorSession });
        assertCurrent();
        stopped = false;
        const identity = { ...binding };
        await persist({ customType: identityType,
          content: `Communication session: ${binding.session}. Use bound tools for DB-audited coordination. The host maintains presence and delivers peer context; action messages wake an idle agent and passive messages wait. Skip manual setup and inbox polling.`,
          display: false, details: identity }, () => entries().some(e => matches(e, identityType, d => d.session === identity.session && d.database === identity.database)), generation);
        assertCurrent();
        await recover();
        assertCurrent();
        options.onBinding?.({ ...binding });
        const timerGeneration = generation;
        const tick = async () => {
          if (timerGeneration !== generation) return;
          if (!currentBinding()) { void shutdown().catch(error => console.error(`Communication cleanup: ${error.message}`)); return; }
          if (active) {
            idle = idleMin;
            if (Date.now() - lastHeartbeat >= 15000) { lastHeartbeat = Date.now(); void call('heartbeat').catch(error => console.error(`Communication presence: ${error.message}`)); }
          } else {
            // Each empty poll doubles the wait; a delivery or turn boundary resets it.
            const before = deliveries;
            await drain();
            idle = deliveries === before ? Math.min(idle * 2, idleMax) : idleMin;
          }
          if (timerGeneration !== generation) return;
          timer = setTimeout(tick, idle);
          timer.unref();
        };
        timer = setTimeout(tick, idle);
        timer.unref();
        await drain();
        assertCurrent();
      } catch (error) {
        await stop().catch(cleanup => console.error(`Communication cleanup: ${cleanup.message}`));
        if (error !== cancelled) throw error;
      }
    });
  });
  pi.on('before_agent_start', async (_event, ctx) => {
    if (!enabled() || !isBoundContext(ctx)) { await shutdown(); return; }
    context = ctx;
    starting = true;
    try { await drain(false); } finally { starting = false; }
  });
  pi.on('agent_start', (_event, ctx) => { if (currentBinding() && isBoundContext(ctx)) active = true; });
  pi.on('agent_end', (_event, ctx) => {
    if (!currentBinding() || !isBoundContext(ctx)) return;
    context = ctx;
    active = false;
    idle = idleMin;
    // Pi remains streaming until all agent_end handlers return.
    setImmediate(() => { void drain(); });
  });
  pi.on('message_end', async (event, ctx) => {
    const message = event.message;
    if (!currentBinding() || !isBoundContext(ctx) || message?.role !== 'assistant' || !message.usage) return;
    const u = message.usage;
    await call('record_usage', { key: `pi-${message.timestamp ?? randomUUID()}`, scope: 'request',
      ...(Number.isInteger(u.input) ? { inputTokens: u.input } : {}),
      ...(Number.isInteger(u.output) ? { outputTokens: u.output } : {}),
      ...(Number.isInteger(u.cacheRead) ? { cachedInputTokens: u.cacheRead } : {}),
      ...(Number.isInteger(u.cacheWrite) ? { cacheWriteTokens: u.cacheWrite } : {}),
      ...([u.input, u.cacheRead, u.cacheWrite].every(Number.isInteger)
        ? { contextTokens: u.input + u.cacheRead + u.cacheWrite } : {}) });
  });
  pi.on('session_shutdown', shutdown);
  const controller = {
    call: (command, input) => {
      if (!currentBinding()) return Promise.reject(new Error('Communication is disabled or no current session is bound'));
      return call(command, input);
    },
    getBinding: () => currentBinding() ? { ...binding } : null, isBoundContext, drain,
  };
  hosts.set(pi, controller);
  return controller;
}
