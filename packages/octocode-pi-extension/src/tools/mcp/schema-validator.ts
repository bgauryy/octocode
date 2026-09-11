import { Compile } from 'typebox/compile';
import { ErrorContext, ErrorSchema, Stack } from 'typebox/schema';
import { Locale, Settings } from 'typebox/system';

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
    throw new McpSchemaUnsupportedError(
      `MCP schema is not serializable: ${(error as Error).message}`
    );
  }
  if (text === undefined)
    throw new McpSchemaUnsupportedError('MCP schema must be a JSON value');
  if (text.length > MAX_SCHEMA_CHARS)
    throw new McpSchemaUnsupportedError(
      `MCP schema is too large (${text.length} characters)`
    );
  return text;
}

function assertSupportedDialect(schema: unknown): void {
  if (!isRecord(schema) || schema['$schema'] === undefined) return;
  const dialect = schema['$schema'];
  if (typeof dialect !== 'string' || !SUPPORTED_DIALECTS.has(dialect)) {
    throw new McpSchemaUnsupportedError(
      `Unsupported JSON Schema dialect: ${String(dialect)}`
    );
  }
}

function clip(value: unknown): string {
  const text =
    typeof value === 'string' ? value : String(value ?? 'invalid value');
  return text.length <= MAX_ERROR_TEXT_CHARS
    ? text
    : `${text.slice(0, MAX_ERROR_TEXT_CHARS - 1)}…`;
}

/** Keep validation strict, but hide errors belonging to a different discriminated branch. */
function irrelevantUnionBranches(
  schema: unknown,
  value: unknown
): Array<{ schemaPath: string; instancePath: string }> {
  const excluded: Array<{ schemaPath: string; instancePath: string }> = [];
  const visit = (
    node: unknown,
    input: unknown,
    schemaPath: string,
    instancePath: string
  ): void => {
    if (!isRecord(node)) return;
    for (const keyword of ['anyOf', 'oneOf', 'allOf']) {
      const branches = node[keyword];
      if (!Array.isArray(branches)) continue;
      let selected = branches.map((_, index) => index);
      if (keyword !== 'allOf' && isRecord(input) && branches.every(isRecord)) {
        const firstProperties = branches[0]?.['properties'];
        if (isRecord(firstProperties)) {
          // A discriminator must be constrained in every branch and present in this input.
          const keys = Object.keys(firstProperties).filter(
            key =>
              Object.hasOwn(input, key) &&
              branches.every(branch => {
                const properties = branch['properties'];
                const field = isRecord(properties)
                  ? properties[key]
                  : undefined;
                return isRecord(field) && Object.hasOwn(field, 'const');
              })
          );
          if (keys.length > 0) {
            const matches = selected.filter(index =>
              keys.every(key => {
                const properties = branches[index]!['properties'] as Record<
                  string,
                  Record<string, unknown>
                >;
                return Object.is(properties[key]!['const'], input[key]);
              })
            );
            // Unknown discriminator values still need all branch diagnostics.
            if (matches.length > 0) selected = matches;
          }
        }
      }
      branches.forEach((branch, index) => {
        const branchPath = `${schemaPath}/${keyword}/${index}`;
        if (selected.includes(index))
          visit(branch, input, branchPath, instancePath);
        else excluded.push({ schemaPath: branchPath, instancePath });
      });
    }
    if (isRecord(node['properties']) && isRecord(input)) {
      for (const [key, property] of Object.entries(node['properties'])) {
        // Match TypeBox's diagnostic path spelling, including literal property names.
        if (Object.hasOwn(input, key))
          visit(
            property,
            input[key],
            `${schemaPath}/properties/${key}`,
            `${instancePath}/${key}`
          );
      }
    }
    if (isRecord(node['items']) && Array.isArray(input)) {
      input.forEach((item, index) =>
        visit(
          node['items'],
          item,
          `${schemaPath}/items`,
          `${instancePath}/${index}`
        )
      );
    }
  };
  visit(schema, value, '#', '');
  return excluded;
}

function withinPointer(pointer: string, prefix: string): boolean {
  return pointer === prefix || pointer.startsWith(`${prefix}/`);
}

export function compileMcpSchemaValidator(
  schema: unknown
): McpCompiledSchemaValidator {
  schemaText(schema);
  assertSupportedDialect(schema);

  let validator: ReturnType<typeof Compile>;
  try {
    validator = Compile(schema as Parameters<typeof Compile>[0]);
  } catch (error) {
    throw new McpSchemaUnsupportedError(
      `Unsupported MCP input schema: ${(error as Error).message}`
    );
  }

  return {
    validate(value: unknown): McpSchemaValidationResult {
      if (validator.Check(value)) return { valid: true, errors: [] };
      // Filter before applying our limit so unrelated union branches cannot displace
      // diagnostics for the operation the caller selected.
      const excluded = irrelevantUnionBranches(schema, value);
      const context = new ErrorContext();
      const inputSchema = schema as Parameters<typeof ErrorSchema>[4];
      // ErrorContext buffers at most Settings maxErrors (default 8) BEFORE the
      // irrelevant-union-branch filter below, which could drop the diagnostics
      // for the operation the caller selected. Raise the buffer bound during
      // collection — bounded, so exhaustive diagnostics still cannot balloon.
      const previousMaxErrors = Settings.Get().maxErrors;
      Settings.Set({ maxErrors: 256 });
      try {
        ErrorSchema(
          Stack({}, inputSchema),
          context,
          '#',
          '',
          inputSchema,
          value
        );
      } finally {
        Settings.Set({ maxErrors: previousMaxErrors });
      }
      const collected = context
        .GetErrors()
        .filter(
          error =>
            !excluded.some(
              branch =>
                withinPointer(error.schemaPath, branch.schemaPath) &&
                withinPointer(error.instancePath, branch.instancePath)
            )
        )
        .slice(0, MAX_ERRORS);
      const localize = Locale.Get();
      const rawErrors = collected
        .flatMap<McpSchemaValidationError>(error => {
          const message = localize(error);
          // TypeBox groups unexpected properties at the containing object. Expose
          // each offending argument so callers can repair the exact field.
          if (error.keyword === 'additionalProperties') {
            return error.params.additionalProperties.map(key => ({
              ...error,
              message,
              instancePath: `${error.instancePath}/${key}`,
              schemaPath: `${error.schemaPath}/additionalProperties`,
            }));
          }
          return [{ ...error, message }];
        })
        .slice(0, MAX_ERRORS);
      const errors = rawErrors.map(error => ({
        keyword: clip(error.keyword),
        instancePath: clip(error.instancePath),
        schemaPath: clip(error.schemaPath),
        message: clip(error.message),
      }));
      if (errors.length === 0) {
        errors.push({
          keyword: 'schema',
          instancePath: '',
          schemaPath: '#',
          message: 'arguments do not match the MCP input schema',
        });
      }
      return { valid: false, errors };
    },
  };
}
