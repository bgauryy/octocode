// Serializes the contract declaration and its Zod schema to the committed JSON files.
import { z } from 'zod';
import { contractFromDeclaration } from './declaration/index.ts';
import { CONTRACT_SCHEMA_ID, CONTRACT_SCHEMA_TITLE, contractSchema } from './schema.ts';

export function renderContractJson(): string {
  return `${JSON.stringify(contractFromDeclaration(), null, 2)}\n`;
}

export function renderContractSchema(): string {
  const { $schema, ...rest } = z.toJSONSchema(contractSchema, { target: 'draft-2020-12' });
  return `${JSON.stringify({ $schema, $id: CONTRACT_SCHEMA_ID, title: CONTRACT_SCHEMA_TITLE, ...rest }, null, 2)}\n`;
}
