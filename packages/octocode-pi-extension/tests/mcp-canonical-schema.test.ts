import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import {
  DIRECT_TOOL_DEFINITIONS,
  buildDirectToolCommandPatterns,
  prepareDirectToolInput,
} from '@octocodeai/octocode-core/schema';
import { compileMcpSchemaValidator } from '../src/tools/mcp/schema-validator.js';

describe('canonical research schemas through the Pi MCP gateway validator', () => {
  it.each(DIRECT_TOOL_DEFINITIONS)(
    '$name accepts its executable examples and rejects unknown query fields',
    tool => {
      const validator = compileMcpSchemaValidator(
        z.toJSONSchema(tool.inputSchema, { io: 'input' })
      );
      for (const example of buildDirectToolCommandPatterns(tool.name)) {
        const input = prepareDirectToolInput(tool.name, example.query);
        const validation = validator.validate(input);
        expect(validation, example.label).toEqual({ valid: true, errors: [] });
      }
      expect(validator.validate({ queries: [{ unexpected: true }] }).valid).toBe(
        false
      );
    }
  );
});
