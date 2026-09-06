import { describe, expect, it } from 'vitest';
import { build } from 'esbuild';
import { resolve } from 'node:path';

describe('direct entry dependency loading', () => {
  it('keeps unrelated execution handlers outside the initial static import graph', async () => {
    const config = await import(
      new URL('../../buildConfig.mjs', import.meta.url).href
    );
    const entry = config.entryPoints.find((item: { entryPoints: string[] }) =>
      item.entryPoints.includes('src/direct.ts')
    );
    const result = await build({
      ...config.sharedBuildOptions,
      ...entry,
      write: false,
      metafile: true,
      logLevel: 'silent',
    });
    const outputs = result.metafile!.outputs;
    const direct = Object.entries(outputs).find(
      ([, output]) => output.entryPoint === 'src/direct.ts'
    )![0];
    const seen = new Set<string>();
    const visit = (file: string) => {
      if (seen.has(file)) return;
      seen.add(file);
      for (const item of outputs[file]!.imports) {
        if (!item.external && item.kind !== 'dynamic-import') visit(item.path);
      }
    };
    visit(direct);
    const initialInputs = [...seen].flatMap(file =>
      Object.keys(outputs[file]!.inputs)
    );
    expect(initialInputs).not.toContain(
      'src/tools/local_analyze_graph/execution.ts'
    );
    expect(initialInputs).not.toContain(
      'src/tools/package_search/execution.ts'
    );
    expect(
      Object.values(outputs).some(
        output => 'src/tools/local_analyze_graph/execution.ts' in output.inputs
      )
    ).toBe(true);
    expect(
      Object.keys(outputs).every(file =>
        resolve(file).startsWith(resolve('dist') + '/')
      )
    ).toBe(true);
  });
});
