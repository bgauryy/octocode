import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { runtimeCommand } from './cli-command.mjs';

// The parent binds identity and supplies the same catalog used by MCP.
export default function (pi) {
  // A globally enabled idle cache warmer must not add model calls to this worker.
  pi.on('cache_warming_decision', () => ({ action: 'stop' }));
  registerBoundTools(pi, JSON.parse(process.env.OCTOCODE_COMMUNICATION_BINDING));
}

export function registerBoundTools(pi, options) {
  const { tools } = options;
  const names = new Set(tools.map(tool => tool.name));
  // Responses can normalize omitted strict into all-required parameters.
  // Preserve the canonical optional fields; the runtime still validates every call.
  pi.on?.('before_provider_request', event => {
    const payload = event.payload;
    if (!Array.isArray(payload?.tools)) return payload;
    let changed = false;
    const descriptors = payload.tools.map(tool => {
      if (tool.type !== 'function') return tool;
      if (names.has(tool.function?.name) && tool.function.strict !== false) {
        changed = true;
        return { ...tool, function: { ...tool.function, strict: false } };
      }
      if (names.has(tool.name) && tool.strict !== false) {
        changed = true;
        return { ...tool, strict: false };
      }
      return tool;
    });
    return changed ? { ...payload, tools: descriptors } : payload;
  });
  for (const tool of tools) {
    pi.registerTool({
      name: tool.name,
      label: tool.name,
      description: tool.description,
      parameters: tool.inputSchema,
      async execute(id, input, signal) {
        const { binary, workspace, database, session } = options.getBinding ? options.getBinding() ?? {} : options;
        if (!session) throw new Error("Communication is disabled or no session is bound");
        // Host call IDs survive re-execution; different calls may intentionally repeat a body.
        const keyed = ['send_message', 'notify_all'].includes(tool.name) && input.key === undefined;
        if (keyed && (typeof id !== 'string' || !id)) throw new Error('Message execution requires a stable host call ID or explicit key');
        const json = JSON.stringify(keyed ? { ...input, key: `pi:${createHash('sha256').update(id).digest('hex')}` } : input);
        const stdout = await new Promise((resolve, reject) => {
          let inputError;
          const invocation = runtimeCommand(binary, [tool.name, '-',
            '--workspace', workspace, '--database', database, '--session', session]);
          const child = execFile(invocation.command, invocation.args,
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
