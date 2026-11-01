// One `octocode mcp` process per case, under the OS resource reporter, so each
// case gets its own exact peak RSS. Language servers are child processes: their
// memory is sampled as the process tree, never folded into the server peak.
import { execFile, spawn } from 'node:child_process';

// The flag that makes /usr/bin/time print the peak resident set.
const TIME_FLAG = { darwin: '-l', linux: '-v' }[process.platform];

/** Peak RSS (KiB) and, on macOS, peak memory footprint (KiB) from `time` stderr. */
export function parseResourceReport(stderr) {
  if (process.platform === 'darwin') {
    const bytes = label => Number(stderr.match(new RegExp(`^\\s*(\\d+)\\s+${label}$`, 'm'))?.[1] ?? NaN);
    return { peakRssKb: Math.round(bytes('maximum resident set size') / 1024) || null, peakFootprintKb: Math.round(bytes('peak memory footprint') / 1024) || null };
  }
  return { peakRssKb: Number(stderr.match(/Maximum resident set size \(kbytes\): (\d+)/)?.[1]) || null, peakFootprintKb: null };
}

const psRows = () => new Promise(resolve => {
  execFile('ps', ['-A', '-o', 'pid=,ppid=,rss='], { encoding: 'utf8' }, (error, out) => {
    resolve(error ? [] : out.trim().split('\n').map(l => l.trim().split(/\s+/).map(Number)));
  });
});

/** RSS (KiB) of `pid` and of its whole descendant tree. */
async function sampleTree(pid) {
  const rows = await psRows();
  const kids = new Map();
  for (const [p, pp] of rows) kids.set(pp, [...(kids.get(pp) ?? []), p]);
  const rss = new Map(rows.map(([p, , r]) => [p, r]));
  let tree = 0;
  const stack = [pid];
  while (stack.length) { const p = stack.pop(); tree += rss.get(p) ?? 0; stack.push(...(kids.get(p) ?? [])); }
  return { self: rss.get(pid) ?? 0, tree, kids };
}

/** Start a server; `call` returns { ms, bytes, result }; `close` returns the resource report. */
export async function startMeasuredServer(binary, { cwd, env, timeoutMs = 300_000, sampleMs = 100 }) {
  const [cmd, argv] = TIME_FLAG ? ['/usr/bin/time', [TIME_FLAG, binary, 'mcp']] : [binary, ['mcp']];
  const child = spawn(cmd, argv, { cwd, env, stdio: ['pipe', 'pipe', 'pipe'] });
  let buffer = '';
  let stderr = '';
  let id = 0;
  const pending = new Map();
  child.stdout.setEncoding('utf8').on('data', chunk => {
    buffer += chunk;
    let index;
    while ((index = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, index);
      buffer = buffer.slice(index + 1);
      if (!line.trim()) continue;
      const message = JSON.parse(line);
      pending.get(message.id)?.(message, Buffer.byteLength(line));
    }
  });
  child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
  const exited = new Promise(resolve => child.on('exit', (code, signal) => resolve({ code, signal })));

  // The server is `time`'s child when `time` wraps it.
  const serverPid = async () => {
    if (!TIME_FLAG) return child.pid;
    for (let i = 0; i < 50; i++) {
      const { kids } = await sampleTree(child.pid);
      const pid = kids.get(child.pid)?.[0];
      if (pid) return pid;
      await new Promise(r => setTimeout(r, 20));
    }
    return child.pid;
  };

  const rpc = (method, params) => new Promise((resolve, reject) => {
    const n = ++id;
    const timer = setTimeout(() => { pending.delete(n); reject(new Error(`${method} timed out after ${timeoutMs} ms`)); }, timeoutMs);
    pending.set(n, (message, bytes) => { clearTimeout(timer); pending.delete(n); resolve({ message, bytes }); });
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id: n, method, params })}\n`);
  });

  const started = performance.now();
  await rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'octocode-workloads', version: '1' } });
  const initMs = Math.round(performance.now() - started);
  child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
  const pid = await serverPid();

  // Sampled peaks between `mark()` calls: the server alone and with descendants.
  let window = { self: 0, tree: 0 };
  const sampler = { running: true };
  void (async () => {
    while (sampler.running) {
      const { self, tree } = await sampleTree(pid);
      window = { self: Math.max(window.self, self), tree: Math.max(window.tree, tree) };
      await new Promise(r => setTimeout(r, sampleMs));
    }
  })();

  return {
    initMs,
    async call(name, args) {
      const t0 = performance.now();
      const { message, bytes } = await rpc('tools/call', { name, arguments: args });
      return { ms: performance.now() - t0, bytes, result: message.result, error: message.error };
    },
    mark() { const peak = window; window = { self: 0, tree: 0 }; return { sampledRssKb: peak.self, sampledTreeKb: peak.tree }; },
    async close() {
      sampler.running = false;
      child.stdin.end();
      const exit = await Promise.race([exited, new Promise(r => setTimeout(() => { child.kill('SIGTERM'); r(null); }, 15_000))]) ?? await exited;
      return { exit, ...parseResourceReport(stderr), stderrTail: exit.code ? stderr.slice(-2000) : undefined };
    },
  };
}
