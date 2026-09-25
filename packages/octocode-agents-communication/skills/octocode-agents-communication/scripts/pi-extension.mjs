import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const exec = promisify(execFile);

// The Rust parent binds identity and supplies the same catalog used by MCP.
export default function (pi) {
  // A globally enabled idle cache warmer must not add model calls to this worker.
  pi.on('cache_warming_decision', () => ({ action: 'stop' }));
  registerBoundTools(pi, JSON.parse(process.env.OCTOCODE_COMMUNICATION_BINDING));
}

export function registerBoundTools(pi, { binary, workspace, database, session, tools }) {
  for (const tool of tools) {
    pi.registerTool({
      name: tool.name,
      label: tool.name,
      description: tool.description,
      parameters: tool.inputSchema,
      async execute(_id, input, signal) {
        const { stdout } = await exec(binary, [tool.name, JSON.stringify(input),
          '--workspace', workspace, '--database', database, '--session', session],
        // CLI pages are bounded to 256 KiB; keep transport headroom for metadata.
        { signal, timeout: 10_000, maxBuffer: 1024 * 1024 });
        const details = JSON.parse(stdout);
        return { content: [{ type: 'text', text: JSON.stringify(details) }], details };
      },
    });
  }
}
