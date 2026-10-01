// Evaluator-owned capabilities. Solvers never receive upstream credentials.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import net from 'node:net';
import { spawn, execFileSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { propagateOctocodeEnv, getOctocodeHome } from '@octocodeai/config';

export const ISOLATION_VERSION = 2;
export const MODEL_HOSTS = ['api.anthropic.com', 'claude.ai', 'platform.claude.com', 'console.anthropic.com'];
const listen = server => new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', () => resolve(server.address().port)); });
const shutdown = server => !server ? Promise.resolve() : new Promise(resolve => { server.closeAllConnections?.(); server.close(resolve); });
const quoted = value => JSON.stringify(value);

export function sandboxPolicy({ cwd, corpus = [], ports = [] }) {
  if (process.platform !== 'darwin' || !fs.existsSync('/usr/bin/sandbox-exec')) throw new Error('Verified macOS sandbox-exec required; unsupported solver isolation fails closed.');
  const real = fs.realpathSync(cwd);
  const executablePaths = ['claude', 'gh'].map(name => {
    const executable = execFileSync('/usr/bin/which', [name], { encoding: 'utf8' }).trim();
    return fs.realpathSync(executable);
  });
  const readable = [real, ...corpus.map(p => fs.realpathSync(p)), '/System', '/usr', '/bin', '/sbin', '/opt/homebrew', '/usr/local', '/private/etc', '/private/var/db/dyld', '/private/var/db/timezone', '/dev', '/Library/Apple'];
  return `(version 1)
(allow default)
(deny file-read*)
(allow file-read-metadata)
(allow file-read* (literal "/"))
${readable.map(p => `(allow file-read* (subpath ${quoted(p)}))`).join('\n')}
${[...executablePaths, fs.realpathSync(process.execPath)].map(p => `(allow file-read* (literal ${quoted(p)}))`).join('\n')}
(deny process-info*)
(allow process-info* (target self))
(deny appleevent-send)
(deny mach-lookup)
(allow mach-lookup (global-name "com.apple.trustd") (global-name "com.apple.trustd.agent") (global-name "com.apple.cfprefsd.agent") (global-name "com.apple.cfprefsd.daemon"))
(deny network-outbound)
${ports.map(port => `(allow network-outbound (remote ip "localhost:${port}"))`).join('\n')}
(allow network-outbound (remote unix-socket (literal ${quoted(path.join(real, 'github.sock'))})))
(deny file-write*)
(allow file-write* (subpath ${quoted(real)}))
(deny file-read* (regex #"(^|/)octocode-benchmark(/|$)") (regex #"(^|/)\\.octocode(/|$)") (regex #"(^|/)\\.claude(/|$)") (regex #"(^|/)\\.config/gh(/|$)") (regex #"(^|/)(\\.git-credentials|CLAUDE\\.md)$"))
(deny mach-lookup (global-name "com.apple.securityd"))
${corpus.map(p => `(deny file-write* (subpath ${quoted(fs.realpathSync(p))}))`).join('\n')}
`;
}

export function verifySandbox(cwd, corpus = []) {
  const policy = sandboxPolicy({ cwd, corpus });
  const file = path.join(cwd, 'solver.sb');
  fs.writeFileSync(file, policy);
  const privateDir = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-benchmark-isolation-'));
  fs.mkdirSync(path.join(privateDir, 'octocode-benchmark'));
  const secret = path.join(privateDir, 'octocode-benchmark', 'synthetic-secret');
  fs.writeFileSync(secret, 'private');
  try {
    const p = policy;
    const script = `const fs=require('fs');let denied=false;try{fs.readFileSync(${JSON.stringify(secret)})}catch(e){denied=true}if(!denied)process.exit(10);try{fs.writeFileSync(${JSON.stringify(path.join(privateDir, 'write'))},'bad');process.exit(11)}catch{};const net=require('net');const s=net.connect({host:'1.1.1.1',port:443});s.on('connect',()=>process.exit(12));s.on('error',()=>process.exit(0));setTimeout(()=>process.exit(13),3000);`;
    execFileSync('/usr/bin/sandbox-exec', ['-p', p, process.execPath, '-e', script], { cwd, timeout: 5000, stdio: 'pipe' });
    return file;
  } finally { fs.rmSync(privateDir, { recursive: true, force: true }); }
}

export function nativeSandboxPolicy(corpus) {
  if (process.platform !== 'darwin' || !fs.existsSync('/usr/bin/sandbox-exec')) throw new Error('Verified native corpus write isolation requires macOS sandbox-exec.');
  return `(version 1)\n(allow default)\n${corpus.map(p => `(deny file-write* (subpath ${quoted(fs.realpathSync(p))}))`).join('\n')}\n`;
}

function upstreamMcp(corpus, repoRoot, statsHome, githubToken) {
  const env = { ...process.env };
  propagateOctocodeEnv({ cwd: repoRoot, env });
  fs.mkdirSync(statsHome, { recursive: true });
  const nativeProfile = path.join(statsHome, 'native-readonly.sb');
  fs.writeFileSync(nativeProfile, nativeSandboxPolicy(corpus));
  const config = path.join(getOctocodeHome(), '.octocoderc');
  if (fs.existsSync(config)) fs.copyFileSync(config, path.join(statsHome, '.octocoderc'));
  const child = spawn('/usr/bin/sandbox-exec', ['-f', nativeProfile, process.execPath, path.join(repoRoot, 'packages/octocode-mcp/dist/index.js')], {
    detached: true, cwd: repoRoot, env: { ...env, ...(githubToken ? { GITHUB_TOKEN: githubToken } : {}), OCTOCODE_HOME: statsHome, OCTOCODE_ENABLE_STATS: 'true', OCTOCODE_STORAGE_MODE: 'persistent', ENABLE_LOCAL: 'true', WORKSPACE_ROOT: corpus[0] ?? repoRoot, ALLOWED_PATHS: corpus.join(','), DISABLE_TOOLS: 'astRewrite,ghCloneRepo' }, stdio: ['pipe', 'pipe', 'pipe'],
  });
  let buffer = '', id = 0;
  const waiting = new Map();
  const fail = e => { for (const entry of waiting.values()) { clearTimeout(entry.timer); entry.reject(e); } waiting.clear(); };
  child.once('error', fail); child.once('exit', (code, signal) => fail(new Error(`MCP bridge upstream exited ${code}/${signal}`)));
  child.stderr.on('data', () => {});
  child.stdout.on('data', bytes => {
    buffer += bytes;
    let at;
    while ((at = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, at); buffer = buffer.slice(at + 1);
      let msg; try { msg = JSON.parse(line); } catch { fail(new Error('Invalid upstream MCP frame')); child.kill(); return; }
      const entry = waiting.get(msg.id);
      if (entry) { clearTimeout(entry.timer); waiting.delete(msg.id); entry.resolve(msg); }
    }
  });
  return { child, async rpc(message) {
    if (!['initialize', 'notifications/initialized', 'tools/list', 'tools/call', 'ping'].includes(message.method)) throw new Error('MCP method denied');
    if (message.method === 'tools/call' && ['astRewrite', 'ghCloneRepo'].includes(message.params?.name)) throw new Error('Write tool denied');
    if (message.id === undefined) { child.stdin.write(JSON.stringify(message) + '\n'); return null; }
    const originalId = message.id, nextId = ++id;
    const response = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => { waiting.delete(nextId); reject(new Error('MCP gateway deadline exceeded')); }, 240000);
      waiting.set(nextId, { resolve, reject, timer });
      child.stdin.write(JSON.stringify({ ...message, id: nextId }) + '\n');
    });
    return { ...response, id: originalId };
  }, async close() {
    fail(new Error('MCP gateway closed'));
    if (child.exitCode !== null || child.signalCode !== null) return;
    await new Promise(resolve => {
      const timer = setTimeout(() => { try { process.kill(-child.pid, 'SIGKILL'); } catch { child.kill('SIGKILL'); } }, 1500);
      child.once('exit', () => { clearTimeout(timer); resolve(); });
      try { process.kill(-child.pid, 'SIGTERM'); } catch { child.kill(); }
    });
  } };
}

