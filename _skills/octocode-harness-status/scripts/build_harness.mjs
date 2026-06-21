#!/usr/bin/env node
// octocode-harness-status — interactive AI tooling harness dashboard
// Usage: node build_harness.mjs [--port N] [--no-open] [--timeout S] [--help]

import { readFileSync, writeFileSync, existsSync, readdirSync, statSync, rmSync, realpathSync } from 'node:fs';
import { createServer } from 'node:http';
import { request as httpsRequest } from 'node:https';
import { homedir, platform } from 'node:os';
import { join, basename, sep } from 'node:path';
import { spawnSync } from 'node:child_process';
import { VENDORS as VENDOR_DEFS, PLATFORM_INFO } from './vendors.js';

const HOME = homedir();

// ─── CLI args ────────────────────────────────────────────────────────────────

function parseArgs(argv) {
  const args = argv.slice(2);
  const opts = { port: 0, open: true, timeout: 300 };
  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case '--port':    opts.port    = parseInt(args[++i], 10); break;
      case '--no-open': opts.open    = false; break;
      case '--timeout': opts.timeout = parseInt(args[++i], 10); break;
      case '--help': case '-h': printHelp(); process.exit(0); break;
      default: die(`Unknown flag: ${args[i]}`);
    }
  }
  return opts;
}

function printHelp() {
  console.log(`build_harness.mjs — AI tooling harness dashboard

Options:
  --port <n>      HTTP server port (default: auto)
  --no-open       Don't open the browser automatically
  --timeout <s>   Auto-shutdown after N seconds (default: 300)
  -h, --help      Show this help`);
}

function die(msg) { console.error(`error: ${msg}`); process.exit(1); }

// ─── Vendor definitions (from vendors.js) ────────────────────────────────────

const VENDORS = VENDOR_DEFS;

// ─── CLI tools ───────────────────────────────────────────────────────────────

const CLI_TOOLS = [
  { id: 'octocode', name: 'octocode',    emoji: '🐙', versionCmd: ['npx', 'octocode', '--version'], authCmd: ['npx', 'octocode', 'status'], authPattern: /Authenticated as ([^\s\n]+)/, authNegPattern: /not authenticated|not logged/i },
  { id: 'gh',           name: 'gh (GitHub CLI)', emoji: '🐱', versionCmd: ['gh', '--version'],                  authCmd: ['gh', 'auth', 'status'],          authPattern: /Logged in to github\.com account ([^\s(]+)/, authNegPattern: /not logged in/i },
  { id: 'claude',       name: 'Claude Code CLI', emoji: '🔶', versionCmd: ['claude', '--version'],              authCmd: null, authPattern: null, authNegPattern: null },
  { id: 'cursor',       name: 'Cursor CLI',      emoji: '⬛', versionCmd: ['cursor', '--version'],              authCmd: null, authPattern: null, authNegPattern: null },
  { id: 'gemini',       name: 'Gemini CLI',      emoji: '✨', versionCmd: ['gemini', '--version'],              authCmd: null, authPattern: null, authNegPattern: null },
  { id: 'codex',        name: 'Codex CLI',       emoji: '🧠', versionCmd: ['codex',  '--version'],              authCmd: null, authPattern: null, authNegPattern: null },
  { id: 'goose',        name: 'Goose',           emoji: '🦆', versionCmd: ['goose',  '--version'],              authCmd: null, authPattern: null, authNegPattern: null },
  { id: 'opencode',     name: 'OpenCode',        emoji: '🔓', versionCmd: ['opencode','--version'],             authCmd: null, authPattern: null, authNegPattern: null },
];

// ─── Data collection ─────────────────────────────────────────────────────────

function run(cmd, args, { timeout = 8000 } = {}) {
  try {
    const r = spawnSync(cmd, args, { timeout, encoding: 'utf8', shell: false });
    return (r.stdout || '') + (r.stderr || '');
  } catch { return null; }
}

function readJsonFile(p) {
  if (!p || !existsSync(p)) return null;
  try { return JSON.parse(readFileSync(p, 'utf8')); } catch { return null; }
}

// Strip surrounding quotes from a TOML/YAML scalar.
function unquote(s) {
  s = s.trim();
  if ((s.startsWith('"') && s.endsWith('"')) || (s.startsWith("'") && s.endsWith("'"))) {
    return s.slice(1, -1);
  }
  return s;
}

// Parse a single-line inline array: ["a", "b"] → ['a','b'].
function parseInlineArray(s) {
  const inner = s.trim().replace(/^\[/, '').replace(/\]$/, '').trim();
  if (!inner) return [];
  return inner.split(',').map(x => unquote(x.trim())).filter(x => x.length);
}

// Best-effort TOML reader for MCP server tables ([mcp_servers.NAME] / [mcpServers.NAME]).
// Read-only — we never write TOML back. Returns { NAME: {command, args, env} }.
function parseTomlMcpServers(content) {
  const servers = {};
  const lines = content.split(/\r?\n/);
  let cur = null, curEnv = false, pendingArrayKey = null, pendingArrayBuf = '';
  const flushArray = () => {
    if (cur && pendingArrayKey) servers[cur][pendingArrayKey] = parseInlineArray(pendingArrayBuf);
    pendingArrayKey = null; pendingArrayBuf = '';
  };
  for (const raw of lines) {
    const line = raw.trim();
    if (pendingArrayKey) { pendingArrayBuf += ' ' + line; if (line.includes(']')) flushArray(); continue; }
    if (!line || line.startsWith('#')) continue;
    const header = line.match(/^\[(?:mcp_servers|mcpServers)\.(.+?)\]$/);
    if (header) {
      let name = header[1];
      if (name.endsWith('.env')) { name = unquote(name.slice(0, -4)); curEnv = true; }
      else { name = unquote(name); curEnv = false; }
      cur = name;
      if (!servers[cur]) servers[cur] = { command: '', args: [], env: {} };
      continue;
    }
    if (line.startsWith('[')) { cur = null; curEnv = false; continue; } // left the mcp section
    if (!cur) continue;
    const kv = line.match(/^([A-Za-z0-9_-]+)\s*=\s*(.+)$/);
    if (!kv) continue;
    const key = kv[1], val = kv[2].trim();
    if (curEnv) { servers[cur].env[key] = unquote(val); continue; }
    if (key === 'command') servers[cur].command = unquote(val);
    else if (key === 'args') {
      if (val.includes('[') && !val.includes(']')) { pendingArrayKey = 'args'; pendingArrayBuf = val; }
      else servers[cur].args = parseInlineArray(val);
    }
  }
  return servers;
}

// Best-effort YAML reader for Goose-style `extensions:` blocks (read-only).
function parseYamlMcpServers(content, topKey) {
  const servers = {};
  const lines = content.split(/\r?\n/);
  let inSection = false, sectionIndent = 0, cur = null, curIndent = 0, inArgs = false;
  for (const raw of lines) {
    if (!raw.trim() || raw.trim().startsWith('#')) continue;
    const indent = raw.length - raw.trimStart().length;
    const line = raw.trim();
    if (!inSection) {
      if (line === `${topKey}:`) { inSection = true; sectionIndent = indent; }
      continue;
    }
    if (indent <= sectionIndent) break; // left the section
    const entry = line.match(/^([A-Za-z0-9_.-]+):\s*(.*)$/);
    if (entry && (cur === null || indent <= curIndent)) {
      // A new named extension at the first level under the section.
      if (cur === null || indent === curIndent) {
        cur = entry[1]; curIndent = indent; inArgs = false;
        if (!servers[cur]) servers[cur] = { command: '', args: [], env: {} };
        continue;
      }
    }
    if (!cur) continue;
    if (inArgs && line.startsWith('- ')) { servers[cur].args.push(unquote(line.slice(2))); continue; }
    inArgs = false;
    const kv = line.match(/^([A-Za-z0-9_.-]+):\s*(.*)$/);
    if (!kv) continue;
    const key = kv[1], val = kv[2].trim();
    if ((key === 'cmd' || key === 'command') && val) servers[cur].command = unquote(val);
    else if (key === 'args') { if (val.startsWith('[')) servers[cur].args = parseInlineArray(val); else inArgs = true; }
  }
  return servers;
}

// Load MCP servers for a vendor honouring its declared config format.
// Returns { servers: {name: cfg}, readOnly: bool } or null if unreadable.
function loadMcpServers(vendor) {
  const path = vendor.mcpConfigPath, fmt = vendor.mcpConfigFormat || 'json';
  if (!path || !existsSync(path)) return null;
  if (fmt === 'json') {
    const raw = readJsonFile(path);
    if (raw && vendor.mcpKey && raw[vendor.mcpKey]) return { servers: raw[vendor.mcpKey], readOnly: false };
    return { servers: {}, readOnly: false };
  }
  let content;
  try { content = readFileSync(path, 'utf8'); } catch { return null; }
  try {
    if (fmt === 'toml') return { servers: parseTomlMcpServers(content), readOnly: true };
    if (fmt === 'yaml') return { servers: parseYamlMcpServers(content, vendor.mcpKey || 'extensions'), readOnly: true };
  } catch { /* fall through */ }
  return { servers: {}, readOnly: true };
}

function listScriptFiles(skillDir) {
  const scriptsDir = join(skillDir, 'scripts');
  if (!existsSync(scriptsDir)) return [];
  try {
    return readdirSync(scriptsDir, { withFileTypes: true })
      .filter(e => e.isFile())
      .map(e => ({
        name: e.name,
        path: join(scriptsDir, e.name),
        bytes: (() => { try { return statSync(join(scriptsDir, e.name)).size; } catch { return 0; } })(),
      }));
  } catch { return []; }
}

function readSkillMeta(skillDir) {
  const mdPath = join(skillDir, 'SKILL.md');
  if (!existsSync(mdPath)) return null;
  try {
    const content = readFileSync(mdPath, 'utf8');
    const bytes = Buffer.byteLength(content, 'utf8');
    const fm = content.match(/^---\n([\s\S]*?)\n---/);
    let name = null, description = null;
    if (fm) {
      const nm = fm[1].match(/^name:\s*(.+)$/m);
      const dm = fm[1].match(/^description:\s*([\s\S]+?)(?=\n\w|\n---\n|$)/m);
      name = nm ? nm[1].trim() : null;
      description = dm ? dm[1].replace(/\n\s+/g, ' ').trim() : null;
    }
    const scripts = listScriptFiles(skillDir);
    const descBytes = description ? Buffer.byteLength(description, 'utf8') : 0;
    return { name: name || basename(skillDir), description, bytes, descBytes, scripts, path: skillDir };
  } catch { return null; }
}

function collectVendorData(vendor) {
  const skillsDirAliases = Array.isArray(vendor.skillsDirAliases) ? vendor.skillsDirAliases : [];
  const result = {
    id: vendor.id, name: vendor.name, emoji: vendor.emoji,
    category: vendor.category || 'other', installUrl: vendor.installUrl || '', color: vendor.color || '#6366f1',
    contextWindow: vendor.contextWindow, agentModel: vendor.agentModel,
    mcpConfigPath: vendor.mcpConfigPath, mcpKey: vendor.mcpKey || 'mcpServers',
    mcpConfigFormat: vendor.mcpConfigFormat || 'json', mcpReadOnly: false,
    mcpSupport: vendor.mcpSupport !== false, skillsSupport: vendor.skillsSupport === true,
    skillsDir: vendor.skillsDir,
    skillsDirAliases,
    mcpServers: [],
    skills: [],
    configured: false, skillsConfigured: false,
    totalSkillBytes: 0, totalDescBytes: 0,
  };

  if (vendor.mcpConfigPath && existsSync(vendor.mcpConfigPath)) {
    result.configured = true;
    const loaded = loadMcpServers(vendor);
    if (loaded) {
      result.mcpReadOnly = loaded.readOnly;
      for (const [name, cfg] of Object.entries(loaded.servers)) {
        if (!cfg || typeof cfg !== 'object') continue;
        result.mcpServers.push({
          name,
          command: cfg.command || cfg.cmd || (cfg.url ? `(http) ${cfg.url}` : ''),
          args: Array.isArray(cfg.args) ? cfg.args : [],
          env: cfg.env && typeof cfg.env === 'object' ? cfg.env : {},
          envKeys: cfg.env && typeof cfg.env === 'object' ? Object.keys(cfg.env) : [],
          type: cfg.type || (cfg.url ? 'http' : 'stdio'),
        });
      }
    }
  }

  // Scan primary skills dir + all alias dirs
  const allSkillDirs = [
    ...(vendor.skillsDir ? [vendor.skillsDir] : []),
    ...(Array.isArray(vendor.skillsDirAliases) ? vendor.skillsDirAliases : []),
  ];
  for (const skillDir of allSkillDirs) {
    if (!skillDir || !existsSync(skillDir)) continue;
    result.skillsConfigured = true;
    try {
      for (const entry of readdirSync(skillDir, { withFileTypes: true })) {
        if (!entry.isDirectory()) continue;
        const meta = readSkillMeta(join(skillDir, entry.name));
        if (meta) {
          // Tag each skill with its parent directory so the remove API uses the right path
          meta.parentDir = skillDir;
          result.skills.push(meta);
          result.totalSkillBytes += meta.bytes;
          result.totalDescBytes += meta.descBytes;
        }
      }
    } catch { /* ignore */ }
  }

  return result;
}

function collectCliData(tool) {
  const out = run(tool.versionCmd[0], tool.versionCmd.slice(1));
  const installed = out !== null && !out.includes('not found') && !out.includes('command not found');
  let version = null;
  if (installed && out) {
    const vm = out.match(/(\d+\.\d+[\.\d]*)/);
    version = vm ? vm[1] : out.trim().split('\n')[0].trim().slice(0, 40);
  }
  let authUser = null, authStatus = 'unknown';
  if (installed && tool.authCmd) {
    const ao = run(tool.authCmd[0], tool.authCmd.slice(1));
    if (ao) {
      if (tool.authPattern) { const m = ao.match(tool.authPattern); if (m) { authUser = m[1]; authStatus = 'authenticated'; } }
      if (authStatus !== 'authenticated' && tool.authNegPattern?.test(ao)) authStatus = 'not authenticated';
      if (authStatus === 'unknown' && ao.toLowerCase().includes('authenticated')) authStatus = 'authenticated';
    }
  } else if (installed) { authStatus = 'n/a'; }
  return { id: tool.id, name: tool.name, emoji: tool.emoji, installed, version, authStatus, authUser };
}

async function fetchGitHubRateLimit() {
  try {
    const tokenOut = run('gh', ['auth', 'token']);
    const token = tokenOut ? tokenOut.trim() : null;
    if (!token || token.length < 10) return null;
    return new Promise((resolve) => {
      const req = httpsRequest(
        { hostname: 'api.github.com', path: '/rate_limit', method: 'GET',
          headers: { 'User-Agent': 'octocode-harness/1.0', 'Authorization': `Bearer ${token}`, 'Accept': 'application/vnd.github+json' } },
        (res) => {
          let data = '';
          res.on('data', d => data += d);
          res.on('end', () => { try { const j = JSON.parse(data); resolve({ core: j.resources?.core, graphql: j.resources?.graphql, search: j.resources?.search }); } catch { resolve(null); } });
        });
      req.on('error', () => resolve(null));
      req.setTimeout(5000, () => { req.destroy(); resolve(null); });
      req.end();
    });
  } catch { return null; }
}

// ─── Rating helpers ───────────────────────────────────────────────────────────

function rateSkillBudget(totalBytes, contextWindow) {
  const tokens = Math.round(totalBytes / 4);
  const pct = contextWindow > 0 ? (tokens / contextWindow) * 100 : 0;
  let grade, colour;
  if (pct < 5)        { grade = 'Lean';     colour = '#22c55e'; }
  else if (pct < 15)  { grade = 'Moderate'; colour = '#eab308'; }
  else if (pct < 30)  { grade = 'Heavy';    colour = '#f97316'; }
  else                { grade = 'Critical'; colour = '#ef4444'; }
  return { tokens, pct: Math.round(pct * 10) / 10, grade, colour };
}

function rateMcpLoad(count) {
  if (count === 0)      return { grade: 'None',     colour: '#6b7280', pct: 0 };
  if (count <= 2)       return { grade: 'Lean',     colour: '#22c55e', pct: Math.round((count / 12) * 100) };
  if (count <= 5)       return { grade: 'Moderate', colour: '#eab308', pct: Math.round((count / 12) * 100) };
  if (count <= 8)       return { grade: 'Heavy',    colour: '#f97316', pct: Math.round((count / 12) * 100) };
  return               { grade: 'Critical', colour: '#ef4444', pct: Math.min(100, Math.round((count / 12) * 100)) };
}

// ─── Mutation helpers ─────────────────────────────────────────────────────────

// Reject any name that could escape its parent directory via traversal/separators.
function assertSafeName(name, label = 'name') {
  if (typeof name !== 'string' || !name.length) throw new Error(`Invalid ${label}`);
  if (name.includes('/') || name.includes('\\') || name.includes('\0') || name === '..' || name.includes('..')) {
    throw new Error(`Unsafe ${label} rejected: "${name}"`);
  }
}

function removeMcpFromConfig(configPath, mcpKey, serverName) {
  assertSafeName(serverName, 'server name');
  if (!configPath || !existsSync(configPath)) throw new Error('Config file not found');
  const raw = readJsonFile(configPath);
  if (!raw || !raw[mcpKey]?.[serverName]) throw new Error(`Server "${serverName}" not found`);
  delete raw[mcpKey][serverName];
  writeFileSync(configPath, JSON.stringify(raw, null, 2) + '\n', 'utf8');
}

function updateMcpInConfig(configPath, mcpKey, serverName, newCfg) {
  assertSafeName(serverName, 'server name');
  if (!configPath || !existsSync(configPath)) throw new Error('Config file not found');
  const raw = readJsonFile(configPath);
  if (!raw || !raw[mcpKey]) throw new Error('MCP section not found');
  // Merge over the existing entry so fields the editor doesn't expose
  // (url, headers, disabled, autoApprove, cwd, …) are preserved, not dropped.
  const existing = (raw[mcpKey][serverName] && typeof raw[mcpKey][serverName] === 'object') ? raw[mcpKey][serverName] : {};
  const merged = { ...existing, command: newCfg.command, args: newCfg.args || [], type: newCfg.type || existing.type || 'stdio' };
  if (newCfg.env && Object.keys(newCfg.env).length) merged.env = newCfg.env; else delete merged.env;
  raw[mcpKey][serverName] = merged;
  writeFileSync(configPath, JSON.stringify(raw, null, 2) + '\n', 'utf8');
}

function removeSkillFolder(skillsDir, skillName) {
  assertSafeName(skillName, 'skill name');
  if (!skillsDir) throw new Error('No skills directory for this vendor');
  const target = join(skillsDir, skillName);
  if (!existsSync(target)) throw new Error(`Not found: ${target}`);
  rmSync(target, { recursive: true, force: true });
}

function deleteFile(filePath) {
  if (!filePath || !existsSync(filePath)) throw new Error(`File not found: ${filePath}`);
  const resolved = realpathSync(filePath);
  // Safety: only allow real files (not directories) under the home directory.
  if (!resolved.startsWith(HOME + sep) && resolved !== HOME) throw new Error('Path outside home directory rejected');
  if (statSync(resolved).isDirectory()) throw new Error('Refusing to delete a directory');
  rmSync(resolved, { force: true });
}

// ─── HTML helpers ─────────────────────────────────────────────────────────────

const fmtBytes = b => b < 1024 ? `${b} B` : b < 1048576 ? `${(b/1024).toFixed(1)} KB` : `${(b/1048576).toFixed(2)} MB`;
const fmtTok   = t => t < 1000 ? `${t}` : `${(t/1000).toFixed(1)}k`;
const esc      = s => String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;').replace(/'/g,'&#39;');
const escJs    = s => String(s).replace(/\\/g,'\\\\').replace(/'/g,"\\'").replace(/"/g,'\\"');

