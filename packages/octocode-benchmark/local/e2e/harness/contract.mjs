// The generated tool contract the suites validate against (packages/octocode-config/contract).
import fs from 'node:fs';
import path from 'node:path';
import { ROOT } from './paths.mjs';
import { createAjv } from './query-defaults.mjs';

export const contract = JSON.parse(fs.readFileSync(path.join(ROOT, 'packages/octocode-config/contract/tool-contract.json'), 'utf8'));

export function contractValidators() {
  const ajv = createAjv();
  const validatorOf = schemaKey => {
    const compiled = new Map();
    return (tool, value) => {
      if (!compiled.has(tool)) compiled.set(tool, ajv.compile(contract.tools.find(t => t.name === tool)[schemaKey]));
      const validate = compiled.get(tool);
      return validate(value) ? null : `${validate.errors[0].instancePath || '/'} ${validate.errors[0].message}`;
    };
  };
  return { validateInput: validatorOf('inputSchema'), validateOutput: validatorOf('outputSchema') };
}
