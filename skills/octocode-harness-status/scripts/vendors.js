/**
 * vendors.js — Comprehensive vendor registry for AI agents, IDEs, CLIs, and desktop apps
 *
 * Covers every known MCP-capable client, skills directory, and agent toolkit
 * across macOS, Linux, and Windows (including Raspberry Pi / Linux ARM).
 *
 * Usage:
 *   import { getVendors } from './vendors.js';
 *   const vendors = getVendors();   // resolved for current platform + HOME
 *
 *   import { VENDOR_MAP } from './vendors.js';
 *   const cursor = VENDOR_MAP.cursor;
 */

import { homedir, platform } from 'node:os';
import { join } from 'node:path';

const HOME = homedir();
const OS   = platform(); // 'darwin' | 'linux' | 'win32'
const IS_MAC   = OS === 'darwin';
const IS_WIN   = OS === 'win32';
const IS_LINUX = OS === 'linux';  // includes Raspberry Pi (linux/arm64)
const IS_PI    = IS_LINUX;        // treat all Linux as Pi-compatible

// ── Path helpers ──────────────────────────────────────────────────────────────

/** Resolve a path table { darwin, linux, win32 } → string for the current OS. */
function p(paths) {
  const key = IS_MAC ? 'darwin' : IS_WIN ? 'win32' : 'linux';
  return paths[key] ?? paths.darwin ?? null;
}

/** App-data root per platform */
const APPDATA   = IS_WIN ? (process.env.APPDATA  || join(HOME, 'AppData', 'Roaming')) : null;
const LIBAPP    = IS_MAC ? join(HOME, 'Library', 'Application Support') : null;
const XDGCFG    = IS_LINUX ? (process.env.XDG_CONFIG_HOME || join(HOME, '.config')) : null;

/** Resolve cross-platform App Support path. */
function app(macName, linuxName, winName) {
  if (IS_MAC)   return join(LIBAPP,   macName   || linuxName || winName);
  if (IS_WIN)   return join(APPDATA,  winName   || macName   || linuxName);
  return               join(XDGCFG,  linuxName || macName);
}

// ── Vendor categories ─────────────────────────────────────────────────────────

/**
 * @typedef {Object} VendorDef
 * @property {string}   id             — machine-readable identifier
 * @property {string}   name           — display name
 * @property {string}   emoji          — representative emoji
 * @property {string}   category       — 'ide' | 'cli' | 'desktop' | 'extension' | 'agent-folder'
 * @property {string|null} mcpConfigPath — resolved absolute path to the MCP config file
 * @property {string}   mcpKey         — JSON key that contains mcpServers (e.g. "mcpServers")
 * @property {string|null} mcpConfigFormat — 'json' | 'yaml' | 'toml' | null (unstructured)
 * @property {string|null} skillsDir    — resolved absolute path to the skills directory
 * @property {string[]}  skillsDirAliases — additional skills dirs this vendor may load from
 * @property {number}   contextWindow  — typical agent context window in tokens
 * @property {string}   agentModel     — primary model(s) used
 * @property {boolean}  mcpSupport     — whether the client supports MCP
 * @property {boolean}  skillsSupport  — whether the client loads SKILL.md files
 * @property {boolean}  cliTool        — true if this is a terminal-only tool
 * @property {string}   installUrl     — canonical install / docs URL
 * @property {string|null} color       — brand hex color (for UI)
 */

// ── IDE / Editor vendors ──────────────────────────────────────────────────────