// ─── HTML generation ──────────────────────────────────────────────────────────

function buildHtml(data) {
  const { vendors, clis, rateLimit, generatedAt } = data;

  const totalMcps   = vendors.reduce((s, v) => s + v.mcpServers.length, 0);
  const totalSkills = vendors.reduce((s, v) => s + v.skills.length, 0);
  const configured  = vendors.filter(v => v.configured || v.skillsConfigured).length;
  const authClis    = clis.filter(c => c.authStatus === 'authenticated').length;
  const instClis    = clis.filter(c => c.installed).length;
  const totalSkillTokens = Math.round(vendors.reduce((s, v) => s + v.totalSkillBytes, 0) / 4);

  const CAT_LABELS = { ide: 'IDE', cli: 'CLI', desktop: 'Desktop', extension: 'Extension', 'agent-folder': 'Agent', pi: 'Pi', other: 'Other' };
  const CAT_ORDER = ['ide', 'cli', 'desktop', 'extension', 'agent-folder', 'pi', 'other'];
  const presentCats = CAT_ORDER.filter(c => vendors.some(v => v.category === c));

  // Serialise vendor data for client-side JS edits
  const vendorMeta = JSON.stringify(
    vendors.map(v => ({
      id: v.id, mcpConfigPath: v.mcpConfigPath, mcpKey: v.mcpKey,
      skillsDir: v.skillsDir, skillsDirAliases: v.skillsDirAliases || [],
      mcpServers: v.mcpServers.map(m => ({ name: m.name, command: m.command, args: m.args, env: m.env, type: m.type })),
    }))
  );

  const vendorCards = vendors.map(v => {
    const isConf   = v.configured || v.skillsConfigured;
    const hasItems = v.mcpServers.length || v.skills.length;
    const dot      = !isConf ? 'off' : hasItems ? 'ok' : 'warn';
    const sb = rateSkillBudget(v.totalSkillBytes, v.contextWindow);
    const mb = rateMcpLoad(v.mcpServers.length);
    const catLabel = CAT_LABELS[v.category] || 'Other';

    // ── Unconfigured vendor → compact inventory card ──
    if (!isConf) {
      const supports = [
        v.mcpSupport ? '<span class="cap-tag">MCP</span>' : '',
        v.skillsSupport ? '<span class="cap-tag">Skills</span>' : '',
      ].join('');
      return `
      <article class="vendor-card off" id="card-${esc(v.id)}" data-vendor="${esc(v.id)}" data-category="${esc(v.category)}"
        data-name="${esc(v.name.toLowerCase())}" data-mcps="0" data-skills="0" data-ctx="0" data-configured="0">
        <header class="vendor-header" onclick="toggleCard('${esc(v.id)}')" role="button" aria-expanded="false">
          <span class="status-dot dot-${dot}" title="Not configured"></span>
          <span class="vendor-emoji">${v.emoji}</span>
          <div class="vendor-info">
            <h2>${esc(v.name)} <span class="cat-tag">${esc(catLabel)}</span></h2>
            <span class="vendor-model">Not configured on this machine</span>
          </div>
          <div class="vendor-counts">${supports}</div>
          <span class="caret collapsed" id="caret-${esc(v.id)}" aria-hidden="true">&#8250;</span>
        </header>
        <div class="vendor-body collapsed" id="body-${esc(v.id)}">
          ${v.mcpConfigPath ? `<div class="config-path mono"><span class="cfg-ico">&#128196;</span><span class="muted">expected: ${esc(v.mcpConfigPath)}</span></div>` : ''}
          ${v.skillsDir ? `<div class="config-path mono"><span class="cfg-ico">&#128193;</span><span class="muted">expected: ${esc(v.skillsDir)}</span></div>` : ''}
          <div class="empty-state">
            No config or skills found.${v.installUrl ? ` <a href="${esc(v.installUrl)}" target="_blank" rel="noopener">Install / docs &#8599;</a>` : ''}
          </div>
        </div>
      </article>`;
    }

    // ── MCP rows ──
    const mcpSel = !v.mcpReadOnly;
    const mcpRows = [...v.mcpServers].sort((a, b) => a.name.localeCompare(b.name)).map(m => {
      const cmd = m.command + (m.args.length ? ' ' + m.args.join(' ') : '');
      return `
      <tr class="mcp-row" id="mcp-row-${esc(v.id)}-${esc(m.name)}" data-vendor="${esc(v.id)}" data-item="${esc(m.name)}" data-name="${esc(m.name.toLowerCase())}" data-cmd="${esc(cmd.toLowerCase())}" data-env="${m.envKeys.length}">
        ${mcpSel ? `<td class="td-check"><input type="checkbox" class="row-sel" onchange="onSel(this)" aria-label="Select ${esc(m.name)}"/></td>` : ''}
        <td class="td-name"><span class="badge badge-mcp">MCP</span><span class="nm">${esc(m.name)}</span></td>
        <td class="td-cmd mono" title="${esc(cmd)}">${esc(cmd) || '<span class="muted">&mdash;</span>'}</td>
        <td class="td-env td-num">${m.envKeys.length ? m.envKeys.map(k => `<span class="env-tag">${esc(k)}</span>`).join('') : '<span class="muted">&mdash;</span>'}</td>
        <td class="td-actions">${v.mcpReadOnly
          ? `<span class="ro-lock" title="${esc(v.mcpConfigFormat.toUpperCase())} config &mdash; read-only">&#128274;</span>`
          : `<button class="btn-icon btn-edit" title="Edit" aria-label="Edit ${esc(m.name)}" onclick="openEditMcp('${escJs(v.id)}','${escJs(m.name)}')">&#9998;</button>
          <button class="btn-icon btn-del" title="Remove" aria-label="Remove ${esc(m.name)}" onclick="confirmRemove('mcp','${escJs(v.id)}','${escJs(m.name)}','${escJs(v.mcpConfigPath||'')}','${escJs(v.mcpKey)}')">&times;</button>`}
        </td>
      </tr>`;
    }).join('');

    // ── Skill rows (+ grouped script sub-rows) ──
    const skillRows = [...v.skills].sort((a, b) => b.bytes - a.bytes).map(s => {
      const groupKey = `${esc(v.id)}-${esc(s.name)}`;
      const scriptRows = s.scripts.map(f => `
        <tr class="script-row hidden" data-scripts="${groupKey}">
          <td class="td-check"></td>
          <td colspan="2"><span class="badge badge-script">SCRIPT</span><span class="mono">${esc(f.name)}</span></td>
          <td class="td-size mono td-num">${fmtBytes(f.bytes)}</td>
          <td class="td-actions">
            <button class="btn-icon btn-del" title="Delete file" aria-label="Delete ${esc(f.name)}" onclick="confirmDeleteFile('${escJs(f.path)}','${escJs(f.name)}',this)">&times;</button>
          </td>
        </tr>`).join('');
      const scriptToggle = s.scripts.length
        ? `<button class="chip-scripts" onclick="event.stopPropagation();toggleScripts('${escJs(v.id)}-${escJs(s.name)}')">${s.scripts.length} script${s.scripts.length>1?'s':''}</button>`
        : '';
      const desc = (s.description || '').slice(0, 120);
      return `
      <tr class="skill-row" id="skill-row-${esc(v.id)}-${esc(s.name)}" data-vendor="${esc(v.id)}" data-item="${esc(s.name)}" data-group="${groupKey}" data-name="${esc(s.name.toLowerCase())}" data-size="${s.bytes}">
        <td class="td-check"><input type="checkbox" class="row-sel" onchange="onSel(this)" aria-label="Select ${esc(s.name)}"/></td>
        <td class="td-name"><span class="badge badge-skill">SKILL</span><span class="nm">${esc(s.name)}</span> ${scriptToggle}</td>
        <td class="td-desc" title="${esc(s.description||'')}">${esc(desc)}${(s.description||'').length>120?'&hellip;':''}</td>
        <td class="td-size mono td-num">${fmtBytes(s.bytes)}<span class="desc-size muted">desc ${fmtBytes(s.descBytes)}</span></td>
        <td class="td-actions">
          <button class="btn-icon btn-del" title="Delete skill" aria-label="Delete ${esc(s.name)}" onclick="confirmRemove('skill','${escJs(v.id)}','${escJs(s.name)}','${escJs(s.parentDir||v.skillsDir||'')}','')">&times;</button>
        </td>
      </tr>
      ${scriptRows}`;
    }).join('');

    const budgetBars = [];
    if (v.skills.length) {
      const dl = rateSkillBudget(v.totalDescBytes, v.contextWindow);
      budgetBars.push(`
        <div class="budget-row" title="Total SKILL.md bytes vs ${fmtTok(v.contextWindow)} context window">
          <span class="budget-label">Skills context</span>
          <div class="budget-bar-wrap"><div class="budget-bar" style="width:${Math.min(sb.pct,100)}%;background:${sb.colour}"></div></div>
          <span class="budget-val" style="color:${sb.colour}">${sb.grade} &middot; ${sb.pct}% &middot; ~${fmtTok(sb.tokens)} tok</span>
        </div>
        <div class="budget-row" title="Bytes of description front-matter (always loaded)">
          <span class="budget-label">Desc. load</span>
          <div class="budget-bar-wrap"><div class="budget-bar" style="width:${Math.min(dl.pct,100)}%;background:${dl.colour}"></div></div>
          <span class="budget-val" style="color:${dl.colour}">~${fmtTok(Math.round(v.totalDescBytes/4))} tok descriptions</span>
        </div>`);
    }
    if (v.mcpServers.length) {
      budgetBars.push(`
        <div class="budget-row" title="Number of MCP servers configured for this vendor">
          <span class="budget-label">MCP load</span>
          <div class="budget-bar-wrap"><div class="budget-bar" style="width:${mb.pct}%;background:${mb.colour}"></div></div>
          <span class="budget-val" style="color:${mb.colour}">${mb.grade} &middot; ${v.mcpServers.length} server${v.mcpServers.length!==1?'s':''}</span>
        </div>`);
    }

    const itemCount = v.mcpServers.length + v.skills.length;

    return `
    <article class="vendor-card" id="card-${esc(v.id)}" data-vendor="${esc(v.id)}" data-category="${esc(v.category)}"
      data-name="${esc(v.name.toLowerCase())}" data-mcps="${v.mcpServers.length}" data-skills="${v.skills.length}" data-ctx="${sb.pct}" data-configured="1">
      <header class="vendor-header" onclick="toggleCard('${esc(v.id)}')" role="button" aria-expanded="true">
        <span class="status-dot dot-${dot}" title="${dot==='ok'?'Active':'Configured, empty'}"></span>
        <span class="vendor-emoji">${v.emoji}</span>
        <div class="vendor-info">
          <h2>${esc(v.name)} <span class="cat-tag">${esc(catLabel)}</span></h2>
          <span class="vendor-model">${esc(v.agentModel)} &middot; ${fmtTok(v.contextWindow)} ctx</span>
        </div>
        <div class="vendor-counts">
          ${v.mcpServers.length ? `<span class="count-badge count-mcp">${v.mcpServers.length} MCP</span>` : ''}
          ${v.skills.length ? `<span class="count-badge count-skill">${v.skills.length} skill${v.skills.length>1?'s':''}</span>` : ''}
        </div>
        <span class="caret" id="caret-${esc(v.id)}" aria-hidden="true">&#8250;</span>
      </header>
      <div class="vendor-body" id="body-${esc(v.id)}">
        ${v.mcpConfigPath ? `<div class="config-path mono copyable" onclick="copyText('${escJs(v.mcpConfigPath)}')" title="Click to copy"><span class="cfg-ico">&#128196;</span>${esc(v.mcpConfigPath)}${v.mcpReadOnly?` <span class="ro-note">&#128274; ${esc(v.mcpConfigFormat.toUpperCase())}</span>`:''}<span class="copy-hint">copy</span></div>` : ''}
        ${v.skillsDir ? `<div class="config-path mono copyable" onclick="copyText('${escJs(v.skillsDir)}')" title="Click to copy"><span class="cfg-ico">&#128193;</span>${esc(v.skillsDir)}<span class="copy-hint">copy</span></div>` : ''}
        ${(v.skillsDirAliases || []).map(d => existsSync(d) ? `<div class="config-path mono copyable" onclick="copyText('${escJs(d)}')" title="Click to copy"><span class="cfg-ico">&#128193;</span>${esc(d)} <span class="ro-note" style="background:rgba(109,107,246,.15);color:var(--primary2)">alias</span><span class="copy-hint">copy</span></div>` : '').join('')}
        ${budgetBars.join('')}
        ${itemCount ? `
        <div class="card-toolbar">
          <input class="card-filter" type="text" placeholder="Filter this vendor&hellip;" oninput="filterItems(this)" autocomplete="off" aria-label="Filter ${esc(v.name)} items"/>
        </div>` : ''}
        ${v.mcpServers.length ? `
        <section class="table-section" data-table="mcp" data-vendor="${esc(v.id)}">
          <div class="table-title">MCP Servers <span class="tt-count">${v.mcpServers.length}</span>${v.mcpReadOnly ? ` <span class="ro-note">&#128274; ${esc(v.mcpConfigFormat.toUpperCase())} read-only</span>` : ''}</div>
          ${mcpSel ? `<div class="bulk-bar"><span class="bulk-count">0 selected</span><div class="bulk-actions"><button class="btn-bulk-del" onclick="bulkRemove(this)">Remove selected</button><button class="btn-bulk-clear" onclick="clearSel(this)">Clear</button></div></div>` : ''}
          <div class="table-scroll">
          <table class="item-table">
            <thead><tr>
              ${mcpSel ? `<th class="th-check"><input type="checkbox" class="sel-all" onchange="toggleAll(this)" aria-label="Select all MCP servers"/></th>` : ''}
              <th class="sortable sort-asc" data-key="name" onclick="sortTable(this)" aria-sort="ascending">Name</th>
              <th class="sortable" data-key="cmd" onclick="sortTable(this)">Command</th>
              <th class="sortable th-num" data-key="env" onclick="sortTable(this)">Env</th>
              <th class="th-actions"></th>
            </tr></thead>
            <tbody>${mcpRows}</tbody>
            <tfoot><tr><td colspan="${mcpSel ? 5 : 4}">${v.mcpServers.length} server${v.mcpServers.length!==1?'s':''} &middot; ${v.mcpServers.reduce((s,m)=>s+m.envKeys.length,0)} env key${v.mcpServers.reduce((s,m)=>s+m.envKeys.length,0)!==1?'s':''}</td></tr></tfoot>
          </table>
          </div>
        </section>` : ''}
        ${v.skills.length ? `
        <section class="table-section" data-table="skill" data-vendor="${esc(v.id)}">
          <div class="table-title">Skills <span class="tt-count">${v.skills.length}</span></div>
          <div class="bulk-bar"><span class="bulk-count">0 selected</span><div class="bulk-actions"><button class="btn-bulk-del" onclick="bulkRemove(this)">Remove selected</button><button class="btn-bulk-clear" onclick="clearSel(this)">Clear</button></div></div>
          <div class="table-scroll">
          <table class="item-table">
            <thead><tr>
              <th class="th-check"><input type="checkbox" class="sel-all" onchange="toggleAll(this)" aria-label="Select all skills"/></th>
              <th class="sortable" data-key="name" onclick="sortTable(this)">Name</th>
              <th>Description</th>
              <th class="sortable th-num sort-desc" data-key="size" onclick="sortTable(this)" aria-sort="descending">Size</th>
              <th class="th-actions"></th>
            </tr></thead>
            <tbody>${skillRows}</tbody>
            <tfoot><tr><td colspan="5">${v.skills.length} skill${v.skills.length!==1?'s':''} &middot; ${fmtBytes(v.totalSkillBytes)} &middot; ~${fmtTok(sb.tokens)} tok total</td></tr></tfoot>
          </table>
          </div>
        </section>` : ''}
        ${!itemCount ? '<div class="empty-state">Configured, but no MCP servers or skills found.</div>' : ''}
        <div class="card-no-match">No items in this vendor match your filter.</div>
      </div>
    </article>`;
  }).join('');

  const catChips = `<button class="chip chip-active" data-cat="all" onclick="setCategory('all',this)">All</button>` +
    presentCats.map(c => `<button class="chip" data-cat="${esc(c)}" onclick="setCategory('${esc(c)}',this)">${esc(CAT_LABELS[c]||c)}</button>`).join('');

  const cliRows = clis.map(c => {
    const cls = c.authStatus==='authenticated' ? 'status-ok' : c.authStatus==='not authenticated' ? 'status-warn' : 'status-na';
    const txt = c.authStatus==='authenticated' ? `&#10003; ${esc(c.authUser||'authenticated')}` : c.authStatus==='not authenticated' ? '&times; not authenticated' : '&mdash;';
    const authSort = c.authStatus==='authenticated' ? '2' : c.authStatus==='not authenticated' ? '1' : '0';
    return `
    <tr class="${c.installed ? '' : 'row-missing'}" data-name="${esc(c.name.toLowerCase())}" data-version="${esc((c.version||'').toLowerCase())}" data-auth="${authSort}">
      <td class="td-name"><span class="cli-emoji">${c.emoji}</span>${esc(c.name)}</td>
      <td class="mono">${c.installed ? esc(c.version||'?') : '<span class="muted">not installed</span>'}</td>
      <td class="${cls}">${c.installed ? txt : '<span class="muted">&mdash;</span>'}</td>
    </tr>`;
  }).join('');

  const rateLimitHtml = rateLimit ? `
  <div class="rate-grid">
    ${['core','graphql','search'].map(k => {
      const r = rateLimit[k]; if (!r) return '';
      const used = r.limit - r.remaining;
      const pct  = r.limit > 0 ? Math.round((used / r.limit) * 100) : 0;
      const clr  = pct > 80 ? '#ef4444' : pct > 50 ? '#f97316' : '#22c55e';
      return `
      <div class="rate-item">
        <div class="rate-label">${k}</div>
        <div class="rate-bar-wrap"><div class="rate-bar" style="width:${pct}%;background:${clr}"></div></div>
        <div class="rate-nums">${r.remaining.toLocaleString()} / ${r.limit.toLocaleString()} left &middot; resets ${new Date(r.reset*1000).toLocaleTimeString()}</div>
      </div>`;
    }).join('')}
  </div>` : `<div class="empty-state">Unable to fetch &mdash; ensure <code>gh auth login</code> is complete.</div>`;

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8"/>
  <meta name="viewport" content="width=device-width,initial-scale=1"/>
  <title>AI Harness Status</title>
  <link rel="preconnect" href="https://fonts.googleapis.com"/>
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin/>
  <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet"/>
  <style>
    :root{
      --bg:#080a0f; --surface:#0e1118; --surface2:#141823; --surface3:#1b2030; --surface4:#232a3d;
      --line:#222838; --line2:#2e3650; --line3:#3a425c;
      --fg:#eef1f8; --fg2:#aab2c8; --fg3:#717b94; --fg4:#48506a;
      --primary:#6d6bf6; --primary2:#9b9bff; --primary-soft:rgba(109,107,246,.14);
      --accent:#3ad0ee;
      --green:#2dd47f; --yellow:#f5c518; --orange:#fb923c; --red:#f4525f;
      --mcp:#a78bfa; --skill:#34d399; --script:#fb923c;
      --r:14px; --r2:10px; --rs:7px;
      --shadow:0 1px 2px rgba(0,0,0,.4),0 8px 24px -12px rgba(0,0,0,.6);
      --shadow-lg:0 24px 60px -20px rgba(0,0,0,.75);
      --sp:16px;
      --maxw:1440px;
    }
    *,*::before,*::after{box-sizing:border-box;margin:0;padding:0;}
    html{font-size:15px;scroll-behavior:smooth;-webkit-text-size-adjust:100%;}
    body{background:radial-gradient(1200px 600px at 70% -10%,rgba(109,107,246,.10),transparent 60%),var(--bg);color:var(--fg);font-family:'Inter',system-ui,-apple-system,sans-serif;line-height:1.55;min-height:100vh;font-feature-settings:'cv02','cv03','cv04','ss01';-webkit-font-smoothing:antialiased;}
    a{color:var(--accent);text-decoration:none;}
    a:hover{text-decoration:underline;}
    .mono{font-family:'JetBrains Mono',ui-monospace,monospace;font-size:.78rem;}
    .muted{color:var(--fg3);}
    code{font-family:'JetBrains Mono',monospace;font-size:.82em;background:var(--surface3);padding:1px 6px;border-radius:5px;}
    .hidden{display:none!important;}
    ::selection{background:var(--primary-soft);}
    *:focus-visible{outline:2px solid var(--primary);outline-offset:2px;border-radius:4px;}
    /* scrollbar */
    *::-webkit-scrollbar{width:11px;height:11px;}
    *::-webkit-scrollbar-thumb{background:var(--surface4);border-radius:8px;border:3px solid var(--bg);}
    *::-webkit-scrollbar-thumb:hover{background:var(--line2);}

    /* ── App bar ── */
    .appbar{position:sticky;top:0;z-index:50;background:rgba(8,10,15,.82);backdrop-filter:blur(14px) saturate(140%);border-bottom:1px solid var(--line);}
    .appbar-inner{max-width:var(--maxw);margin:0 auto;display:flex;align-items:center;gap:20px;padding:16px 32px;flex-wrap:wrap;}
    .brand{display:flex;align-items:center;gap:12px;}
    .brand-logo{width:38px;height:38px;border-radius:11px;display:grid;place-items:center;font-size:1.3rem;background:linear-gradient(140deg,var(--primary),#7c3aed);box-shadow:0 6px 18px -6px rgba(109,107,246,.7);}
    .brand h1{font-size:1.12rem;font-weight:800;letter-spacing:-.02em;line-height:1.1;}
    .brand .sub{font-size:.7rem;color:var(--fg3);font-weight:500;}
    .metrics{margin-left:auto;display:flex;gap:10px;flex-wrap:wrap;}
    .metric{background:var(--surface2);border:1px solid var(--line);border-radius:12px;padding:8px 16px;min-width:78px;text-align:center;transition:border-color .2s,transform .2s;}
    .metric:hover{border-color:var(--line2);transform:translateY(-1px);}
    .metric .n{font-size:1.32rem;font-weight:700;letter-spacing:-.02em;color:var(--fg);font-variant-numeric:tabular-nums;}
    .metric .n .sep{color:var(--fg4);font-weight:500;}
    .metric .l{font-size:.62rem;color:var(--fg3);text-transform:uppercase;letter-spacing:.08em;font-weight:600;margin-top:1px;}

    /* ── Toolbar ── */
    .toolbar{position:sticky;top:71px;z-index:40;background:rgba(8,10,15,.82);backdrop-filter:blur(14px) saturate(140%);border-bottom:1px solid var(--line);}
    .toolbar-inner{max-width:var(--maxw);margin:0 auto;display:flex;align-items:center;gap:12px;padding:12px 32px;flex-wrap:wrap;}
    .search-wrap{position:relative;flex:1;min-width:240px;}
    .search-wrap .ico{position:absolute;left:14px;top:50%;transform:translateY(-50%);color:var(--fg3);font-size:.9rem;pointer-events:none;}
    .search-input{width:100%;background:var(--surface2);border:1px solid var(--line);border-radius:10px;padding:10px 14px 10px 38px;color:var(--fg);font-size:.88rem;font-family:inherit;outline:none;transition:border-color .2s,box-shadow .2s,background .2s;}
    .search-input:focus{border-color:var(--primary);box-shadow:0 0 0 3px var(--primary-soft);background:var(--surface);}
    .search-input::placeholder{color:var(--fg3);}
    .chips{display:flex;gap:6px;flex-wrap:wrap;}
    .chip{background:var(--surface2);border:1px solid var(--line);color:var(--fg2);font-size:.76rem;font-weight:600;padding:8px 13px;border-radius:9px;cursor:pointer;transition:all .15s;font-family:inherit;white-space:nowrap;}
    .chip:hover{border-color:var(--line2);color:var(--fg);}
    .chip-active{background:var(--primary-soft);border-color:var(--primary);color:var(--primary2);}
    .tb-group{display:flex;gap:8px;align-items:center;}
    .select-wrap{position:relative;}
    .sort-select{appearance:none;background:var(--surface2);border:1px solid var(--line);color:var(--fg2);font-size:.78rem;font-weight:600;font-family:inherit;padding:9px 30px 9px 13px;border-radius:9px;cursor:pointer;outline:none;transition:border-color .15s;}
    .sort-select:hover{border-color:var(--line2);}
    .select-wrap::after{content:'';position:absolute;right:12px;top:50%;width:6px;height:6px;border-right:2px solid var(--fg3);border-bottom:2px solid var(--fg3);transform:translateY(-70%) rotate(45deg);pointer-events:none;}
    .icon-btn{background:var(--surface2);border:1px solid var(--line);color:var(--fg2);width:38px;height:38px;border-radius:9px;cursor:pointer;font-size:.95rem;display:inline-grid;place-items:center;transition:all .15s;}
    .icon-btn:hover{border-color:var(--primary);color:var(--primary2);}
    .icon-btn.active{background:var(--primary-soft);border-color:var(--primary);color:var(--primary2);}
    .txt-btn{background:var(--surface2);border:1px solid var(--line);color:var(--fg2);font-size:.76rem;font-weight:600;padding:9px 13px;border-radius:9px;cursor:pointer;font-family:inherit;white-space:nowrap;transition:all .15s;}
    .txt-btn:hover{border-color:var(--primary);color:var(--primary2);}

    /* ── Layout ── */
    .main{max-width:var(--maxw);margin:0 auto;padding:26px 32px 10px;}
    .section-head{display:flex;align-items:center;gap:12px;margin:30px 0 16px;}
    .section-head:first-child{margin-top:6px;}
    .section-head h2{font-size:.74rem;font-weight:700;text-transform:uppercase;letter-spacing:.1em;color:var(--fg3);}
    .section-head .rule{flex:1;height:1px;background:linear-gradient(90deg,var(--line),transparent);}
    .section-head .count-note{font-size:.74rem;color:var(--fg3);font-variant-numeric:tabular-nums;}

    /* ── Vendor cards ── */
    .vendors-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(560px,1fr));gap:16px;}
    .vendor-card{background:linear-gradient(180deg,var(--surface),var(--surface2));border:1px solid var(--line);border-radius:var(--r);box-shadow:var(--shadow);transition:border-color .2s,box-shadow .2s,transform .2s;overflow:hidden;}
    .vendor-card:hover{border-color:var(--line2);box-shadow:var(--shadow-lg);}
    .vendor-card.card-hidden{display:none;}
    .vendor-card.off{opacity:.66;background:var(--surface);}
    .vendor-card.off:hover{opacity:1;}
    .vendor-header{display:flex;align-items:center;gap:13px;padding:16px 18px;cursor:pointer;user-select:none;transition:background .15s;}
    .vendor-header:hover{background:rgba(255,255,255,.018);}
    .status-dot{width:9px;height:9px;border-radius:50%;flex-shrink:0;}
    .dot-ok{background:var(--green);box-shadow:0 0 0 3px rgba(45,212,127,.16);}
    .dot-warn{background:var(--yellow);box-shadow:0 0 0 3px rgba(245,197,24,.16);}
    .dot-off{background:var(--fg4);}
    .vendor-emoji{font-size:1.6rem;line-height:1;filter:saturate(1.1);}
    .vendor-info{min-width:0;}
    .vendor-info h2{font-size:.98rem;font-weight:700;letter-spacing:-.01em;display:flex;align-items:center;gap:8px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}
    .cat-tag{font-size:.58rem;font-weight:700;text-transform:uppercase;letter-spacing:.06em;color:var(--fg3);background:var(--surface3);border:1px solid var(--line);padding:2px 7px;border-radius:6px;}
    .vendor-model{font-size:.72rem;color:var(--fg3);white-space:nowrap;}
    .vendor-counts{margin-left:auto;display:flex;gap:6px;align-items:center;flex-shrink:0;}
    .count-badge{font-size:.66rem;font-weight:700;padding:3px 10px;border-radius:20px;letter-spacing:.01em;}
    .count-mcp{background:rgba(167,139,250,.14);color:var(--mcp);border:1px solid rgba(167,139,250,.28);}
    .count-skill{background:rgba(52,211,153,.13);color:var(--skill);border:1px solid rgba(52,211,153,.26);}
    .cap-tag{font-size:.6rem;font-weight:700;color:var(--fg3);background:var(--surface3);border:1px solid var(--line);padding:3px 8px;border-radius:6px;text-transform:uppercase;letter-spacing:.05em;}
    .caret{font-size:1.3rem;color:var(--fg3);margin-left:6px;transition:transform .22s cubic-bezier(.4,0,.2,1);flex-shrink:0;transform:rotate(90deg);line-height:1;}
    .caret.collapsed{transform:rotate(0deg);}
    .vendor-body{padding:2px 18px 18px;border-top:1px solid var(--line);}
    .vendor-body.collapsed{display:none;}

    .config-path{font-size:.7rem;color:var(--fg2);margin:10px 0 3px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;display:flex;align-items:center;gap:7px;}
    .cfg-ico{flex-shrink:0;}
    .copyable{cursor:pointer;transition:color .15s;}
    .copyable:hover{color:var(--accent);}
    .copy-hint{font-size:.6rem;color:var(--fg4);opacity:0;transition:opacity .15s;margin-left:auto;border:1px solid var(--line);padding:1px 6px;border-radius:5px;}
    .copyable:hover .copy-hint{opacity:1;}

    /* ── Budget bars ── */
    .budget-row{display:flex;align-items:center;gap:11px;margin:9px 0;}
    .budget-label{font-size:.68rem;color:var(--fg3);white-space:nowrap;min-width:86px;font-weight:500;}
    .budget-bar-wrap{flex:1;height:6px;background:var(--surface3);border-radius:4px;overflow:hidden;}
    .budget-bar{height:100%;border-radius:4px;transition:width .6s cubic-bezier(.4,0,.2,1);}
    .budget-val{font-size:.69rem;font-weight:600;white-space:nowrap;min-width:200px;text-align:right;font-variant-numeric:tabular-nums;}

    /* ── Card toolbar ── */
    .card-toolbar{margin:14px 0 4px;}
    .card-filter{width:100%;background:var(--surface);border:1px solid var(--line);border-radius:8px;padding:7px 12px;color:var(--fg);font-size:.78rem;font-family:inherit;outline:none;transition:border-color .15s,box-shadow .15s;}
    .card-filter:focus{border-color:var(--primary);box-shadow:0 0 0 3px var(--primary-soft);}
    .card-filter::placeholder{color:var(--fg4);}
    .card-no-match{display:none;color:var(--fg3);font-size:.78rem;padding:10px 2px;}
    .card-no-match.show{display:block;}

    /* ── Tables ── */
    .table-section{margin-top:14px;}
    .table-title{font-size:.66rem;font-weight:700;text-transform:uppercase;letter-spacing:.07em;color:var(--fg3);margin-bottom:6px;display:flex;align-items:center;gap:8px;}
    .tt-count{background:var(--surface3);color:var(--fg2);font-size:.62rem;padding:1px 7px;border-radius:10px;font-weight:700;}
    .item-table{width:100%;border-collapse:separate;border-spacing:0;font-size:.82rem;}
    .item-table th{font-size:.64rem;font-weight:700;color:var(--fg3);text-transform:uppercase;letter-spacing:.05em;padding:7px 11px;border-bottom:1px solid var(--line2);text-align:left;background:var(--surface);position:sticky;top:0;}
    .item-table th.sortable{cursor:pointer;user-select:none;transition:color .15s;white-space:nowrap;}
    .item-table th.sortable:hover{color:var(--fg);}
    .item-table th.sortable::after{content:'\\2195';opacity:.32;margin-left:5px;font-size:.85em;}
    .item-table th.sort-asc::after{content:'\\2191';opacity:1;color:var(--primary2);}
    .item-table th.sort-desc::after{content:'\\2193';opacity:1;color:var(--primary2);}
    .th-actions{width:62px;}
    .item-table td{padding:8px 11px;border-bottom:1px solid var(--line);vertical-align:middle;}
    .item-table tbody tr:last-child>td{border-bottom:none;}
    .item-table tbody tr:hover>td{background:rgba(255,255,255,.022);}
    .td-name{font-weight:500;display:flex;align-items:center;gap:8px;flex-wrap:wrap;}
    .td-name .nm{overflow-wrap:anywhere;}
    .td-cmd{max-width:210px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;color:var(--fg2);}
    .td-desc{max-width:250px;color:var(--fg2);font-size:.78rem;}
    .td-size{color:var(--fg3);font-size:.77rem;white-space:nowrap;}
    .desc-size{font-size:.66rem;display:block;}
    .td-env{display:flex;gap:4px;flex-wrap:wrap;max-width:160px;}
    .td-actions{width:62px;text-align:right;white-space:nowrap;}
    .badge{font-size:.58rem;font-weight:800;padding:2px 6px;border-radius:5px;letter-spacing:.04em;text-transform:uppercase;white-space:nowrap;flex-shrink:0;}
    .badge-mcp{background:rgba(167,139,250,.18);color:var(--mcp);}
    .badge-skill{background:rgba(52,211,153,.18);color:var(--skill);}
    .badge-script{background:rgba(251,146,60,.18);color:var(--script);}
    .env-tag{font-size:.63rem;background:var(--surface3);color:var(--fg2);padding:2px 6px;border-radius:5px;font-family:'JetBrains Mono',monospace;border:1px solid var(--line);}
    .btn-icon{background:transparent;border:1px solid var(--line);color:var(--fg3);border-radius:7px;width:25px;height:25px;cursor:pointer;font-size:.82rem;transition:all .15s;display:inline-flex;align-items:center;justify-content:center;margin-left:4px;}
    .btn-edit:hover{background:var(--primary-soft);border-color:var(--primary);color:var(--primary2);}
    .btn-del:hover{background:rgba(244,82,95,.16);border-color:var(--red);color:var(--red);}
    .chip-scripts{font-size:.62rem;background:var(--surface3);color:var(--script);border:1px solid rgba(251,146,60,.3);border-radius:6px;padding:1px 8px;cursor:pointer;transition:background .15s;font-family:inherit;}
    .chip-scripts:hover{background:rgba(251,146,60,.15);}
    .ro-lock{color:var(--fg3);font-size:.82rem;cursor:help;}
    .ro-note{font-size:.58rem;font-weight:700;color:var(--orange);background:rgba(251,146,60,.13);padding:1px 7px;border-radius:5px;letter-spacing:.02em;text-transform:none;}
    .script-row td{background:rgba(255,255,255,.02)!important;font-size:.78rem;}
    .empty-state{color:var(--fg3);font-size:.82rem;padding:12px 2px;}

    /* ── Table: numeric align, checkboxes, scroll, footer ── */
    .th-num,.td-num{text-align:right;}
    .td-env.td-num{justify-content:flex-end;}
    .th-check,.td-check{width:34px;text-align:center;padding-left:12px!important;padding-right:4px!important;}
    .item-table input[type=checkbox]{appearance:none;-webkit-appearance:none;width:15px;height:15px;border:1.5px solid var(--line2);border-radius:4px;background:var(--surface);cursor:pointer;vertical-align:middle;position:relative;transition:all .15s;flex-shrink:0;}
    .item-table input[type=checkbox]:hover{border-color:var(--primary);}
    .item-table input[type=checkbox]:checked{background:var(--primary);border-color:var(--primary);}
    .item-table input[type=checkbox]:checked::after{content:'';position:absolute;left:4px;top:1px;width:4px;height:8px;border:solid #fff;border-width:0 2px 2px 0;transform:rotate(45deg);}
    .item-table input[type=checkbox]:indeterminate{background:var(--primary);border-color:var(--primary);}
    .item-table input[type=checkbox]:indeterminate::after{content:'';position:absolute;left:3px;top:6px;width:7px;height:2px;background:#fff;}
    tr.sel-on>td{background:var(--primary-soft)!important;}
    .table-scroll{max-height:360px;overflow:auto;border:1px solid var(--line);border-radius:9px;}
    .item-table thead th{position:sticky;top:0;z-index:2;}
    .item-table tfoot td{position:sticky;bottom:0;background:var(--surface);border-top:1px solid var(--line2);border-bottom:none;font-size:.66rem;font-weight:600;color:var(--fg3);text-transform:uppercase;letter-spacing:.05em;padding:7px 11px;}
    .bulk-bar{display:none;align-items:center;gap:12px;background:var(--primary-soft);border:1px solid var(--primary);border-radius:9px;padding:7px 12px;margin-bottom:7px;animation:fade .15s ease;}
    .bulk-bar.show{display:flex;}
    .bulk-count{font-size:.74rem;font-weight:700;color:var(--primary2);}
    .bulk-actions{margin-left:auto;display:flex;gap:7px;}
    .btn-bulk-del{background:rgba(244,82,95,.16);border:1px solid rgba(244,82,95,.4);color:#ff8a93;font-size:.72rem;font-weight:600;padding:5px 12px;border-radius:7px;cursor:pointer;font-family:inherit;transition:all .15s;}
    .btn-bulk-del:hover{background:rgba(244,82,95,.3);}
    .btn-bulk-clear{background:var(--surface3);border:1px solid var(--line2);color:var(--fg2);font-size:.72rem;font-weight:600;padding:5px 12px;border-radius:7px;cursor:pointer;font-family:inherit;transition:all .15s;}
    .btn-bulk-clear:hover{border-color:var(--fg3);color:var(--fg);}

    /* ── CLI + rate cards ── */
    .panel{background:linear-gradient(180deg,var(--surface),var(--surface2));border:1px solid var(--line);border-radius:var(--r);box-shadow:var(--shadow);overflow:hidden;}
    .cli-table{width:100%;border-collapse:separate;border-spacing:0;font-size:.84rem;}
    .cli-table th{font-size:.64rem;font-weight:700;color:var(--fg3);text-transform:uppercase;letter-spacing:.05em;padding:11px 18px;border-bottom:1px solid var(--line2);text-align:left;background:var(--surface);}
    .cli-table th.sortable{cursor:pointer;user-select:none;white-space:nowrap;}
    .cli-table th.sortable:hover{color:var(--fg);}
    .cli-table th.sortable::after{content:'\\2195';opacity:.32;margin-left:5px;font-size:.85em;}
    .cli-table th.sort-asc::after{content:'\\2191';opacity:1;color:var(--primary2);}
    .cli-table th.sort-desc::after{content:'\\2193';opacity:1;color:var(--primary2);}
    .cli-table td{padding:11px 18px;border-bottom:1px solid var(--line);}
    .cli-table .td-name{display:flex;align-items:center;gap:9px;font-weight:500;}
    .cli-emoji{font-size:1.1rem;}
    .cli-table tbody tr:last-child td{border-bottom:none;}
    .cli-table tbody tr:hover td{background:rgba(255,255,255,.022);}
    .row-missing{opacity:.42;}
    .status-ok{color:var(--green);font-weight:600;}
    .status-warn{color:var(--yellow);font-weight:600;}
    .status-na{color:var(--fg3);}
    .rate-panel{padding:18px 20px;}
    .rate-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:16px;}
    .rate-label{font-size:.78rem;font-weight:700;color:var(--fg2);margin-bottom:6px;text-transform:capitalize;}
    .rate-bar-wrap{height:6px;background:var(--surface3);border-radius:4px;overflow:hidden;margin-bottom:6px;}
    .rate-bar{height:100%;border-radius:4px;transition:width .6s ease;}
    .rate-nums{font-size:.7rem;color:var(--fg3);font-variant-numeric:tabular-nums;}

    .no-results{display:none;text-align:center;padding:56px 20px;color:var(--fg3);}
    .no-results.show{display:block;}
    .no-results .nr-emoji{font-size:2.4rem;display:block;margin-bottom:10px;opacity:.7;}

    /* ── Footer ── */
    .footer{text-align:center;color:var(--fg4);font-size:.7rem;padding:28px 0 40px;}
    .footer a{color:var(--fg3);}

    /* ── Density (compact) ── */
    body.dense .vendor-header{padding:11px 16px;}
    body.dense .vendor-body{padding:2px 16px 14px;}
    body.dense .item-table td{padding:5px 11px;}
    body.dense .budget-row{margin:6px 0;}
    body.dense .vendors-grid{gap:12px;}
    body.dense .metric{padding:6px 13px;}

    /* ── Modals ── */
    .overlay{display:none;position:fixed;inset:0;background:rgba(4,6,11,.7);backdrop-filter:blur(6px);z-index:1000;align-items:center;justify-content:center;padding:20px;}
    .overlay.open{display:flex;animation:fade .15s ease;}
    @keyframes fade{from{opacity:0}to{opacity:1}}
    .modal{background:var(--surface2);border:1px solid var(--line2);border-radius:16px;padding:26px;max-width:520px;width:100%;box-shadow:var(--shadow-lg);animation:pop .18s cubic-bezier(.34,1.4,.5,1);}
    @keyframes pop{from{opacity:0;transform:scale(.96) translateY(8px)}to{opacity:1;transform:none}}
    .modal h3{font-size:1.05rem;font-weight:700;margin-bottom:9px;}
    .modal p{color:var(--fg2);font-size:.86rem;margin-bottom:14px;}
    .modal-path{font-size:.72rem;color:var(--fg3);word-break:break-all;margin-bottom:18px;background:var(--surface3);padding:9px 13px;border-radius:8px;font-family:'JetBrains Mono',monospace;border:1px solid var(--line);}
    .modal-btns{display:flex;gap:9px;justify-content:flex-end;}
    .btn{padding:8px 17px;border-radius:9px;font-size:.84rem;font-weight:600;cursor:pointer;border:1px solid var(--line);transition:all .15s;font-family:inherit;}
    .btn-cancel{background:var(--surface3);color:var(--fg2);}
    .btn-cancel:hover{background:var(--surface4);}
    .btn-danger{background:rgba(244,82,95,.16);color:#ff8a93;border-color:rgba(244,82,95,.4);}
    .btn-danger:hover{background:rgba(244,82,95,.28);}
    .btn-primary{background:var(--primary);color:#fff;border-color:var(--primary);}
    .btn-primary:hover{background:var(--primary2);}
    .edit-modal{max-width:620px;}
    .form-group{margin-bottom:14px;}
    .form-label{font-size:.7rem;font-weight:700;color:var(--fg2);margin-bottom:6px;display:block;text-transform:uppercase;letter-spacing:.04em;}
    .form-input,.form-textarea{width:100%;background:var(--surface3);border:1px solid var(--line2);border-radius:8px;padding:9px 13px;color:var(--fg);font-size:.84rem;font-family:'JetBrains Mono',monospace;outline:none;transition:border-color .2s,box-shadow .2s;}
    .form-input:focus,.form-textarea:focus{border-color:var(--primary);box-shadow:0 0 0 3px var(--primary-soft);}
    .form-input[readonly]{opacity:.6;cursor:not-allowed;}
    .form-textarea{resize:vertical;min-height:72px;}
    .form-hint{font-size:.68rem;color:var(--fg3);margin-top:5px;}
    .env-editor{display:flex;flex-direction:column;gap:7px;}
    .env-row{display:flex;gap:7px;align-items:center;}
    .env-row input{flex:1;background:var(--surface3);border:1px solid var(--line2);border-radius:7px;padding:6px 10px;color:var(--fg);font-size:.78rem;font-family:'JetBrains Mono',monospace;outline:none;}
    .env-row input:focus{border-color:var(--primary);}
    .btn-env-add{background:var(--surface3);border:1px dashed var(--line2);color:var(--fg3);border-radius:7px;padding:5px 13px;font-size:.76rem;cursor:pointer;transition:all .15s;font-family:inherit;margin-top:5px;}
    .btn-env-add:hover{border-color:var(--primary);color:var(--primary2);}

    /* ── Toast ── */
    .toast{position:fixed;bottom:26px;right:26px;background:var(--surface3);border:1px solid var(--line2);border-radius:11px;padding:12px 20px;font-size:.84rem;font-weight:500;z-index:2000;opacity:0;transform:translateY(10px);transition:opacity .25s,transform .25s;pointer-events:none;box-shadow:var(--shadow-lg);}
    .toast.show{opacity:1;transform:none;}
    .toast-ok{border-color:var(--green);color:var(--green);}
    .toast-err{border-color:var(--red);color:#ff8a93;}
    .toast-info{border-color:var(--accent);color:var(--accent);}

    @media(max-width:880px){
      .appbar-inner,.toolbar-inner,.main{padding-left:16px;padding-right:16px;}
      .vendors-grid{grid-template-columns:1fr;}
      .metrics{width:100%;margin-left:0;}
      .toolbar{top:0;position:relative;}
      .appbar{position:relative;}
      .search-wrap{min-width:100%;}
    }
  </style>
</head>
<body>

<header class="appbar">
  <div class="appbar-inner">
    <div class="brand">
      <div class="brand-logo">&#129520;</div>
      <div>
        <h1>AI Harness Status</h1>
        <div class="sub">Generated ${esc(new Date(generatedAt).toLocaleString())}</div>
      </div>
    </div>
    <div class="metrics">
      <div class="metric"><div class="n">${configured}<span class="sep"> / </span>${vendors.length}</div><div class="l">Vendors</div></div>
      <div class="metric"><div class="n">${totalMcps}</div><div class="l">MCP Servers</div></div>
      <div class="metric"><div class="n">${totalSkills}</div><div class="l">Skills</div></div>
      <div class="metric"><div class="n">~${fmtTok(totalSkillTokens)}</div><div class="l">Skill Tokens</div></div>
      <div class="metric"><div class="n">${authClis}<span class="sep"> / </span>${instClis}</div><div class="l">CLIs Auth'd</div></div>
    </div>
  </div>
</header>

<div class="toolbar">
  <div class="toolbar-inner">
    <div class="search-wrap">
      <span class="ico">&#128269;</span>
      <input class="search-input" id="search" type="text" placeholder="Search vendors, MCPs, skills&hellip;  (press / )" oninput="onSearch(this.value)" autocomplete="off" aria-label="Global search"/>
    </div>
    <div class="chips" id="chips">${catChips}</div>
    <div class="tb-group">
      <div class="select-wrap">
        <select class="sort-select" id="sort" onchange="setSort(this.value)" aria-label="Sort vendors by">
          <option value="name">Sort: Name</option>
          <option value="mcps">Sort: MCP count</option>
          <option value="skills">Sort: Skill count</option>
          <option value="ctx">Sort: Context %</option>
          <option value="category">Sort: Category</option>
        </select>
      </div>
      <button class="icon-btn" id="dir-btn" onclick="toggleDir()" title="Toggle sort direction" aria-label="Toggle sort direction">&#8595;</button>
      <button class="icon-btn" id="dense-btn" onclick="toggleDense()" title="Toggle compact density" aria-label="Toggle density">&#8801;</button>
      <button class="txt-btn" onclick="setAllCards(false)" title="Expand all">Expand</button>
      <button class="txt-btn" onclick="setAllCards(true)" title="Collapse all">Collapse</button>
    </div>
  </div>
</div>

<div class="main">
  <div class="section-head">
    <h2>Vendors &amp; Agent Tooling</h2>
    <div class="rule"></div>
    <span class="count-note" id="count-note"></span>
  </div>
  <div class="vendors-grid" id="vendors-grid">
    ${vendorCards || '<div class="empty-state">No vendors found on this machine.</div>'}
  </div>
  <div class="no-results" id="no-results"><span class="nr-emoji">&#128270;</span>No vendors, MCPs, or skills match your filters.</div>

  <div class="section-head">
    <h2>CLI Tools &amp; Authentication</h2>
    <div class="rule"></div>
    <span class="count-note">${instClis} of ${clis.length} installed</span>
  </div>
  <div class="panel">
    <table class="cli-table">
      <thead><tr>
        <th class="sortable" data-key="name" onclick="sortTable(this)">Tool</th>
        <th class="sortable" data-key="version" onclick="sortTable(this)">Version</th>
        <th class="sortable" data-key="auth" onclick="sortTable(this)">Auth Status</th>
      </tr></thead>
      <tbody>${cliRows}</tbody>
    </table>
  </div>

  <div class="section-head">
    <h2>GitHub API Rate Limits</h2>
    <div class="rule"></div>
  </div>
  <div class="panel rate-panel">
    ${rateLimitHtml}
  </div>

  <div class="footer">
    octocode-harness-status &middot; Refresh to rescan &middot;
    <a href="https://github.com/bgauryy/octocode-mcp" target="_blank" rel="noopener">octocode-mcp</a>
  </div>
</div>

<!-- Confirm / delete modal -->
<div class="overlay" id="modal-confirm">
  <div class="modal" role="dialog" aria-modal="true">
    <h3 id="mc-title">Confirm removal</h3>
    <p id="mc-body"></p>
    <div class="modal-path" id="mc-path"></div>
    <div class="modal-btns">
      <button class="btn btn-cancel" onclick="closeModal('modal-confirm')">Cancel</button>
      <button class="btn btn-danger" id="mc-ok">Remove</button>
    </div>
  </div>
</div>

<!-- Edit MCP modal -->
<div class="overlay" id="modal-edit">
  <div class="modal edit-modal" role="dialog" aria-modal="true">
    <h3>Edit MCP Server</h3>
    <div class="form-group">
      <label class="form-label">Name (read-only)</label>
      <input class="form-input" id="edit-name" readonly/>
    </div>
    <div class="form-group">
      <label class="form-label">Command</label>
      <input class="form-input" id="edit-command" placeholder="npx, node, python&hellip;"/>
    </div>
    <div class="form-group">
      <label class="form-label">Args (one per line)</label>
      <textarea class="form-textarea" id="edit-args" rows="3" placeholder="octocode-mcp@latest"></textarea>
      <div class="form-hint">Each line becomes one argument. Other config fields (url, headers, &hellip;) are preserved.</div>
    </div>
    <div class="form-group">
      <label class="form-label">Type</label>
      <input class="form-input" id="edit-type" placeholder="stdio"/>
    </div>
    <div class="form-group">
      <label class="form-label">Environment Variables</label>
      <div class="env-editor" id="env-editor"></div>
      <button class="btn-env-add" onclick="addEnvRow()">+ Add variable</button>
    </div>
    <div class="modal-btns" style="margin-top:18px;">
      <button class="btn btn-cancel" onclick="closeModal('modal-edit')">Cancel</button>
      <button class="btn btn-primary" onclick="saveEditMcp()">Save changes</button>
    </div>
  </div>
</div>

<div class="toast" id="toast"></div>

<script>
  const VENDOR_META = ${vendorMeta};
  function getVendor(id){ return VENDOR_META.find(v => v.id === id); }
  const cssEsc = s => (window.CSS && CSS.escape) ? CSS.escape(s) : String(s).replace(/[^a-zA-Z0-9_-]/g,'\\\\$&');

  // ── View state ──
  const ST = { q:'', cat:'all', sort:'name', dir:1, dense:false };

  function onSearch(v){ ST.q = v.toLowerCase().trim(); applyView(); }
  function setCategory(cat, el){
    ST.cat = cat;
    document.querySelectorAll('#chips .chip').forEach(c => c.classList.toggle('chip-active', c === el));
    applyView();
  }
  function setSort(v){ ST.sort = v; applyView(); }
  function toggleDir(){
    ST.dir *= -1;
    document.getElementById('dir-btn').innerHTML = ST.dir === 1 ? '&#8595;' : '&#8593;';
    applyView();
  }
  function toggleDense(){
    ST.dense = !ST.dense;
    document.body.classList.toggle('dense', ST.dense);
    document.getElementById('dense-btn').classList.toggle('active', ST.dense);
  }

  function applyView(){
    const grid = document.getElementById('vendors-grid');
    const cards = [...grid.querySelectorAll('.vendor-card')];
    let visible = 0;
    cards.forEach(card => {
      const matchCat = ST.cat === 'all' || card.dataset.category === ST.cat;
      const matchQ = !ST.q || card.textContent.toLowerCase().includes(ST.q);
      const show = matchCat && matchQ;
      card.classList.toggle('card-hidden', !show);
      if (show) visible++;
    });
    // sort (stable)
    const key = ST.sort, dir = ST.dir;
    const val = c => {
      if (key === 'name') return c.dataset.name;
      if (key === 'category') return c.dataset.category + '\\u0000' + c.dataset.name;
      return parseFloat(c.dataset[key] || '0');
    };
    cards.sort((a,b) => {
      const x = val(a), y = val(b);
      if (typeof x === 'number') return (x - y) * dir;
      return x < y ? -dir : x > y ? dir : 0;
    });
    cards.forEach(c => grid.appendChild(c));
    document.getElementById('no-results').classList.toggle('show', visible === 0);
    document.getElementById('count-note').textContent = 'Showing ' + visible + ' of ' + cards.length;
  }

  // ── Card expand/collapse ──
  function toggleCard(id){
    const body = document.getElementById('body-' + id);
    const caret = document.getElementById('caret-' + id);
    const header = body && body.previousElementSibling;
    if (!body) return;
    const collapsed = body.classList.toggle('collapsed');
    if (caret) caret.classList.toggle('collapsed', collapsed);
    if (header) header.setAttribute('aria-expanded', String(!collapsed));
  }
  function setAllCards(collapsed){
    document.querySelectorAll('.vendor-card:not(.card-hidden) .vendor-body').forEach(b => b.classList.toggle('collapsed', collapsed));
    document.querySelectorAll('.vendor-card:not(.card-hidden) .caret').forEach(c => c.classList.toggle('collapsed', collapsed));
    document.querySelectorAll('.vendor-card:not(.card-hidden) .vendor-header').forEach(h => h.setAttribute('aria-expanded', String(!collapsed)));
  }
  function toggleScripts(key){
    document.querySelectorAll('.script-row[data-scripts="' + cssEsc(key) + '"]').forEach(r => r.classList.toggle('hidden'));
  }

  // ── Per-vendor item filter ──
  function filterItems(input){
    const card = input.closest('.vendor-card');
    const q = input.value.toLowerCase().trim();
    let visible = 0;
    card.querySelectorAll('tr.mcp-row, tr.skill-row').forEach(row => {
      const match = !q || row.textContent.toLowerCase().includes(q);
      row.classList.toggle('hidden', !match);
      if (match) visible++;
      if (row.classList.contains('skill-row') && !match) {
        const gk = row.dataset.group;
        card.querySelectorAll('.script-row[data-scripts="' + cssEsc(gk) + '"]').forEach(sr => sr.classList.add('hidden'));
      }
    });
    card.querySelectorAll('.table-section').forEach(sec => {
      const rows = sec.querySelectorAll('tr.mcp-row, tr.skill-row');
      const any = [...rows].some(r => !r.classList.contains('hidden'));
      sec.classList.toggle('hidden', rows.length > 0 && !any);
    });
    const nm = card.querySelector('.card-no-match');
    if (nm) nm.classList.toggle('show', !!q && visible === 0);
  }

  // ── Sortable tables (group-aware for skills) ──
  function sortTable(th){
    const table = th.closest('table');
    const tbody = table.tBodies[0];
    const key = th.dataset.key;
    let dir = th.classList.contains('sort-asc') ? -1 : 1;
    table.querySelectorAll('th.sortable').forEach(h => h.classList.remove('sort-asc','sort-desc'));
    th.classList.add(dir === 1 ? 'sort-asc' : 'sort-desc');
    th.setAttribute('aria-sort', dir === 1 ? 'ascending' : 'descending');
    const rows = [...tbody.children];
    const groups = []; let cur = null;
    rows.forEach(r => {
      if (r.classList.contains('script-row') && cur) cur.extra.push(r);
      else { cur = { head: r, extra: [] }; groups.push(cur); }
    });
    const val = r => {
      const d = r.dataset[key];
      if (d === undefined) return '';
      const n = parseFloat(d);
      return (d !== '' && !isNaN(n) && String(n) === d) ? n : d.toLowerCase();
    };
    groups.sort((a,b) => {
      const x = val(a.head), y = val(b.head);
      if (x < y) return -dir; if (x > y) return dir; return 0;
    });
    groups.forEach(g => { tbody.appendChild(g.head); g.extra.forEach(e => tbody.appendChild(e)); });
  }

  // ── Bulk selection ──
  let pendingBulk = null;
  function onSel(cb){
    cb.closest('tr').classList.toggle('sel-on', cb.checked);
    updateBulk(cb.closest('.table-section'));
  }
  function toggleAll(cb){
    const sec = cb.closest('.table-section');
    sec.querySelectorAll('tbody tr:not(.hidden) .row-sel').forEach(x => { x.checked = cb.checked; x.closest('tr').classList.toggle('sel-on', cb.checked); });
    updateBulk(sec);
  }
  function clearSel(btn){
    const sec = btn.closest('.table-section');
    sec.querySelectorAll('.row-sel').forEach(x => { x.checked = false; x.closest('tr').classList.remove('sel-on'); });
    const all = sec.querySelector('.sel-all'); if (all) { all.checked = false; all.indeterminate = false; }
    updateBulk(sec);
  }
  function updateBulk(sec){
    const checked = sec.querySelectorAll('.row-sel:checked').length;
    const total = sec.querySelectorAll('.row-sel').length;
    const bar = sec.querySelector('.bulk-bar');
    if (bar) { bar.classList.toggle('show', checked > 0); const c = bar.querySelector('.bulk-count'); if (c) c.textContent = checked + ' selected'; }
    const all = sec.querySelector('.sel-all');
    if (all) { all.checked = checked > 0 && checked === total; all.indeterminate = checked > 0 && checked < total; }
  }
  function bulkRemove(btn){
    const sec = btn.closest('.table-section');
    const type = sec.dataset.table, vendor = sec.dataset.vendor;
    const rows = [...sec.querySelectorAll('.row-sel:checked')].map(cb => cb.closest('tr'));
    if (!rows.length) return;
    const names = rows.map(r => r.dataset.item);
    pendingBulk = { type, vendor, rows };
    const label = (type === 'mcp' ? 'MCP server' : 'skill') + (rows.length > 1 ? 's' : '');
    document.getElementById('mc-title').textContent = 'Remove ' + rows.length + ' ' + label;
    document.getElementById('mc-body').textContent = 'Remove the selected ' + label + ' from ' + vendor + '? This edits files on disk and cannot be undone.';
    document.getElementById('mc-path').textContent = names.join('  ·  ');
    document.getElementById('mc-ok').textContent = 'Remove ' + rows.length;
    document.getElementById('mc-ok').onclick = executeBulkRemove;
    openModal('modal-confirm');
  }
  async function executeBulkRemove(){
    closeModal('modal-confirm');
    const b = pendingBulk; if (!b) return;
    let ok = 0, fail = 0;
    for (const row of b.rows) {
      const name = row.dataset.item;
      const res = await apiPost('/api/remove', { type: b.type, vendorId: b.vendor, itemName: name });
      if (res.ok) {
        ok++;
        if (b.type === 'skill') {
          const key = b.vendor + '-' + name;
          document.querySelectorAll('.script-row[data-scripts="' + cssEsc(key) + '"]').forEach(r => r.remove());
        }
        row.remove();
      } else { fail++; }
    }
    showToast('Removed ' + ok + (fail ? ' · ' + fail + ' failed' : ''), fail ? 'err' : 'ok');
    const sec = document.querySelector('.table-section[data-vendor="' + cssEsc(b.vendor) + '"][data-table="' + b.type + '"]');
    if (sec) updateBulk(sec);
    pendingBulk = null;
  }

  // ── Copy ──
  function copyText(text){
    if (navigator.clipboard) navigator.clipboard.writeText(text).then(() => showToast('Copied to clipboard', 'info'));
    else showToast('Clipboard unavailable', 'err');
  }

  // ── Modals ──
  let pendingAction = null;
  function openModal(id){ document.getElementById(id).classList.add('open'); }
  function closeModal(id){ document.getElementById(id).classList.remove('open'); pendingAction = null; }
  document.querySelectorAll('.overlay').forEach(el => {
    el.addEventListener('click', e => { if (e.target === el) el.classList.remove('open'); });
  });

  function confirmRemove(type, vendorId, itemName, configPath, mcpKey){
    pendingAction = { type, vendorId, itemName, configPath, mcpKey };
    const label = type === 'mcp' ? 'MCP server' : 'skill folder';
    document.getElementById('mc-title').textContent = 'Remove ' + label;
    document.getElementById('mc-body').textContent = 'Remove "' + itemName + '" from ' + vendorId + '? This edits files on disk.';
    document.getElementById('mc-path').textContent = type === 'mcp' ? 'Config: ' + configPath : 'Folder: ' + (configPath || '') + '/' + itemName;
    document.getElementById('mc-ok').textContent = 'Remove';
    document.getElementById('mc-ok').onclick = executeRemove;
    openModal('modal-confirm');
  }
  async function executeRemove(){
    closeModal('modal-confirm');
    const a = pendingAction; if (!a) return;
    const res = await apiPost('/api/remove', a);
    if (res.ok) {
      showToast('Removed "' + a.itemName + '"', 'ok');
      if (a.type === 'mcp') {
        const row = document.getElementById('mcp-row-' + a.vendorId + '-' + a.itemName);
        if (row) row.remove();
      } else {
        const row = document.getElementById('skill-row-' + a.vendorId + '-' + a.itemName);
        const key = a.vendorId + '-' + a.itemName;
        document.querySelectorAll('.script-row[data-scripts="' + cssEsc(key) + '"]').forEach(r => r.remove());
        if (row) row.remove();
      }
    } else { showToast(res.error || 'Failed', 'err'); }
  }

  function confirmDeleteFile(filePath, fileName, btn){
    pendingAction = { type:'file', filePath, fileName, row: btn ? btn.closest('tr') : null };
    document.getElementById('mc-title').textContent = 'Delete script file';
    document.getElementById('mc-body').textContent = 'Permanently delete "' + fileName + '" from disk?';
    document.getElementById('mc-path').textContent = filePath;
    document.getElementById('mc-ok').textContent = 'Delete';
    document.getElementById('mc-ok').onclick = executeDeleteFile;
    openModal('modal-confirm');
  }
  async function executeDeleteFile(){
    closeModal('modal-confirm');
    const a = pendingAction; if (!a) return;
    const res = await apiPost('/api/delete-file', { filePath: a.filePath });
    if (res.ok) { showToast('Deleted "' + a.fileName + '"', 'ok'); if (a.row) a.row.remove(); }
    else { showToast(res.error || 'Failed', 'err'); }
  }

  // ── Edit MCP ──
  let editCtx = null;
  function openEditMcp(vendorId, serverName){
    const vendor = getVendor(vendorId); if (!vendor) return;
    const srv = vendor.mcpServers.find(m => m.name === serverName); if (!srv) return;
    editCtx = { vendorId, serverName, vendor };
    document.getElementById('edit-name').value = srv.name;
    document.getElementById('edit-command').value = srv.command || '';
    document.getElementById('edit-args').value = (srv.args || []).join('\\n');
    document.getElementById('edit-type').value = srv.type || 'stdio';
    const editor = document.getElementById('env-editor');
    editor.innerHTML = '';
    for (const [k, v] of Object.entries(srv.env || {})) addEnvRow(k, v);
    openModal('modal-edit');
  }
  function addEnvRow(key = '', val = ''){
    const editor = document.getElementById('env-editor');
    const row = document.createElement('div');
    row.className = 'env-row';
    row.innerHTML = '<input placeholder="KEY" value="' + esc(key) + '"/>' +
      '<input placeholder="value" value="' + esc(val) + '"/>' +
      '<button class="btn-icon btn-del" onclick="this.parentElement.remove()" title="Remove">&times;</button>';
    editor.appendChild(row);
  }
  function esc(s){ return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;'); }

  async function saveEditMcp(){
    if (!editCtx) return;
    const command = document.getElementById('edit-command').value.trim();
    const type = document.getElementById('edit-type').value.trim() || 'stdio';
    const args = document.getElementById('edit-args').value.split('\\n').map(l => l.trim()).filter(Boolean);
    const env = {};
    document.querySelectorAll('#env-editor .env-row').forEach(r => {
      const i = r.querySelectorAll('input');
      const k = i[0] && i[0].value.trim(), v = (i[1] && i[1].value) || '';
      if (k) env[k] = v;
    });
    const newCfg = { command, args, type };
    if (Object.keys(env).length) newCfg.env = env;
    closeModal('modal-edit');
    const res = await apiPost('/api/edit-mcp', { vendorId: editCtx.vendorId, serverName: editCtx.serverName, newCfg });
    if (res.ok) {
      showToast('Saved "' + editCtx.serverName + '"', 'ok');
      const srv = editCtx.vendor.mcpServers.find(m => m.name === editCtx.serverName);
      if (srv) { srv.command = command; srv.args = args; srv.env = env; srv.type = type; }
      const row = document.getElementById('mcp-row-' + editCtx.vendorId + '-' + editCtx.serverName);
      if (row) {
        const cmdCell = row.querySelector('.td-cmd');
        const full = command + (args.length ? ' ' + args.join(' ') : '');
        if (cmdCell) { cmdCell.textContent = full || '\\u2014'; cmdCell.title = full; }
        const envCell = row.querySelector('.td-env');
        if (envCell) {
          const keys = Object.keys(env);
          envCell.innerHTML = keys.length ? keys.map(k => '<span class="env-tag">' + esc(k) + '</span>').join('') : '<span class="muted">\\u2014</span>';
        }
        row.dataset.cmd = full.toLowerCase();
        row.dataset.env = String(Object.keys(env).length);
      }
    } else { showToast(res.error || 'Failed', 'err'); }
  }

  // ── API ──
  async function apiPost(url, body){
    try {
      const r = await fetch(url, { method:'POST', headers:{ 'Content-Type':'application/json' }, body: JSON.stringify(body) });
      return await r.json();
    } catch (e) { return { ok:false, error: e.message }; }
  }

  // ── Toast ──
  let toastTimer = null;
  function showToast(msg, type){
    const t = document.getElementById('toast');
    t.textContent = msg;
    t.className = 'toast toast-' + type + ' show';
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => { t.className = 'toast'; }, 3200);
  }

  // ── Keyboard ──
  document.addEventListener('keydown', e => {
    if (e.key === 'Escape') {
      document.querySelectorAll('.overlay.open').forEach(el => el.classList.remove('open'));
      pendingAction = null;
    } else if (e.key === '/' && !/^(INPUT|TEXTAREA|SELECT)$/.test(document.activeElement.tagName)) {
      e.preventDefault();
      document.getElementById('search').focus();
    }
  });

  applyView();
</script>
</body>
</html>`;
}

// ─── HTTP server ──────────────────────────────────────────────────────────────

async function startServer(html, vendors, port, timeout) {
  let srvPort = 0;
  // Reject cross-origin / rebound-host requests so no website you visit can
  // drive the mutating API on 127.0.0.1 while the server is alive (CSRF / DNS rebinding).
  const sameOriginOk = (req) => {
    const host = req.headers.host || '';
    const allowedHosts = [`127.0.0.1:${srvPort}`, `localhost:${srvPort}`];
    if (!allowedHosts.includes(host)) return false;
    const origin = req.headers.origin;
    if (origin && ![`http://127.0.0.1:${srvPort}`, `http://localhost:${srvPort}`].includes(origin)) return false;
    return true;
  };

  const server = createServer((req, res) => {
    // No wildcard CORS: the dashboard is served same-origin and needs none;
    // foreign-origin browser requests are blocked by the absence of ACAO headers.
    if (req.method === 'OPTIONS') { res.writeHead(204); res.end(); return; }

    if (req.method === 'GET' && req.url === '/') {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }); res.end(html); return;
    }

    if (req.method === 'GET' && req.url === '/health') {
      res.writeHead(200, { 'Content-Type': 'application/json' }); res.end('{"ok":true}'); return;
    }

    if (req.method === 'POST') {
      if (!sameOriginOk(req)) {
        res.writeHead(403, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: 'Cross-origin request rejected' }));
        return;
      }
      let body = '';
      req.on('data', d => body += d);
      req.on('end', () => {
        res.setHeader('Content-Type', 'application/json');
        try {
          const payload = JSON.parse(body);

          if (req.url === '/api/remove') {
            // Paths come from the trusted server-side vendor record, never the client payload.
            const { type, vendorId, itemName, configPath: clientParentDir } = payload;
            const vendor = vendors.find(v => v.id === vendorId);
            if (!vendor) { res.writeHead(400); res.end(JSON.stringify({ ok: false, error: 'Vendor not found' })); return; }
            if (type === 'mcp') {
              if (vendor.mcpReadOnly) throw new Error(`${(vendor.mcpConfigFormat||'').toUpperCase()} config is read-only`);
              removeMcpFromConfig(vendor.mcpConfigPath, vendor.mcpKey || 'mcpServers', itemName);
            } else if (type === 'skill') {
              // Validate that clientParentDir (if provided) is one of the vendor's authorised skill dirs.
              const allowedDirs = [vendor.skillsDir, ...(vendor.skillsDirAliases || [])].filter(Boolean);
              const targetDir = clientParentDir && allowedDirs.includes(clientParentDir) ? clientParentDir : vendor.skillsDir;
              removeSkillFolder(targetDir, itemName);
            } else { throw new Error('Unknown type'); }
            res.writeHead(200); res.end(JSON.stringify({ ok: true }));
            return;
          }

          if (req.url === '/api/edit-mcp') {
            const { vendorId, serverName, newCfg } = payload;
            const vendor = vendors.find(v => v.id === vendorId);
            if (!vendor) { res.writeHead(400); res.end(JSON.stringify({ ok: false, error: 'Vendor not found' })); return; }
            if (vendor.mcpReadOnly) throw new Error(`${(vendor.mcpConfigFormat||'').toUpperCase()} config is read-only`);
            updateMcpInConfig(vendor.mcpConfigPath, vendor.mcpKey, serverName, newCfg);
            // Keep vendor data in sync for future removals
            const srv = vendor.mcpServers.find(m => m.name === serverName);
            if (srv) { srv.command = newCfg.command; srv.args = newCfg.args || []; srv.env = newCfg.env || {}; srv.envKeys = Object.keys(newCfg.env || {}); }
            res.writeHead(200); res.end(JSON.stringify({ ok: true }));
            return;
          }

          if (req.url === '/api/delete-file') {
            const { filePath } = payload;
            deleteFile(filePath);
            res.writeHead(200); res.end(JSON.stringify({ ok: true }));
            return;
          }

          res.writeHead(404); res.end('{"ok":false,"error":"Not found"}');
        } catch (e) {
          res.writeHead(500); res.end(JSON.stringify({ ok: false, error: e.message }));
        }
      });
      return;
    }

    res.writeHead(404); res.end('Not found');
  });

  const actualPort = await new Promise((resolve, reject) => {
    server.listen(port || 0, '127.0.0.1', () => resolve(server.address().port));
    server.on('error', reject);
  });
  srvPort = actualPort;

  const timer = setTimeout(() => {
    console.log('\nAuto-shutdown. Goodbye!');
    server.close(() => process.exit(0));
  }, timeout * 1000);
  timer.unref();

  return { server, port: actualPort };
}

function openBrowser(url) {
  const cmd = platform() === 'darwin' ? 'open' : platform() === 'win32' ? 'start' : 'xdg-open';
  spawnSync(cmd, [url], { shell: platform() === 'win32' });
}

// ─── Main ─────────────────────────────────────────────────────────────────────

async function main() {
  const opts = parseArgs(process.argv);
  console.log('🔍 Scanning AI harness…');
  const vendors = VENDORS.map(collectVendorData);
  console.log('🔧 Checking CLI tools…');
  const clis = CLI_TOOLS.map(collectCliData);
  console.log('🔑 Fetching GitHub rate limits…');
  const rateLimit = await fetchGitHubRateLimit().catch(() => null);

  const html = buildHtml({ vendors, clis, rateLimit, generatedAt: Date.now() });
  console.log('🌐 Starting HTTP server…');
  const { port } = await startServer(html, vendors, opts.port, opts.timeout);
  const url = `http://127.0.0.1:${port}`;

  const totalMcps   = vendors.reduce((s, v) => s + v.mcpServers.length, 0);
  const totalSkills = vendors.reduce((s, v) => s + v.skills.length, 0);
  const confCount   = vendors.filter(v => v.configured || v.skillsConfigured).length;

  console.log(`
╭─────────────────────────────────────────────╮
│  🧰  AI Harness Status Dashboard             │
├─────────────────────────────────────────────┤
│  Vendors configured : ${String(confCount).padEnd(22)} │
│  Total MCPs         : ${String(totalMcps).padEnd(22)} │
│  Total Skills       : ${String(totalSkills).padEnd(22)} │
├─────────────────────────────────────────────┤
│  Dashboard URL: ${url.padEnd(28)} │
│  Server timeout: ${String(opts.timeout).padEnd(2)}s (Ctrl-C to exit)     │
╰─────────────────────────────────────────────╯`);

  if (opts.open) { setTimeout(() => openBrowser(url), 400); console.log(`\nOpening ${url} in your browser…`); }
  console.log('\nPress Ctrl-C to stop the server.\n');
}

main().catch(err => { console.error('Fatal:', err.message); process.exit(1); });
