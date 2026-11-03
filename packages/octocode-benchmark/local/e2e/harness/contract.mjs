// The generated tool contract the suites validate against (packages/octocode-config/contract).
import fs from 'node:fs';
import path from 'node:path';
import { ROOT } from './paths.mjs';
import { createAjv } from './query-defaults.mjs';

const stored = JSON.parse(fs.readFileSync(path.join(ROOT, 'packages/octocode-config/contract/tool-contract.json'), 'utf8'));
const withSharedDefs = schema => ({ ...schema, $defs: { ...stored.$defs, ...schema.$defs } });
// The harness walks schema roots directly; give each root the same definition
// view native's shared-reference resolvers provide, without changing the IR.
export const contract = {
  ...stored,
  tools: stored.tools.map(tool => ({
    ...tool,
    ...Object.fromEntries(['querySchema', 'inputSchema', 'outputSchema'].map(key => [key, withSharedDefs(tool[key])])),
  })),
};
const presentation = JSON.parse(fs.readFileSync(path.join(ROOT, 'packages/octocode-config/contract/cli-presentation.json'), 'utf8'));
const publicTools = new Map(presentation.tools.map(tool => [tool.name, tool]));

const contractTool = name => contract.tools.find(tool => tool.name === name);

export function contractValidators({ selection = true, precompile = false } = {}) {
  const ajv = createAjv();
  const validatorOf = schemaOf => {
    const compiled = new Map();
    const validator = tool => {
      if (!compiled.has(tool)) compiled.set(tool, ajv.compile(schemaOf(tool)));
      return compiled.get(tool);
    };
    if (precompile) for (const { name } of contract.tools) validator(name);
    return (tool, value) => {
      const validate = validator(tool);
      return validate(value) ? null : `${validate.errors[0].instancePath || '/'} ${validate.errors[0].message}`;
    };
  };
  const publicInput = name => {
    const tool = publicTools.get(name);
    return (selection && tool.selection ? tool.selection.mcp : tool.mcp).inputSchema;
  };
  const listed = validatorOf(publicInput);
  const exact = validatorOf(name => contractTool(name).inputSchema);
  return {
    validateInput: listed,
    // What native rejects before any work: the listed schema (no private state) plus
    // the exact form union (a superset's gaps: a field outside its form, a cursor mix).
    validateExactInput: (tool, value) => listed(tool, value) ?? exact(tool, value),
    validateOutput: validatorOf(name => contractTool(name).outputSchema),
  };
}
