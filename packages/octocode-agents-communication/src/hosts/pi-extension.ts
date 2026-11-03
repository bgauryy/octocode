import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { bindingArgs, readBinding, runtimeCommand, type HostBinding, type PiHost, type PiToolHost } from './shared/command.js';
import { isObject, type Json } from '../runtime/validation.js';
import { keyedInput, retryKeyPayload } from '../runtime/tool-groups.js';

interface Tool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

/** `tools` come from the catalog (`schema tools`); the binding is fixed or read per call. */
export interface BoundToolsOptions {
  tools: Tool[];
  getBinding?: () => Partial<HostBinding> | null | undefined;
  binary?: string;
  workspace?: string;
  database?: string;
  session?: string;
}

// Some Pi providers require function parameters to be a plain top-level object.
// Keep the field schema and let the shared runtime catalog enforce cross-field rules.
function providerParameters(schema: Record<string, unknown>): Record<string, unknown> {
  const result = { ...schema };
  // Keep all variant argument schemas visible when a provider forbids top-level unions.
  // The canonical runtime still enforces the selected type/argument pairing.
  if (isObject(schema.properties) && 'type' in schema.properties && 'input' in schema.properties && Array.isArray(schema.oneOf)) {
    result.properties = { ...schema.properties, input: { anyOf: schema.oneOf.map(branch => {
      if (!isObject(branch) || !isObject(branch.properties)) throw new Error('Malformed tool variant');
      return branch.properties.input;
    }) } };
  }
  for (const keyword of ['oneOf', 'anyOf', 'allOf', 'enum', 'const', 'not']) delete result[keyword];
  return result;
}

// The parent binds identity and supplies the same catalog used by MCP.
export default function (pi: PiHost): void {
  // A globally enabled idle cache warmer must not add model calls to this worker.
  const options = readBinding<BoundToolsOptions>();
  if (!Array.isArray(options.tools)) throw new Error('OCTOCODE_COMMUNICATION_BINDING must provide the catalog tools');
  pi.on('cache_warming_decision', () => ({ action: 'stop' }));
  registerBoundTools(pi, options as BoundToolsOptions);
}

export function registerBoundTools(pi: PiToolHost, options: BoundToolsOptions): void {
  const { tools } = options;
  const names = new Set(tools.map(tool => tool.name));
  const projected = tools.map(tool => ({ tool, parameters: providerParameters(tool.inputSchema) }));
  const schemas = new Map(projected.map(({ tool, parameters }) => [tool.name, parameters]));
  // Responses can normalize omitted strict into all-required parameters.
  // Preserve the canonical optional fields; the runtime still validates every call.
  pi.on?.('before_provider_request', event => {
    const payload = event.payload;
    if (!Array.isArray(payload?.tools)) return payload;
    let changed = false;
    const descriptors = payload.tools.map(tool => {
      if (!isObject(tool) || tool.type !== 'function') return tool;
      const call = isObject(tool.function) ? tool.function : {};
      if (typeof call.name === 'string' && names.has(call.name)) {
        const parameters = schemas.get(call.name);
        if (call.strict === false && call.parameters === parameters) return tool;
        changed = true;
        return { ...tool, function: { ...call, strict: false, parameters } };
      }
      if (typeof tool.name === 'string' && names.has(tool.name)) {
        const parameters = schemas.get(tool.name);
        if (tool.strict === false && tool.parameters === parameters) return tool;
        changed = true;
        return { ...tool, strict: false, parameters };
      }
      return tool;
    });
    return changed ? { ...payload, tools: descriptors } : payload;
  });
  for (const { tool, parameters } of projected) {
    pi.registerTool({
      name: tool.name,
      label: tool.name,
      description: tool.description,
      parameters,
      async execute(id: string, input: Record<string, unknown>, signal?: AbortSignal) {
        const { binary, workspace, database, session }: Partial<HostBinding> = options.getBinding ? options.getBinding() ?? {} : options;
        if (!session) throw new Error('Communication is disabled or no session is bound');
        if (!binary || !workspace) throw new Error('Communication binding requires a binary and a workspace');
        // Host call IDs survive re-execution; different calls may intentionally repeat a body.
        const payload = retryKeyPayload(tool.name, input);
        const retryable = payload !== null;
        const keyed = retryable && payload.key === undefined;
        if (keyed && (typeof id !== 'string' || !id)) throw new Error('Message execution requires a stable host call ID or explicit key');
        const value = keyed ? keyedInput(tool.name, input, `pi:${createHash('sha256').update(id).digest('hex')}`) : input;
        const json = JSON.stringify(value);
        const stdout = await new Promise<string>((resolve, reject) => {
          let inputError: (Error & { code?: string }) | undefined;
          const invocation = runtimeCommand(binary, [tool.name, '-', ...bindingArgs({ workspace, database, session })]);
          const child = execFile(invocation.command, invocation.args,
          // JSON goes through stdin so large documents do not exceed OS argv limits.
          // CLI pages are bounded to 256 KiB; keep output headroom for metadata.
          { signal, timeout: 10_000, maxBuffer: 1024 * 1024 }, (error, output, stderr) => {
            const failure: (Error & { code?: string | number | null; signal?: string | null }) | undefined = error || inputError;
            if (!failure) { resolve(output); return; }
            const detail = stderr?.trim() || `Communication ${tool.name} failed (${failure.code ?? failure.name}${failure.signal ? `, ${failure.signal}` : ''})`;
            const retry = retryable
              ? ` Outcome may be unknown; inspect history before retrying unchanged input with key ${JSON.stringify(retryKeyPayload(tool.name, value)?.key)}.` : '';
            reject(new Error(detail + retry + (output ? '\nOutput captured before failure: ' + output : '')));
          });
          // An early exit/abort can close the pipe before a large write finishes.
          child.stdin?.on('error', error => { inputError = error; });
          child.stdin?.end(json);
        });
        const details: Json = JSON.parse(stdout);
        return { content: [{ type: 'text', text: JSON.stringify(details) }], details };
      },
    });
  }
}
