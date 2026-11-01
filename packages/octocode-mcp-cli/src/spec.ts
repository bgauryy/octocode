import addFormats from 'ajv-formats';
import Ajv2020 from 'ajv/dist/2020.js';
import { z, ZodError, type ZodType } from 'zod';
import { flagsFromJsonSchema, objectProperties } from './json-schema-flags.js';
import { notesOf } from './flag-names.js';
import { isRecord } from './json-schema.js';
import { flagsFromZod, zodRootIsObject } from './zod-flags.js';

const RESERVED_COMMANDS = new Set(['help', 'version']);
const COMMAND_WORD = /^[A-Za-z0-9_.][A-Za-z0-9_.-]*$/;

export type FlagKind =
  | 'string'
  | 'integer'
  | 'number'
  | 'boolean'
  | 'enum'
  | 'array'
  | 'json';

export type ItemKind = 'string' | 'integer' | 'number' | 'boolean';

export interface Flag {
  property: string;
  name: string;
  kind: FlagKind;
  required: boolean;
  presence: boolean;
  partial: boolean;
  hasDefault: boolean;
  description?: string;
  defaultValue?: unknown;
  enumValues?: unknown[];
  itemKind?: ItemKind;
}

export type JsonSchema = Record<string, unknown>;

export interface ToolAnnotations {
  title?: string;
  readOnlyHint?: boolean;
  destructiveHint?: boolean;
  idempotentHint?: boolean;
  openWorldHint?: boolean;
}

export interface CliCommand {
  name: string;
  description: string;
  title?: string;
  flags: readonly Flag[];
  source: 'zod' | 'mcp';
  schema?: ZodType;
  inputSchema?: JsonSchema;
  outputSchema?: JsonSchema;
  annotations?: ToolAnnotations;
  mcpName?: string;
  fidelityNotes: readonly string[];
  run: (input: Record<string, unknown>, context?: { signal?: AbortSignal }) => Promise<unknown> | unknown;
}

export interface CliSpec {
  name: string;
  instructions: string;
  version: string;
  commands: readonly CliCommand[];
}

export interface DefineCommandInput {
  name: string;
  description: string;
  run: CliCommand['run'];
  title?: string;
  schema?: ZodType;
  inputSchema?: JsonSchema;
  outputSchema?: JsonSchema;
  annotations?: ToolAnnotations;
  source?: 'zod' | 'mcp';
  mcpName?: string;
  notes?: readonly string[];
}

const validators = new WeakMap<object, (data: unknown) => string | undefined>();

export function commandToken(name: string): string {
  if (COMMAND_WORD.test(name) && !RESERVED_COMMANDS.has(name)) return name;
  let token = name.replace(/[^A-Za-z0-9_.-]+/g, '-').replace(/^-+|\.+$|-+$/g, '');
  if (!COMMAND_WORD.test(token)) token = `tool-${token || 'command'}`;
  if (RESERVED_COMMANDS.has(token)) token = `${token}-command`;
  return token;
}

function assertCommandName(name: string): void {
  if (RESERVED_COMMANDS.has(name)) {
    throw new Error(`Reserved command name: ${name}`);
  }
  if (!COMMAND_WORD.test(name)) {
    throw new Error(`Command name must match ${COMMAND_WORD}: ${name}`);
  }
}

export function defineCommand(input: DefineCommandInput): CliCommand {
  assertCommandName(input.name);
  const source = input.source ?? (input.schema && !input.inputSchema ? 'zod' : 'mcp');
  if (source === 'zod' && !input.schema) {
    throw new Error(`Zod command ${input.name} requires schema`);
  }
  if (source === 'mcp' && !input.inputSchema) {
    throw new Error(`MCP command ${input.name} requires inputSchema`);
  }
  const built = source === 'zod'
    ? flagsFromZod(input.schema as ZodType)
    : flagsFromJsonSchema(input.inputSchema as JsonSchema);
  return {
    name: input.name,
    description: input.description,
    ...(input.title ? { title: input.title } : {}),
    flags: built.flags,
    source,
    ...(input.schema ? { schema: input.schema } : {}),
    ...(input.inputSchema ? { inputSchema: input.inputSchema } : {}),
    ...(input.outputSchema ? { outputSchema: input.outputSchema } : {}),
    ...(input.annotations ? { annotations: input.annotations } : {}),
    ...(input.mcpName ? { mcpName: input.mcpName } : {}),
    fidelityNotes: notesOf([...(input.notes ?? [])], built.notes),
    run: input.run,
  };
}

export function defineCli(input: {
  name: string;
  instructions: string;
  version?: string;
  commands: readonly CliCommand[];
}): CliSpec {
  const seen = new Set<string>();
  for (const command of input.commands) {
    assertCommandName(command.name);
    if (seen.has(command.name)) throw new Error(`Duplicate command: ${command.name}`);
    seen.add(command.name);
  }
  return {
    name: input.name,
    instructions: input.instructions,
    version: input.version ?? '0.0.0',
    commands: [...input.commands],
  };
}

export function commandJsonSchema(command: CliCommand): JsonSchema {
  if (command.source === 'mcp') {
    if (!command.inputSchema) throw new Error(`MCP command ${command.name} has no inputSchema`);
    return command.inputSchema;
  }
  if (!command.schema) throw new Error(`Zod command ${command.name} has no schema`);
  return z.toJSONSchema(command.schema, { io: 'input' }) as JsonSchema;
}

function validatorFor(schema: JsonSchema): (data: unknown) => string | undefined {
  const cached = validators.get(schema);
  if (cached) return cached;
  const ajv = new Ajv2020({ allErrors: true, strict: false, validateSchema: false });
  addFormats(ajv);
  const validate = ajv.compile(schema);
  const run = (data: unknown): string | undefined => {
    if (validate(data)) return undefined;
    const issue = validate.errors?.[0];
    const where = issue?.instancePath ? issue.instancePath : '';
    return `${where} ${issue?.message ?? 'does not match the schema'}`.trim();
  };
  validators.set(schema, run);
  return run;
}

export function parseCommandInput(
  command: CliCommand,
  raw: Record<string, unknown>,
): Record<string, unknown> {
  const value: Record<string, unknown> = { ...raw };
  for (const flag of command.flags) {
    if (value[flag.property] === undefined && flag.hasDefault) {
      value[flag.property] = flag.defaultValue;
    }
  }
  if (command.source === 'zod' && command.schema) {
    const payload = zodRootIsObject(command.schema) ? value : value.input;
    try {
      const parsed = command.schema.parse(payload);
      if (isRecord(parsed)) return parsed;
      return { input: parsed };
    } catch (error) {
      if (error instanceof ZodError) {
        const issue = error.issues[0];
        const path = issue?.path.join('.') ?? '';
        throw new Error(path ? `${path}: ${issue?.message}` : (issue?.message ?? error.message));
      }
      throw error;
    }
  }
  if (!command.inputSchema) throw new Error(`MCP command ${command.name} has no inputSchema`);
  const payload = objectProperties(command.inputSchema) !== undefined ? value : value.input;
  const message = validatorFor(command.inputSchema)(payload);
  if (message) throw new Error(message);
  if (isRecord(payload)) return payload;
  return { input: payload };
}
