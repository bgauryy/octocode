// The generated tool contract the suites validate against (packages/octocode-config/contract).
import fs from 'node:fs';
import path from 'node:path';
import { ROOT } from './paths.mjs';
import { createAjv } from './query-defaults.mjs';

export const contract = JSON.parse(fs.readFileSync(path.join(ROOT, 'packages/octocode-config/contract/tool-contract.json'), 'utf8'));
const presentation = JSON.parse(fs.readFileSync(path.join(ROOT, 'packages/octocode-config/contract/cli-presentation.json'), 'utf8'));
const publicTools = new Map(presentation.tools.map(tool => [tool.name, tool]));

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
  return { validateInput: validatorOf(publicInput), validateOutput: validatorOf(name => contract.tools.find(tool => tool.name === name).outputSchema) };
}
