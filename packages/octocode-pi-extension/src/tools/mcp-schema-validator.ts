import { Compile } from 'typebox/compile';

const MAX_SCHEMA_CHARS = 256 * 1024;
const MAX_ERRORS = 8;
const MAX_ERROR_TEXT_CHARS = 240;
const SUPPORTED_DIALECTS = new Set([
  'http://json-schema.org/draft-03/schema#',
  'http://json-schema.org/draft-04/schema#',
  'http://json-schema.org/draft-06/schema#',
  'http://json-schema.org/draft-07/schema#',
  'https://json-schema.org/draft/2019-09/schema',
  'https://json-schema.org/draft/2019-09/schema#',
  'https://json-schema.org/draft/2020-12/schema',
  'https://json-schema.org/draft/2020-12/schema#',
]);

export interface McpSchemaValidationError {
  keyword: string;
  instancePath: string;
  schemaPath: string;
  message: string;
}

export type McpSchemaValidationResult =
  | { valid: true; errors: [] }
  | { valid: false; errors: McpSchemaValidationError[] };

export interface McpCompiledSchemaValidator {
  validate(value: unknown): McpSchemaValidationResult;
}

export class McpSchemaUnsupportedError extends Error {
  readonly code = 'SCHEMA_UNSUPPORTED';

  constructor(message: string) {
    super(message);
    this.name = 'McpSchemaUnsupportedError';
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function schemaText(schema: unknown): string {
  let text: string | undefined;
  try {
    text = JSON.stringify(schema);
  } catch (error) {
    throw new McpSchemaUnsupportedError(`MCP schema is not serializable: ${(error as Error).message}`);
  }
  if (text === undefined) throw new McpSchemaUnsupportedError('MCP schema must be a JSON value');
  if (text.length > MAX_SCHEMA_CHARS) throw new McpSchemaUnsupportedError(`MCP schema is too large (${text.length} characters)`);
  return text;
}

function assertSupportedDialect(schema: unknown): void {
  if (!isRecord(schema) || schema['$schema'] === undefined) return;
  const dialect = schema['$schema'];
  if (typeof dialect !== 'string' || !SUPPORTED_DIALECTS.has(dialect)) {
    throw new McpSchemaUnsupportedError(`Unsupported JSON Schema dialect: ${String(dialect)}`);
  }
}

function clip(value: unknown): string {
  const text = typeof value === 'string' ? value : String(value ?? 'invalid value');
  return text.length <= MAX_ERROR_TEXT_CHARS ? text : `${text.slice(0, MAX_ERROR_TEXT_CHARS - 1)}…`;
}

export function compileMcpSchemaValidator(schema: unknown): McpCompiledSchemaValidator {
  schemaText(schema);
  assertSupportedDialect(schema);

  let validator: ReturnType<typeof Compile>;
  try {
    validator = Compile(schema as Parameters<typeof Compile>[0]);
  } catch (error) {
    throw new McpSchemaUnsupportedError(`Unsupported MCP input schema: ${(error as Error).message}`);
  }

  return {
    validate(value: unknown): McpSchemaValidationResult {
      if (validator.Check(value)) return { valid: true, errors: [] };
      const rawErrors = validator.Errors(value).slice(0, MAX_ERRORS);
      const errors = rawErrors.map((error) => ({
        keyword: clip(error.keyword),
        instancePath: clip(error.instancePath),
        schemaPath: clip(error.schemaPath),
        message: clip(error.message),
      }));
      if (errors.length === 0) {
        errors.push({ keyword: 'schema', instancePath: '', schemaPath: '#', message: 'arguments do not match the MCP input schema' });
      }
      return { valid: false, errors };
    },
  };
}
