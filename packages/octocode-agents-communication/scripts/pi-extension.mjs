import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { runtimeCommand } from './cli-command.mjs';

// Some Pi providers require function parameters to be a plain top-level object.
// Keep the field schema and let the shared Python catalog enforce cross-field rules.
function providerParameters(schema) {
  const result = { ...schema };
  for (const keyword of ['oneOf', 'anyOf', 'allOf', 'enum', 'const', 'not']) delete result[keyword];
  return result;
}

// The parent binds identity and supplies the same catalog used by MCP.
export default function (pi) {
  // A globally enabled idle cache warmer must not add model calls to this worker.
  pi.on('cache_warming_decision', () => ({ action: 'stop' }));
  registerBoundTools(pi, JSON.parse(process.env.OCTOCODE_COMMUNICATION_BINDING));
}

export function registerBoundTools(pi, options) {
  const { tools } = options;
  const names = new Set(tools.map(tool => tool.name));
  const schemas = new Map(tools.map(tool => [tool.name, providerParameters(tool.inputSchema)]));
  // Responses can normalize omitted strict into all-required parameters.
  // Preserve the canonical optional fields; the runtime still validates every call.
  pi.on?.('before_provider_request', event => {
    const payload = event.payload;
    if (!Array.isArray(payload?.tools)) return payload;
    let changed = false;
    const descriptors = payload.tools.map(tool => {
      if (tool.type !== 'function') return tool;
      if (names.has(tool.function?.name)) {
        const parameters = schemas.get(tool.function.name);
        if (tool.function.strict === false && tool.function.parameters === parameters) return tool;
        changed = true;
        return { ...tool, function: { ...tool.function, strict: false, parameters } };
      }
      if (names.has(tool.name)) {
        const parameters = schemas.get(tool.name);
        if (tool.strict === false && tool.parameters === parameters) return tool;
        changed = true;
        return { ...tool, strict: false, parameters };
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
      parameters: schemas.get(tool.name),
      async execute(id, input, signal) {
        const { binary, workspace, database, session } = options.getBinding ? options.getBinding() ?? {} : options;
        if (!session) throw new Error("Communication is disabled or no session is bound");
        // Host call IDs survive re-execution; different calls may intentionally repeat a body.
        const retryable = ['send_message', 'notify_all', 'record'].includes(tool.name);
        const keyed = retryable && input.key === undefined;
        if (keyed && (typeof id !== 'string' || !id)) throw new Error('Message execution requires a stable host call ID or explicit key');
        const value = keyed ? { ...input, key: `pi:${createHash('sha256').update(id).digest('hex')}` } : input;
        const json = JSON.stringify(value);
        const stdout = await new Promise((resolve, reject) => {
          let inputError;
          const invocation = runtimeCommand(binary, [tool.name, '-',
            '--workspace', workspace, '--database', database, '--session', session]);
          const child = execFile(invocation.command, invocation.args,
          // JSON goes through stdin so large documents do not exceed OS argv limits.
          // CLI pages are bounded to 256 KiB; keep output headroom for metadata.
          { signal, timeout: 10_000, maxBuffer: 1024 * 1024 }, (error, output, stderr) => {
            const failure = error || inputError;
            if (!failure) { resolve(output); return; }
            const detail = stderr?.trim() || `Communication ${tool.name} failed (${failure.code ?? failure.name}${failure.signal ? `, ${failure.signal}` : ''})`;
            const retry = retryable
              ? ` Outcome may be unknown; inspect history before retrying unchanged input with key ${JSON.stringify(value.key)}.` : '';
            reject(new Error(detail + retry + (output ? '\nOutput captured before failure: ' + output : '')));
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