const CURSOR = {
  id: 'cursor',
  name: 'Cursor',
  emoji: '⬛',
  category: 'ide',
  mcpConfigPath: p({
    darwin: join(HOME, '.cursor', 'mcp.json'),
    linux:  join(HOME, '.cursor', 'mcp.json'),
    win32:  join(APPDATA || HOME, 'Cursor', 'mcp.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: join(HOME, '.cursor', 'skills'),
  skillsDirAliases: [join(HOME, '.cursor', 'skills-cursor')],
  contextWindow: 200_000,
  agentModel: 'Claude 4 / GPT-4o',
  mcpSupport: true,
  skillsSupport: true,
  cliTool: false,
  installUrl: 'https://cursor.com',
  color: '#1fb6ff',
};

const WINDSURF = {
  id: 'windsurf',
  name: 'Windsurf',
  emoji: '🌊',
  category: 'ide',
  mcpConfigPath: p({
    darwin: join(HOME, '.codeium', 'windsurf', 'mcp_config.json'),
    linux:  join(HOME, '.codeium', 'windsurf', 'mcp_config.json'),
    win32:  join(HOME, '.codeium', 'windsurf', 'mcp_config.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: join(HOME, '.codeium', 'windsurf', 'skills'),
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude / GPT-4o',
  mcpSupport: true,
  skillsSupport: true,
  cliTool: false,
  installUrl: 'https://codeium.com/windsurf',
  color: '#0091ff',
};

const ZED = {
  id: 'zed',
  name: 'Zed',
  emoji: '⚡',
  category: 'ide',
  mcpConfigPath: p({
    darwin: join(HOME, '.config', 'zed', 'settings.json'),
    linux:  join(HOME, '.config', 'zed', 'settings.json'),
    win32:  join(APPDATA || HOME, 'Zed', 'settings.json'),
  }),
  mcpKey: 'context_servers',         // Zed uses context_servers, not mcpServers
  mcpConfigFormat: 'json',
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude / GPT-4o',
  mcpSupport: true,
  skillsSupport: false,
  cliTool: false,
  installUrl: 'https://zed.dev',
  color: '#084cdf',
};

const KIRO = {
  id: 'kiro',
  name: 'Kiro',
  emoji: '🎯',
  category: 'ide',
  mcpConfigPath: p({
    darwin: join(HOME, '.kiro', 'mcp.json'),
    linux:  join(HOME, '.kiro', 'mcp.json'),
    win32:  join(APPDATA || HOME, 'Kiro', 'mcp.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: join(HOME, '.kiro', 'skills'),
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude / GPT-4o (Amazon)',
  mcpSupport: true,
  skillsSupport: true,
  cliTool: false,
  installUrl: 'https://kiro.dev',
  color: '#ff6b35',
};

const TRAE = {
  id: 'trae',
  name: 'Trae',
  emoji: '🌀',
  category: 'ide',
  mcpConfigPath: p({
    darwin: join(HOME, 'Library', 'Application Support', 'Trae', 'mcp.json'),
    linux:  join(HOME, '.config', 'Trae', 'mcp.json'),
    win32:  join(APPDATA || HOME, 'Trae', 'mcp.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude / GPT-4o',
  mcpSupport: true,
  skillsSupport: false,
  cliTool: false,
  installUrl: 'https://www.trae.ai',
  color: '#6c47ff',
};

const VOID = {
  id: 'void',
  name: 'Void',
  emoji: '🌑',
  category: 'ide',
  mcpConfigPath: p({
    darwin: join(HOME, 'Library', 'Application Support', 'Void', 'User', 'mcp.json'),
    linux:  join(HOME, '.config', 'Void', 'User', 'mcp.json'),
    win32:  join(APPDATA || HOME, 'Void', 'User', 'mcp.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude / GPT-4o',
  mcpSupport: true,   // announced, rolling out
  skillsSupport: false,
  cliTool: false,
  installUrl: 'https://voideditor.com',
  color: '#1a1a2e',
};

// ── CLI agents ────────────────────────────────────────────────────────────────

const CLAUDE_CODE = {
  id: 'claude-code',
  name: 'Claude Code',
  emoji: '🔶',
  category: 'cli',
  mcpConfigPath: p({
    darwin: join(HOME, '.claude', 'settings.json'),
    linux:  join(HOME, '.claude', 'settings.json'),
    win32:  join(HOME, '.claude', 'settings.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: join(HOME, '.claude', 'skills'),
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude 4 Sonnet / Opus',
  mcpSupport: true,
  skillsSupport: true,
  cliTool: true,
  installUrl: 'https://docs.anthropic.com/en/docs/claude-code',
  color: '#d97706',
};

const CODEX_CLI = {
  id: 'codex',
  name: 'Codex CLI',
  emoji: '🧠',
  category: 'cli',
  mcpConfigPath: p({
    darwin: join(HOME, '.codex', 'config.toml'),
    linux:  join(HOME, '.codex', 'config.toml'),
    win32:  join(HOME, '.codex', 'config.toml'),
  }),
  mcpKey: 'mcp_servers',         // Codex uses [mcp_servers.NAME] TOML tables
  mcpConfigFormat: 'toml',
  skillsDir: join(HOME, '.codex', 'skills'),
  skillsDirAliases: [],
  contextWindow: 128_000,
  agentModel: 'GPT-4.1 / o3',
  mcpSupport: true,
  skillsSupport: true,
  cliTool: true,
  installUrl: 'https://github.com/openai/codex',
  color: '#10a37f',
};

const GEMINI_CLI = {
  id: 'gemini-cli',
  name: 'Gemini CLI',
  emoji: '✨',
  category: 'cli',
  mcpConfigPath: p({
    darwin: join(HOME, '.gemini', 'settings.json'),
    linux:  join(HOME, '.gemini', 'settings.json'),
    win32:  join(HOME, '.gemini', 'settings.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: join(HOME, '.gemini', 'skills'),
  skillsDirAliases: [],
  contextWindow: 1_000_000,
  agentModel: 'Gemini 2.5 Pro',
  mcpSupport: true,
  skillsSupport: true,
  cliTool: true,
  installUrl: 'https://github.com/google-gemini/gemini-cli',
  color: '#4285f4',
};

const GOOSE = {
  id: 'goose',
  name: 'Goose',
  emoji: '🦆',
  category: 'cli',
  mcpConfigPath: p({
    darwin: join(HOME, 'Library', 'Application Support', 'goose', 'config.yaml'),
    linux:  join(HOME, '.config', 'goose', 'config.yaml'),
    win32:  join(APPDATA || HOME, 'goose', 'config.yaml'),
  }),
  mcpKey: 'extensions',           // Goose uses "extensions" not "mcpServers"
  mcpConfigFormat: 'yaml',
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 128_000,
  agentModel: 'Claude / GPT / Gemini (multi)',
  mcpSupport: true,
  skillsSupport: false,
  cliTool: true,
  installUrl: 'https://github.com/block/goose',
  color: '#22c55e',
};

const OPENCODE = {
  id: 'opencode',
  name: 'OpenCode',
  emoji: '🔓',
  category: 'cli',
  mcpConfigPath: p({
    darwin: join(HOME, 'Library', 'Application Support', 'opencode', 'config.json'),
    linux:  join(HOME, '.config', 'opencode', 'config.json'),
    win32:  join(APPDATA || HOME, 'opencode', 'config.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: join(HOME, '.opencode', 'skills'),
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude / GPT (configurable)',
  mcpSupport: true,
  skillsSupport: true,
  cliTool: true,
  installUrl: 'https://opencode.ai',
  color: '#f59e0b',
};

const AIDER = {
  id: 'aider',
  name: 'Aider',
  emoji: '🤝',
  category: 'cli',
  mcpConfigPath: null,            // Aider does not support MCP natively
  mcpKey: null,
  mcpConfigFormat: null,
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 128_000,
  agentModel: 'GPT-4 / Claude / Gemini',
  mcpSupport: false,
  skillsSupport: false,
  cliTool: true,
  installUrl: 'https://aider.chat',
  color: '#8b5cf6',
};

const PLANDEX = {
  id: 'plandex',
  name: 'Plandex',
  emoji: '📐',
  category: 'cli',
  mcpConfigPath: null,
  mcpKey: null,
  mcpConfigFormat: null,
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 128_000,
  agentModel: 'GPT-4 / Claude',
  mcpSupport: false,
  skillsSupport: false,
  cliTool: true,
  installUrl: 'https://plandex.ai',
  color: '#7c3aed',
};

// ── Desktop apps ──────────────────────────────────────────────────────────────

const CLAUDE_DESKTOP = {
  id: 'claude-desktop',
  name: 'Claude Desktop',
  emoji: '🟣',
  category: 'desktop',
  mcpConfigPath: p({
    darwin: join(HOME, 'Library', 'Application Support', 'Claude', 'claude_desktop_config.json'),
    linux:  join(HOME, '.config', 'claude', 'claude_desktop_config.json'),
    win32:  join(APPDATA || HOME, 'Claude', 'claude_desktop_config.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude 4 Sonnet / Opus',
  mcpSupport: true,
  skillsSupport: false,
  cliTool: false,
  installUrl: 'https://claude.ai/download',
  color: '#d97706',
};

const ANTIGRAVITY = {
  id: 'antigravity',
  name: 'Antigravity (Gemini)',
  emoji: '🚀',
  category: 'desktop',
  mcpConfigPath: p({
    darwin: join(HOME, '.gemini', 'antigravity', 'mcp_config.json'),
    linux:  join(HOME, '.gemini', 'antigravity', 'mcp_config.json'),
    win32:  join(HOME, '.gemini', 'antigravity', 'mcp_config.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 1_000_000,
  agentModel: 'Gemini 2.5 Pro',
  mcpSupport: true,
  skillsSupport: false,
  cliTool: false,
  installUrl: 'https://gemini.google.com',
  color: '#4285f4',
};

// ── VS Code extensions ────────────────────────────────────────────────────────

const VSCODE_CLINE = {
  id: 'vscode-cline',
  name: 'VS Code — Cline',
  emoji: '🔵',
  category: 'extension',
  mcpConfigPath: p({
    darwin: join(HOME, 'Library', 'Application Support', 'Code', 'User', 'globalStorage', 'saoudrizwan.claude-dev', 'settings', 'cline_mcp_settings.json'),
    linux:  join(HOME, '.config', 'Code', 'User', 'globalStorage', 'saoudrizwan.claude-dev', 'settings', 'cline_mcp_settings.json'),
    win32:  join(APPDATA || HOME, 'Code', 'User', 'globalStorage', 'saoudrizwan.claude-dev', 'settings', 'cline_mcp_settings.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude / GPT-4o',
  mcpSupport: true,
  skillsSupport: false,
  cliTool: false,
  installUrl: 'https://marketplace.visualstudio.com/items?itemName=saoudrizwan.claude-dev',
  color: '#007acc',
};

const VSCODE_ROO = {
  id: 'vscode-roo',
  name: 'VS Code — Roo Code',
  emoji: '🦘',
  category: 'extension',
  mcpConfigPath: p({
    darwin: join(HOME, 'Library', 'Application Support', 'Code', 'User', 'globalStorage', 'rooveterinaryinc.roo-cline', 'settings', 'mcp_settings.json'),
    linux:  join(HOME, '.config', 'Code', 'User', 'globalStorage', 'rooveterinaryinc.roo-cline', 'settings', 'mcp_settings.json'),
    win32:  join(APPDATA || HOME, 'Code', 'User', 'globalStorage', 'rooveterinaryinc.roo-cline', 'settings', 'mcp_settings.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude / GPT-4o',
  mcpSupport: true,
  skillsSupport: false,
  cliTool: false,
  installUrl: 'https://marketplace.visualstudio.com/items?itemName=RooVeterinaryInc.roo-cline',
  color: '#007acc',
};

const VSCODE_CONTINUE = {
  id: 'vscode-continue',
  name: 'VS Code — Continue',
  emoji: '▶️',
  category: 'extension',
  mcpConfigPath: p({
    darwin: join(HOME, '.continue', 'config.json'),
    linux:  join(HOME, '.continue', 'config.json'),
    win32:  join(HOME, '.continue', 'config.json'),
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Claude / GPT-4o / Gemini',
  mcpSupport: true,
  skillsSupport: false,
  cliTool: false,
  installUrl: 'https://marketplace.visualstudio.com/items?itemName=Continue.continue',
  color: '#1e3a5f',
};

const VSCODE_COPILOT = {
  id: 'vscode-copilot',
  name: 'VS Code — GitHub Copilot',
  emoji: '🐙',
  category: 'extension',
  mcpConfigPath: p({
    darwin: join(HOME, '.vscode', 'mcp.json'),   // VS Code 1.99+ global MCP config
    linux:  join(HOME, '.vscode', 'mcp.json'),
    win32:  join(HOME, '.vscode', 'mcp.json'),
  }),
  mcpKey: 'servers',              // Copilot uses "servers" not "mcpServers"
  mcpConfigFormat: 'json',
  skillsDir: null,
  skillsDirAliases: [],
  contextWindow: 128_000,
  agentModel: 'GPT-4o / Claude Sonnet',
  mcpSupport: true,               // VS Code 1.99+ agent mode
  skillsSupport: false,
  cliTool: false,
  installUrl: 'https://github.com/features/copilot',
  color: '#238636',
};

// ── Agent / skills-only folders ───────────────────────────────────────────────

const AGENTS_FOLDER = {
  id: 'agents',
  name: '.agents (universal)',
  emoji: '🤖',
  category: 'agent-folder',
  mcpConfigPath: null,
  mcpKey: null,
  mcpConfigFormat: null,
  skillsDir: join(HOME, '.agents', 'skills'),
  skillsDirAliases: [],
  contextWindow: 200_000,
  agentModel: 'Universal — any agent',
  mcpSupport: false,
  skillsSupport: true,
  cliTool: false,
  installUrl: 'https://github.com/bgauryy/octocode-mcp',
  color: '#6366f1',
};

// ── Raspberry Pi / Linux ARM ──────────────────────────────────────────────────
// Pi uses the same Linux paths; this entry is provided for explicit Pi targeting
// in the harness UI. Actual paths mirror the Linux entries above.

const PI = {
  id: 'pi',
  name: 'Raspberry Pi (Linux)',
  emoji: '🍓',
  category: 'pi',
  mcpConfigPath: p({
    darwin: join(HOME, '.cursor', 'mcp.json'),   // fallback if testing on mac
    linux:  join(HOME, '.cursor', 'mcp.json'),   // Pi runs Cursor via arm64
    win32:  null,
  }),
  mcpKey: 'mcpServers',
  mcpConfigFormat: 'json',
  // Pi typically uses Claude Desktop or a custom host; skills live in ~/.claude/skills
  skillsDir: p({
    darwin: join(HOME, '.claude', 'skills'),
    linux:  join(HOME, '.claude', 'skills'),
    win32:  null,
  }),
  skillsDirAliases: [
    join(HOME, '.cursor', 'skills'),
    join(HOME, '.agents', 'skills'),
  ],
  contextWindow: 200_000,
  agentModel: 'Claude (via octocode-mcp on ARM64)',
  mcpSupport: true,
  skillsSupport: true,
  cliTool: false,
  installUrl: 'https://github.com/bgauryy/octocode-mcp/blob/main/docs/configuration/clients/PI_SETUP_GUIDE.md',
  color: '#c13c3c',
  notes: 'Use --host=0.0.0.0 when running octocode-mcp on Pi so remote clients can connect. See PI_SETUP_GUIDE.md.',
};

// ── All vendors list (canonical order) ───────────────────────────────────────

/** Flat ordered array of all vendor definitions. */
export const VENDORS = [
  // IDEs
  CURSOR,
  WINDSURF,
  ZED,
  KIRO,
  TRAE,
  VOID,
  // CLI agents
  CLAUDE_CODE,
  CODEX_CLI,
  GEMINI_CLI,
  GOOSE,
  OPENCODE,
  AIDER,
  PLANDEX,
  // Desktop apps
  CLAUDE_DESKTOP,
  ANTIGRAVITY,
  // VS Code extensions
  VSCODE_CLINE,
  VSCODE_ROO,
  VSCODE_CONTINUE,
  VSCODE_COPILOT,
  // Agent folders
  AGENTS_FOLDER,
  // Special
  PI,
];

/** Lookup by vendor id. */
export const VENDOR_MAP = Object.fromEntries(VENDORS.map(v => [v.id, v]));

/**
 * Returns the full vendor list resolved for the current platform and HOME.
 * Filters out vendors whose config and skills paths both don't exist when
 * `onlyInstalled` is true.
 */
export function getVendors({ onlyInstalled = false } = {}) {
  if (!onlyInstalled) return VENDORS;
  // Dynamic import for ES module compat — callers must await this
  return import('node:fs').then(({ existsSync }) =>
    VENDORS.filter(v =>
      (v.mcpConfigPath && existsSync(v.mcpConfigPath)) ||
      (v.skillsDir && existsSync(v.skillsDir)) ||
      (v.skillsDirAliases || []).some(d => existsSync(d))
    )
  );
}

/**
 * Returns vendors by category.
 * @param {'ide'|'cli'|'desktop'|'extension'|'agent-folder'|'pi'} category
 */
export function getVendorsByCategory(category) {
  return VENDORS.filter(v => v.category === category);
}

/** Returns only vendors that have MCP config support on this platform. */
export function getMcpVendors() {
  return VENDORS.filter(v => v.mcpSupport && v.mcpConfigPath);
}

/** Returns only vendors that have a skills directory on this platform. */
export function getSkillsVendors() {
  return VENDORS.filter(v => v.skillsSupport && v.skillsDir);
}

/** All unique skills directories (deduped). */
export function getAllSkillsDirs() {
  const seen = new Set();
  const dirs = [];
  for (const v of VENDORS) {
    if (v.skillsDir && !seen.has(v.skillsDir)) { seen.add(v.skillsDir); dirs.push(v.skillsDir); }
    for (const d of (v.skillsDirAliases || [])) {
      if (!seen.has(d)) { seen.add(d); dirs.push(d); }
    }
  }
  return dirs;
}

// ── Platform summary ──────────────────────────────────────────────────────────

export const PLATFORM_INFO = {
  os: OS,
  home: HOME,
  isMac: IS_MAC,
  isLinux: IS_LINUX,
  isWin: IS_WIN,
  isPi: IS_PI,        // true on any Linux (ARM or x86)
};
