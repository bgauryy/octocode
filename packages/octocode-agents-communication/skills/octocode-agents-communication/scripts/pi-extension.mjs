import { execFile } from 'node:child_process';

// The Rust parent binds identity and supplies the same catalog used by MCP.
export default function (pi) {
  // A globally enabled idle cache warmer must not add model calls to this worker.
  pi.on('cache_warming_decision', () => ({ action: 'stop' }));
  registerBoundTools(pi, JSON.parse(process.env.OCTOCODE_COMMUNICATION_BINDING));
}

export function registerBoundTools(pi, options) {
  const { tools } = options;
  for (const tool of tools) {
    pi.registerTool({
      name: tool.name,
      label: tool.name,
      description: tool.description,
      parameters: tool.inputSchema,
      async execute(_id, input, signal) {
        const { binary, workspace, database, session } = options.getBinding ? options.getBinding() ?? {} : options;
        if (!session) throw new Error("Communication is disabled or no session is bound");
        const json = JSON.stringify(input);
        const stdout = await new Promise((resolve, reject) => {
          let inputError;
          const child = execFile(binary, [tool.name, '-',
            '--workspace', workspace, '--database', database, '--session', session],
          // JSON goes through stdin so large documents do not exceed OS argv limits.
          // CLI pages are bounded to 256 KiB; keep output headroom for metadata.
          { signal, timeout: 10_000, maxBuffer: 1024 * 1024 }, (error, output) => {
            if (error || inputError) reject(error || inputError);
            else resolve(output);
          });
          // An early exit/abort can close the pipe before a large write finishes.
          child.stdin.on('error', error => { inputError = error; });
          child.stdin.end(json);
        });
        const details = JSON.parse(stdout);
        return { content: [{ type: 'text', text: JSON.stringify(details) }], details };
      },
    });
  }
}
