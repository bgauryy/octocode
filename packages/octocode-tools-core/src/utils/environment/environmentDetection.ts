import { getConfigSync } from 'octocode-shared';

type Environment = 'vscode' | 'claude-code-mcp' | 'cursor' | 'standalone';

export function detectEnvironment(): Environment {
  if (process.env.VSCODE_PID || process.env.VSCODE_IPC_HOOK) {
    return 'vscode';
  }
  if (process.env.CURSOR_CHANNEL || process.env.CURSOR_TRACE_ID) {
    return 'cursor';
  }
  return 'standalone';
}

export function shouldUseMCPLsp(): boolean {
  try {
    return getConfigSync().local.enabled;
  } catch {
    return false;
  }
}

export function getLspEnvironmentHint(): string | null {
  try {
    if (!getConfigSync().local.enabled) {
      return 'Local tools are disabled (ENABLE_LOCAL=false). MCP LSP tools are unavailable.';
    }
  } catch {
    void 0;
  }
  return null;
}
