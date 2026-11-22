import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { writeNewFile } from './output.js';
import { adapters, list, record, string } from './providers.js';
import type { Observation } from './providers.js';

const hash = (bytes: Buffer) =>
  createHash('sha256').update(bytes).digest('hex');

/** Materialize an explicitly attested set of provider artifacts alongside the source graph. */
export async function enrichExport(
  baseFile: string,
  manifestFile: string,
  outputFile: string
) {
  const bytes = await readFile(baseFile);
  const manifest = record(JSON.parse(await readFile(manifestFile, 'utf8')));
  const revision = string(manifest.revision, 'revision');
  if (hash(bytes) !== manifest.baseSha256)
    throw new Error('Snapshot digest mismatch');
  const data = record(JSON.parse(bytes.toString('utf8')));
  list(data.files, 'graph files');
  if (data.enrichment)
    throw new Error('Start from the original export to replace its enrichment');
  const edges: Observation[] = [],
    kinds = new Set<string>();
  const providers = [];
  for (const item of list(manifest.providers, 'providers')) {
    const p = record(item),
      format = string(p.format, 'format'),
      file = string(p.file, 'file');
    if (p.revision !== revision)
      throw new Error(`Provider revision mismatch: ${file}`);
    if (!Object.hasOwn(adapters, format))
      throw new Error(`Unsupported provider ${format}`);
    const input = await readFile(resolve(dirname(manifestFile), file));
    if (hash(input) !== p.sha256)
      throw new Error(`Provider digest mismatch: ${file}`);
    const adapter = adapters[format as keyof typeof adapters];
    const sourceRoot = string(p.sourceRoot ?? data.section, 'sourceRoot');
    const workload =
      p.workload === undefined ? undefined : string(p.workload, 'workload');
    const testId =
      p.testId === undefined ? undefined : string(p.testId, 'testId');
    let moduleSha256: string | undefined;
    if (format === 'llvm-graphviz') {
      const moduleFile = string(p.moduleFile, 'moduleFile');
      moduleSha256 = hash(
        await readFile(resolve(dirname(manifestFile), moduleFile))
      );
      if (moduleSha256 !== p.moduleSha256)
        throw new Error(`LLVM module digest mismatch: ${moduleFile}`);
    }
    const document = record(JSON.parse(input.toString('utf8')));
    const parsed = adapter.read(document, {
      artifact: file,
      sourceRoot,
      workload,
      testId,
      moduleSha256,
    });
    for (const edge of parsed) edges.push(edge);
    const supported =
      'kindsFor' in adapter ? adapter.kindsFor(document) : adapter.kinds;
    for (const kind of supported) kinds.add(kind);
    providers.push({
      format,
      kinds: supported,
      file,
      revision,
      sha256: p.sha256,
      sourceRoot,
      workload,
      testId,
      ...(moduleSha256 && { moduleSha256, moduleFile: p.moduleFile }),
      relations: parsed.length,
    });
  }
  data.enrichment = {
    revision,
    baseSha256: manifest.baseSha256,
    kinds: [...kinds],
    providers,
    relations: edges,
  };
  await writeNewFile(outputFile, JSON.stringify(data));
}
