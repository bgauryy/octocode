// The authored config contract. `contractFromDeclaration` is the only way it becomes data.
import { contractSchema, type Contract, type ContractInput } from '../schema.ts';
import { environment } from './environment.ts';
import { sections } from './sections.ts';

/** Validates the declaration against the schema and returns it with its authored key order. */
export function contractFromDeclaration(): Contract {
  const declaration: ContractInput & Contract = {
    $schema: './config-contract.schema.json',
    config: { fileName: '.octocoderc', schemaVersion: 1, runtimeSurfaces: ['mcp', 'cli'] },
    environment,
    sections,
  };
  contractSchema.parse(declaration);
  return declaration;
}
