/**
 * Canonical source used by the signaturesOnly alignment tests on both the
 * GitHub terminal (fileOperations.processContent.test.ts) and the local
 * terminal (local_fetch_content.test.ts). Both must return exactly
 * extractSignatures(SIGNATURE_SOURCE) — that equality IS the alignment contract.
 */
export const SIGNATURE_SOURCE = [
  "import { A } from './a';",
  '',
  'export interface Foo {',
  '  id: string;',
  '}',
  '',
  'export async function doThing(',
  '  a: string,',
  '): Promise<void> {',
  '  const secretLocal = 1;',
  '  return use(secretLocal);',
  '}',
  '',
].join('\n');
