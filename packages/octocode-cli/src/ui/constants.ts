export const CLIENT_INFO = {
  cursor: {
    name: 'Cursor',
    description: 'AI-first code editor',
    url: 'https://cursor.sh',
  },
  'claude-desktop': {
    name: 'Claude Desktop',
    description: "Anthropic's Claude desktop app",
    url: 'https://claude.ai/download',
  },
  'claude-code': {
    name: 'Claude Code',
    description: 'Claude CLI for terminal',
    url: 'https://docs.anthropic.com/claude-code',
  },
  opencode: {
    name: 'Opencode',
    description: 'AI coding agent CLI',
    url: 'https://opencode.ai',
  },
  'vscode-cline': {
    name: 'Cline (VS Code)',
    description: 'AI coding assistant extension',
    url: 'https://marketplace.visualstudio.com/items?itemName=saoudrizwan.claude-dev',
  },
  'vscode-roo': {
    name: 'Roo-Cline (VS Code)',
    description: 'Roo AI coding extension',
    url: 'https://github.com/RooVetGit/Roo-Cline',
  },
  'vscode-continue': {
    name: 'Continue (VS Code)',
    description: 'Open-source AI assistant',
    url: 'https://continue.dev',
  },
  windsurf: {
    name: 'Windsurf',
    description: 'Codeium AI IDE',
    url: 'https://codeium.com/windsurf',
  },
  trae: {
    name: 'Trae',
    description: 'Adaptive AI IDE',
    url: 'https://trae.ai',
  },
  antigravity: {
    name: 'Antigravity',
    description: 'Gemini-powered AI IDE',
    url: 'https://antigravity.dev',
  },
  zed: {
    name: 'Zed',
    description: 'High-performance code editor',
    url: 'https://zed.dev',
  },
  codex: {
    name: 'Codex',
    description: 'OpenAI Codex CLI agent',
    url: 'https://github.com/openai/codex',
  },
  'gemini-cli': {
    name: 'Gemini CLI',
    description: 'Google Gemini CLI',
    url: 'https://github.com/google-gemini/gemini-cli',
  },
  goose: {
    name: 'Goose',
    description: 'Block AI coding agent',
    url: 'https://block.github.io/goose',
  },
  kiro: {
    name: 'Kiro',
    description: 'AWS AI IDE',
    url: 'https://kiro.dev',
  },
  custom: {
    name: 'Custom Path',
    description: 'Specify your own MCP config path',
    url: '',
  },

  claude: {
    name: 'Claude Desktop',
    description: "Anthropic's Claude desktop app",
    url: 'https://claude.ai/download',
  },
} as const;

export const IDE_INFO = {
  cursor: CLIENT_INFO.cursor,
  claude: CLIENT_INFO.claude,
} as const;

export const INSTALL_METHOD_INFO = {
  npx: {
    name: 'NPX',
    description: 'Run via npx from npm registry',
    pros: ['Standard npm workflow', 'Faster after first run (cached)'],
    cons: ['Requires Node.js/npm'],
  },
} as const;