export function evaluatorCredentials() {
  let githubToken = process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN;
  let oauthToken = process.env.CLAUDE_CODE_OAUTH_TOKEN;
  if (!githubToken) { try { githubToken = execFileSync('gh', ['auth', 'token'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim(); } catch { /* anonymous reads */ } }
  if (!oauthToken && !process.env.ANTHROPIC_API_KEY) {
    try { oauthToken = JSON.parse(execFileSync('security', ['find-generic-password', '-s', 'Claude Code-credentials', '-w'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] })).claudeAiOauth?.accessToken; } catch { /* fail closed below */ }
  }
  return { githubToken, oauthToken };
}

export async function solverBoundary({ cwd, corpus, repoRoot, mcp = false, statsHome, githubToken, oauthToken, githubFetch = fetch }) {
  const sandboxProfile = verifySandbox(cwd, corpus);
  if (!oauthToken && !process.env.ANTHROPIC_API_KEY) throw new Error('Claude authentication must be supplied by evaluator outside isolated solver home.');
  const secret = randomBytes(24).toString('hex');
  const traffic = { modelConnects: [], rejectedConnects: 0, githubGetRequests: 0, githubRejectedWrites: 0, githubForwardedAttempts: 0, githubCompletedResponses: 0, githubUpstreamFailures: 0, githubResponseStatuses: {}, githubAttempts: [], mcpRequests: 0 };
  const nativeCalls = [];
  if (mcp && !statsHome) throw new Error('isolated evaluator statsHome required for native MCP');
  let upstream, github, gateway;
  const sockets = new Set();
  const close = async () => { for (const s of sockets) s.destroy(); await Promise.all([upstream?.close(), shutdown(gateway), shutdown(github)]); };
  try {
    upstream = mcp ? upstreamMcp(corpus, repoRoot, statsHome, githubToken) : null;
    github = http.createServer(async (req, res) => {
      if (!['GET', 'HEAD'].includes(req.method)) { traffic.githubRejectedWrites++; res.writeHead(405); res.end('Read-only gateway: REST GET/HEAD only; GraphQL POST is unavailable.'); return; }
      traffic.githubGetRequests++;
      const url = new URL(req.url, 'https://api.github.com');
      url.pathname = url.pathname.replace(/^\/api\/v3(?=\/|$)/, '');
      if (url.origin !== 'https://api.github.com' || url.pathname.startsWith('//')) { res.writeHead(400); res.end(); return; }
      const attempt = { method: req.method, startedAt: new Date().toISOString(), status: null, outcome: 'pending' };
      traffic.githubForwardedAttempts++; traffic.githubAttempts.push(attempt);
      try {
        const response = await githubFetch(url, { method: req.method, redirect: 'error', signal: AbortSignal.timeout(60000), headers: { Accept: req.headers.accept ?? 'application/vnd.github+json', 'User-Agent': 'octocode-benchmark-readonly', ...(githubToken ? { Authorization: `Bearer ${githubToken}` } : {}) } });
        attempt.status = response.status;
        traffic.githubCompletedResponses++;
        traffic.githubResponseStatuses[response.status] = (traffic.githubResponseStatuses[response.status] ?? 0) + 1;
        const body = Buffer.from(await response.arrayBuffer());
        attempt.outcome = 'response';
        res.writeHead(response.status, { 'content-type': response.headers.get('content-type') ?? 'application/json' }); res.end(body);
      } catch { attempt.outcome = 'upstream-failure'; traffic.githubUpstreamFailures++; res.writeHead(502); res.end('GitHub gateway upstream failed'); }
      finally { attempt.completedAt = new Date().toISOString(); }
    });
    const socketPath = path.join(fs.realpathSync(cwd), 'github.sock');
    await new Promise((resolve, reject) => { github.once('error', reject); github.listen(socketPath, resolve); });
    const ghConfigDir = path.join(cwd, 'gh-config');
    fs.mkdirSync(ghConfigDir);
    fs.writeFileSync(path.join(ghConfigDir, 'config.yml'), `http_unix_socket: ${socketPath}\n`);
    gateway = http.createServer(async (req, res) => {
      if (!upstream || req.url !== `/mcp/${secret}` || req.method !== 'POST') { res.writeHead(403); res.end(); return; }
      traffic.mcpRequests++;
      let body = ''; for await (const chunk of req) { body += chunk; if (body.length > 4 * 1024 * 1024) { res.writeHead(413); res.end(); return; } }
      let receipt;
      try {
        const message = JSON.parse(body);
        if (message.method === 'tools/call') {
          receipt = { requestId: `native-${nativeCalls.length + 1}`, tool: message.params?.name, startedAt: new Date().toISOString(), rowErrors: [], transportError: false, isError: false };
          nativeCalls.push(receipt);
        }
        const value = await upstream.rpc(message);
        if (receipt) {
          const walk = (node, at = '') => {
            if (!node || typeof node !== 'object') return;
            if (node.status === 'error') receipt.rowErrors.push({ path: at, errorCode: node.errorCode ?? node.data?.errorCode ?? null });
            for (const [key, child] of Object.entries(node)) walk(child, `${at}.${key}`);
          };
          walk(value?.result?.structuredContent);
          receipt.isError = value?.result?.isError === true || !!value?.error;
          receipt.completedAt = new Date().toISOString();
        }
        res.writeHead(200, { 'content-type': 'application/json' }); res.end(JSON.stringify(value));
      } catch (e) {
        if (receipt) { receipt.transportError = true; receipt.completedAt = new Date().toISOString(); }
        res.writeHead(502); res.end(JSON.stringify({ error: String(e.message) }));
      }
    });
    gateway.on('connect', (req, client, head) => {
      const [host, port] = String(req.url).split(':');
      if (port !== '443' || !MODEL_HOSTS.includes(host)) { traffic.rejectedConnects++; client.end('HTTP/1.1 403 Forbidden\r\n\r\n'); return; }
      traffic.modelConnects.push(host);
      const remote = net.connect({ host, port: 443 }); sockets.add(client); sockets.add(remote);
      remote.once('connect', () => { client.write('HTTP/1.1 200 Connection Established\r\n\r\n'); if (head.length) remote.write(head); client.pipe(remote); remote.pipe(client); });
      remote.on('error', () => client.destroy()); client.on('error', () => remote.destroy());
      client.on('close', () => { sockets.delete(client); remote.destroy(); }); remote.on('close', () => sockets.delete(remote));
    });
    const proxyPort = await listen(gateway);
    fs.writeFileSync(sandboxProfile, sandboxPolicy({ cwd, corpus, ports: [proxyPort] }));
    const bridge = path.join(cwd, 'mcp-bridge.cjs');
    if (mcp) fs.writeFileSync(bridge, `const http=require('http');const rl=require('readline').createInterface({input:process.stdin});rl.on('line',line=>{let m;try{m=JSON.parse(line)}catch{return}const r=http.request(${JSON.stringify(`http://127.0.0.1:${proxyPort}/mcp/${secret}`)},{method:'POST',headers:{'content-type':'application/json'}},res=>{let b='';res.on('data',c=>b+=c);res.on('end',()=>{if(res.statusCode!==200){console.error('MCP bridge rejected request');process.exit(1)}if(b!=='null')process.stdout.write(b+'\\n')})});r.on('error',()=>process.exit(1));r.end(line)});`);
    const env = { PATH: process.env.PATH, HOME: cwd, TMPDIR: cwd, CLAUDE_CODE_TMPDIR: cwd, CLAUDE_TMPDIR: cwd, BUN_TMPDIR: cwd, DISABLE_TELEMETRY: '1', DISABLE_ERROR_REPORTING: '1', DISABLE_AUTOUPDATER: '1', CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: '1', SHELL: '/bin/bash', LANG: 'en_US.UTF-8', CLAUDE_CONFIG_DIR: path.join(cwd, 'claude-config'), CLAUDE_CODE_OAUTH_TOKEN: oauthToken, ANTHROPIC_API_KEY: process.env.ANTHROPIC_API_KEY, HTTPS_PROXY: `http://127.0.0.1:${proxyPort}`, HTTP_PROXY: `http://127.0.0.1:${proxyPort}`, NO_PROXY: 'localhost,127.0.0.1', GH_HOST: 'api.github.com', GH_CONFIG_DIR: ghConfigDir, GH_TOKEN: 'local-readonly-gateway' };
    return { sandboxProfile, env, bridge, githubSocket: socketPath, proxyPort, version: ISOLATION_VERSION,
      traffic: () => structuredClone(traffic),
      nativeCalls: () => structuredClone(nativeCalls),
      providerStats: () => { try { return JSON.parse(fs.readFileSync(path.join(statsHome, 'stats.json'), 'utf8')).stats?.clasify ?? null; } catch { return null; } },
      githubStats: () => { try { return JSON.parse(fs.readFileSync(path.join(statsHome, 'stats.json'), 'utf8')).stats?.github ?? null; } catch { return null; } },
      close };
  } catch (error) { await close(); throw error; }
}
