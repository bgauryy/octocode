import fs from 'node:fs';
import path from 'node:path';

const data = result => result.json?.results?.[0]?.data;
const input = query => ({ queries: [query] });

export const ADVANCED_GRAPH = {
  graphBuild: async ctx => {
    const initial = data(await ctx.cliCall('graphBuild', input({ keep: 10 })));
    const reused = data(await ctx.cliCall('graphBuild', input({ keep: 10 })));
    const checks = [
      ['graph.reuse.identity', initial.id === reused.id && reused.reused === true, JSON.stringify(reused)],
      ['graph.storage.workspace', fs.realpathSync(path.resolve(ctx.cwd, initial.dir)).startsWith(path.join(fs.realpathSync(ctx.cwd), '.octocode/graph') + path.sep), initial.dir],
    ];
    fs.writeFileSync(path.join(ctx.cwd, 'graph-health.ts'), 'export const graphHealthAdded = 1;\n');
    const stale = data(await ctx.cliCall('graphQuery', input({ operation: 'stats', graph: initial.id })));
    checks.push(['graph.stale.after-add', stale.freshness?.fresh === false, JSON.stringify(stale.freshness)]);
    const rebuilt = data(await ctx.cliCall('graphBuild', input({ keep: 10 })));
    checks.push(['graph.rebuild.changed', rebuilt.id !== initial.id && !rebuilt.reused, rebuilt.id]);
    const drift = data(await ctx.cliCall('graphQuery', input({ operation: 'drift', graph: rebuilt.id, baseline: initial.id })));
    checks.push(['graph.drift.added', JSON.stringify(drift).includes('graph-health.ts'), JSON.stringify(drift)]);
    return checks;
  },
  graphQuery: async ctx => {
    const snapshot = data(await ctx.cliCall('graphBuild', input({ keep: 10 })));
    const files = data(await ctx.cliCall('graphQuery', input({ operation: 'find', target: '.ts', kind: 'file' })));
    const symbols = data(await ctx.cliCall('graphQuery', input({ operation: 'find', target: 'greet', kind: 'symbol' })));
    const file = files.results[0].id;
    const symbol = symbols.results[0].id;
    const checks = [];
    for (const operation of ['stats', 'find', 'node', 'symbols', 'deps', 'dependents', 'callers', 'callees', 'path', 'walk', 'cycles', 'hubs', 'diagnostics', 'stale', 'impact', 'drift']) {
      const query = { operation, graph: snapshot.id };
      if (['node', 'callers', 'callees'].includes(operation)) query.target = symbol;
      if (['symbols', 'deps', 'dependents', 'walk', 'impact', 'path'].includes(operation)) query.target = file;
      if (operation === 'find') query.target = 'greet';
      if (operation === 'path') query.to = file;
      if (operation === 'drift') query.baseline = snapshot.id;
      const result = await ctx.cliCall('graphQuery', input(query));
      checks.push([`graph.operation.${operation}`, !result.isError && data(result)?.graph?.id === snapshot.id, result.text]);
    }
    return checks;
  },
};
