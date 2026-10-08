import Ajv2020 from 'ajv/dist/2020.js';
import { type ZodType } from 'zod';
import { isCliView } from './run.js';
import { commandJsonSchema, type CliCommand, type CliSpec, type JsonSchema, type ToolAnnotations } from './spec.js';

export interface McpToolDefinition {
  name: string;
  description: string;
  inputSchema: JsonSchema;
  title?: string;
  outputSchema?: JsonSchema;
  annotations?: ToolAnnotations;
}

export interface McpExport {
  instructions: string;
  tools: McpToolDefinition[];
}

export interface ToolRegistrar {
  registerTool(
    name: string,
    config: {
      description?: string;
      inputSchema?: unknown;
      title?: string;
      outputSchema?: unknown;
      annotations?: ToolAnnotations;
    },
    callback: (args: Record<string, unknown>, context?: { signal?: AbortSignal; mcpReq?: { signal?: AbortSignal } }) => Promise<{
      content: Array<{ type: 'text'; text: string }>;
      structuredContent?: Record<string, unknown>;
    }>,
  ): unknown;
}

function protocolFields(command: CliCommand): Pick<McpToolDefinition, 'title' | 'outputSchema' | 'annotations'> {
  return {
    ...(command.title ? { title: command.title } : {}),
    ...(command.outputSchema ? { outputSchema: command.outputSchema } : {}),
    ...(command.annotations ? { annotations: command.annotations } : {}),
  };
}

export function cliToMcp(spec: CliSpec): McpExport {
  return {
    instructions: spec.instructions,
    tools: spec.commands.map(command => {
      return {
        name: command.mcpName ?? command.name,
        description: command.description,
        inputSchema: commandJsonSchema(command),
        ...protocolFields(command),
      };
    }),
  };
}

export function jsonSchemaStandard(schema: JsonSchema): { '~standard': Record<string, unknown> } {
  const ajv = new Ajv2020({ allErrors: true, strict: false, validateSchema: false });
  const validate = ajv.compile(schema);
  return {
    '~standard': {
      version: 1,
      vendor: 'octocode-mcp-cli',
      validate: (value: unknown) => {
        if (validate(value)) return { value };
        const issues = (validate.errors ?? []).map(error => ({
          message: error.message ?? 'invalid',
          ...(error.instancePath ? { path: error.instancePath.split('/').filter(Boolean) } : {}),
        }));
        return { issues };
      },
      jsonSchema: {
        input: () => schema,
        output: () => schema,
      },
    },
  };
}

export function mcpServerOptions(spec: CliSpec): { instructions: string } {
  return { instructions: spec.instructions };
}

function toolText(result: unknown): string {
  if (isCliView(result)) return result.text;
  if (typeof result === 'string') return result;
  if (result === undefined) return '';
  return JSON.stringify(result);
}

function toolMessage(result: unknown): { content: Array<{ type: 'text'; text: string }>; structuredContent?: Record<string, unknown> } {
  if (isCliView(result) && result.json && typeof result.json === 'object' && !Array.isArray(result.json)) {
    const structured = (result.json as { structuredContent?: unknown }).structuredContent;
    if (structured && typeof structured === 'object' && !Array.isArray(structured)) {
      return {
        content: [{ type: 'text', text: result.text }],
        structuredContent: structured as Record<string, unknown>,
      };
    }
  }
  return { content: [{ type: 'text', text: toolText(result) }] };
}

export function registerOn(server: ToolRegistrar, spec: CliSpec): void {
  for (const command of spec.commands) {
    const inputSchema: ZodType | ReturnType<typeof jsonSchemaStandard> = command.source === 'zod'
      ? command.schema as ZodType
      : jsonSchemaStandard(command.inputSchema ?? { type: 'object', properties: {} });
    const outputSchema = command.outputSchema ? jsonSchemaStandard(command.outputSchema) : undefined;
    server.registerTool(
      command.mcpName ?? command.name,
      {
        description: command.description,
        inputSchema,
        ...(command.title ? { title: command.title } : {}),
        ...(outputSchema ? { outputSchema } : {}),
        ...(command.annotations ? { annotations: command.annotations } : {}),
      },
      async (args, context) => toolMessage(await command.run(args ?? {}, { signal: context?.signal ?? context?.mcpReq?.signal })),
    );
  }
}
