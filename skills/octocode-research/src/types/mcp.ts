interface JsonSchemaProperty {
  type: string;
  description?: string;
  items?: JsonSchemaProperty;
  properties?: Record<string, JsonSchemaProperty>;
  required?: string[];
  minItems?: number;
  maxItems?: number;
  minimum?: number;
  maximum?: number;
  minLength?: number;
  maxLength?: number;
  enum?: (string | number | boolean)[];
  default?: unknown;
}

interface JsonSchema {
  $schema?: string;
  type: 'object';
  properties: Record<string, JsonSchemaProperty>;
  required?: string[];
  additionalProperties?: boolean;
}


export function transformToJsonSchema(
  schemaDescriptions: Record<string, string>,
  toolName: string
): JsonSchema {
  const properties: Record<string, JsonSchemaProperty> = {};
  
  for (const [key, description] of Object.entries(schemaDescriptions)) {
    properties[key] = {
      type: 'string',
      description,
    };
  }

  return {
    $schema: 'http://json-schema.org/draft-07/schema#',
    type: 'object',
    properties: {
      queries: {
        type: 'array',
        description: `Research queries for ${toolName} (1-3 queries per call for optimal resource management). Review schema before use for optimal results`,
        items: {
          type: 'object',
          properties,
          required: ['mainResearchGoal', 'researchGoal', 'reasoning'],
        },
        minItems: 1,
        maxItems: 3,
      },
    },
    required: ['queries'],
  };
}
