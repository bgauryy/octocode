import { defineCli, defineCommand, commandToken, parseCommandInput, runCli, type CliCommand, type CliIo, type CliSpec, type JsonSchema } from 'octocode-mcp-cli';
import { catalog as runtimeCatalog } from '../runtime/catalog.js';
import { asObject, isObject, type JsonObject } from '../runtime/validation.js';
import { capture, stream } from '../runtime/cli.js';
import manifest from '../../package.json' with { type: 'json' };
const maxInputBytes = 8 * 1024 * 1024;
async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  let bytes = 0;
  for await (const chunk of process.stdin) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    bytes += buffer.length;
    if (bytes > maxInputBytes) throw new Error('JSON input exceeds 8 MiB');
    chunks.push(buffer);
  }
  return Buffer.concat(chunks, bytes).toString('utf8');
}
const flagProperty = (name: string) => name.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
const transportDescriptions: Record<string, string> = {
  workspace: 'Checkout root for this invocation; defaults to the current directory.',
  database: 'SQLite store path; defaults to shared Octocode configuration.',
  session: 'Existing communication identity; inspect join output. Required for bound operations.',
  vendor: 'Host vendor for setup or managed identity.', model: 'Vendor model identifier.',
  name: 'Unique managed identity name.', prompt: 'User task for this worker.',
  durationMs: 'Maximum lifetime in milliseconds.', trace: 'Emit protocol trace diagnostics.',
  managed: 'Create an identity owned by this MCP connection; close expires its leases.',
  tools: 'Tool profile or comma-separated tool names; inspect schema-tools.',
  compact: 'Return compact record-type summaries.',
};
const flagOnly = new Set(['mcp', 'run', 'listen', 'host-hook', 'host-config', 'skill', 'schema', 'schema tools', 'db info', 'db protocol']);
const streaming = new Set(['mcp', 'run', 'listen', 'host-hook', 'hook', 'view']);
const booleanFlags = new Set(['trace', 'managed', 'compact']);
const payloadSchemas = new WeakMap<CliCommand, JsonSchema>();
export function createCommunicationCli(): CliSpec {
  const catalog = runtimeCatalog();
  catalog.commands.push(
    { name: 'schema types', description: 'Discover every record type; --compact returns a summary.', usage: 'schema types', cliFlags: ['compact'], inputSchema: { type: 'object', properties: {}, additionalProperties: false } },
    { name: 'schema type', description: 'Inspect one exact record type and its example.', usage: 'schema type', cliFlags: [], inputSchema: { type: 'object', properties: { type: { type: 'string', description: 'Exact record type name, for example coordinate.in.' } }, required: ['type'], additionalProperties: false } },
  );
  const commands = catalog.commands.map(descriptor => {
    const payloadSchema = asObject(descriptor.inputSchema ?? { type: 'object', properties: {}, additionalProperties: false }, `${descriptor.name} inputSchema`);
    const schema: JsonObject = structuredClone(payloadSchema);
    schema.properties ??= {};
    const properties = asObject(schema.properties, `${descriptor.name} properties`);
    const requiredNames = (): string[] => (Array.isArray(schema.required) ? schema.required.filter((name): name is string => typeof name === 'string') : []);
    if (descriptor.name === 'schema') properties.command = { type: 'string', description: 'Exact canonical command name to inspect; omit for the complete catalog.' };
    const transport = new Map<string, string>();
    for (const name of descriptor.cliFlags) {
      const property = flagProperty(name);
      transport.set(property, name);
      properties[property] ??= { type: booleanFlags.has(name) ? 'boolean' : name === 'duration-ms' ? 'integer' : 'string', ...(transportDescriptions[property] === undefined ? {} : { description: transportDescriptions[property] }) };
      if (booleanFlags.has(name)) asObject(properties[property], property).default = false;
    }
    if (descriptor.usage.includes('--session <id>') && !['mcp', 'run'].includes(descriptor.name)) schema.required = [...new Set([...requiredNames(), 'session'])];
    if (descriptor.cliSchema) {
      const extra = asObject(descriptor.cliSchema, `${descriptor.name} cliSchema`);
      schema.required = [...new Set([...requiredNames(), ...(Array.isArray(extra.required) ? extra.required.filter((name): name is string => typeof name === 'string') : [])])];
      schema.properties = { ...properties, ...(isObject(extra.properties) ? extra.properties : {}) };
      if (extra.oneOf) schema.oneOf = extra.oneOf;
    }
    const command = defineCommand({ name: commandToken(descriptor.name), description: descriptor.description,
      mcpName: descriptor.name,
      inputSchema: schema, annotations: descriptor.annotations,
      run: async (input: Record<string, unknown>) => {
        const args = descriptor.name.split(' '), payload = { ...input };
        if (descriptor.name === 'schema' && payload.command !== undefined) { args.push(payload.command as string); delete payload.command; }
        if (descriptor.name === 'schema type') { args.push(payload.type as string); delete payload.type; }
        for (const [property, name] of transport) {
          delete payload[property]; const value = input[property];
          if (value === undefined || value === false) continue;
          args.push('--' + name);
          if (value !== true) args.push(String(value));
        }
        const hasPayload = !flagOnly.has(descriptor.name) && !descriptor.name.startsWith('schema ');
        if (hasPayload) args.push('-');
        const text = hasPayload ? JSON.stringify(payload) : undefined;
        return streaming.has(descriptor.name) ? stream(args, text) : capture(args, text);
      },
    });
    payloadSchemas.set(command, payloadSchema);
    return command;
  });
  return defineCli({ name: 'npx -y @octocodeai/octocode-agents-communication /cli', version: manifest.version,
    instructions: 'Durable cross-vendor communication. Start with join --name <unique-name> --vendor generic --json; reuse its id with --session. Before edits acquire leases with lock or lock_many. Every command has --help and --help --json. Use --json for machine output and copy next.input for every page. --workspace selects the invocation checkout. The default package entry serves MCP; this /cli route runs operations without closing your identity.', commands });
}
export async function runCommunicationCli(argv: readonly string[] = process.argv.slice(2), io?: CliIo): Promise<number> {
  const spec = createCommunicationCli();
  const args = [...argv];
  // Preserve canonical multiword discovery and database names.
  for (const prefix of ['db', 'inbox', 'schema']) {
    if (args[0] === prefix && args[1] && spec.commands.some(command => command.name === `${prefix}-${args[1]}`)) args.splice(0, 2, `${prefix}-${args[1]}`);
  }
  if (args[0] === 'schema' && args[1] && !args[1].startsWith('-')) args.splice(1, 1, '--command', args[1]);
  if (args[0] === 'schema-type' && args[1] && !args[1].startsWith('-')) args.splice(1, 1, '--type', args[1]);
  const command = spec.commands.find(item => item.name === args[0]);
  // Existing JSON invocations remain valid; all values still pass shared schema validation.
  const position = args[1]?.startsWith('{') || args[1] === '-' ? 1 : -1;
  if (command && position !== -1) {
    const raw = args[position] === '-' ? await readStdin() : args[position];
    if (Buffer.byteLength(raw) > maxInputBytes) throw new Error('JSON input exceeds 8 MiB');
    const input = JSON.parse(raw);
    if (!input || typeof input !== 'object' || Array.isArray(input)) throw new Error('JSON input must be an object');
    // Validate before converting JSON to text flags, so coercion cannot hide bad types.
    parseCommandInput(defineCommand({ name: 'validate', description: '', inputSchema: payloadSchemas.get(command), run() {} }), input);
    const flags: string[] = [];
    for (const [key, value] of Object.entries(input)) {
      const flag = command.flags.find(item => item.property === key);
      if (!flag) throw new Error(`Unknown input field: ${key}`);
      const add = (item: unknown) => flags.push(`--${flag.name}=${typeof item === 'object' || flag.kind === 'json' ? JSON.stringify(item) : String(item)}`);
      if (flag.presence) { if (value === true) flags.push(`--${flag.name}`); else if (value !== false) throw new Error(`${key} must be boolean`); }
      else if (flag.kind === 'array' && Array.isArray(value)) flags.push(`--${flag.name}=${JSON.stringify(value)}`);
      else add(value);
    }
    args.splice(position, 1, ...flags);
  }
  return runCli(spec, args, io);
}
