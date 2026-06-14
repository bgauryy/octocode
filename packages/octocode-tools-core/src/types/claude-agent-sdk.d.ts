declare module '@anthropic-ai/claude-agent-sdk' {
  export function query(input: {
    prompt: string;
    options: {
      model?: string;
      maxTurns?: number;
      cwd?: string;
      allowedTools?: string[];
      mcpServers: Record<
        string,
        {
          command: string;
          args: string[];
          env?: Record<string, string>;
        }
      >;
      permissionMode: 'bypassPermissions';
      allowDangerouslySkipPermissions: boolean;
    };
  }): AsyncIterable<unknown> & {
    mcpServerStatus?: () => Promise<unknown>;
  };
}
