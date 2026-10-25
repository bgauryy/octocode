import { isDeepStrictEqual } from 'node:util';
import { inventoryRows, patchNumbersIssue, rawPatch } from '../responses.mjs';
import { addedLines, sha } from './mock-model.mjs';
import { commitJson } from './mock-github-json.mjs';
import { tokenFor } from './surfaces.mjs';

const commitRows = data => data?.commitSha ? [data] : data?.pullRequests?.flatMap(pr => pr.commits ?? []) ?? [];

async function collections(ctx, surface, out, repo) {
  const branch = ctx.mock.model.branchCommits[0];
  const expectedFiles = branch.added.map(file => [branch.oid, file, 'added', addedLines(branch.files[file]).length, 0]);
  for (const responseChars of [50000, 1500]) {
    const report = await ctx.explore('ghGetHistoryItem', { queries: [{ ...repo, operation: 'pullRequest', number: 4, sections: ['commitFiles'] }], responseChars }, surface);
    const commits = report.responses.filter(row => row.tool === 'ghGetHistoryItem').flatMap(row => row.result.json?.results?.flatMap(({ data }) => commitRows(data)) ?? []);
    const observed = commits.flatMap(commit => inventoryRows(commit.files).map(file => [commit.commitSha, file.path, file.status, file.additions, file.deletions]));
    out.push([`advanced.commitFiles.exactOnce.${responseChars}.${surface}`, !report.failures.length && isDeepStrictEqual(observed, expectedFiles)
      && (responseChars !== 1500 || report.responses.length > 1),
    `${observed.length} commit files, ${report.responses.length} calls; ${JSON.stringify(report.failures)}`]);
  }
  const expectedCommits = ctx.mock.model.commits.slice(1).map(commit => [commit.oid, commit.message.split('\n')[0]]);
  for (const responseChars of [50000, 300]) {
    const report = await ctx.explore('ghGetHistoryItem', { queries: [{ ...repo, operation: 'compare', baseRef: ctx.mock.model.commits[0].oid, headRef: 'main', sections: ['commits'] }], responseChars }, surface);
    const observed = report.responses.filter(row => row.tool === 'ghGetHistoryItem').flatMap(row => row.result.json?.results?.flatMap(item => (item.data?.commits ?? []).map(commit => [commit.commitSha, commit.messageHeadline])) ?? []);
    out.push([`advanced.compare.commits.exactOnce.${responseChars}.${surface}`, !report.failures.length && isDeepStrictEqual(observed, expectedCommits)
      && (responseChars !== 300 || report.responses.length > 1),
    `${observed.length} comparison commits, ${report.responses.length} calls; ${JSON.stringify(report.failures)}`]);
  }
}

async function indexedFiles(ctx, surface, out, repo) {
  const branch = ctx.mock.model.branchCommits[0];
  const expected = branch.added.map(file => [1, branch.oid, file]);
  const query = { ...repo, operation: 'pullRequest', number: 4, sections: ['commitFiles'] };
  const report = await ctx.explore('ghGetHistoryItem', {
    queries: [{ ...query, unexpectedField: true }, query], responseChars: 1500,
  }, surface);
  const rows = report.responses.flatMap(response => response.result.json?.results ?? []);
  const rejected = rows.filter(row => row.data?.errorCode);
  const actual = rows.flatMap(row => commitRows(row.data).flatMap(commit =>
    inventoryRows(commit.files).map(file => [row.index, commit.commitSha, file.path])));
  out.push([`advanced.commitFiles.originalIndex.${surface}`, !report.failures.length
    && rejected.length === 1 && rejected[0].index === 0 && rejected[0].data.errorCode === 'invalidInput'
    && rows.filter(row => !row.data?.errorCode).every(row => row.index === 1)
    && report.responses.length > 1 && isDeepStrictEqual(actual, expected),
  JSON.stringify({ files: actual.length, rejected, calls: report.responses.length, failures: report.failures })]);
}

function editFixture(model) {
  const oldLines = Array.from({ length: 36 }, (_, index) => `source ${index + 1}: café 界😀`);
  oldLines[4] += ' needle removed';
  oldLines[11] += ' needle before';
  const replacement = 'replacement: needle after 界', inserted = 'inserted: needle café';
  const events = oldLines.flatMap((text, index) => {
    if (index === 4) return [['-', text]];
    if (index === 11) return [['-', text], ['+', replacement]];
    return index === 19 ? [[' ', text], ['+', inserted]] : [[' ', text]];
  });
  const newLines = events.filter(([sign]) => sign !== '-').map(([, text]) => text);
  const patch = `@@ -1,${oldLines.length} +1,${newLines.length} @@\n${events.map(([sign, text]) => sign + text).join('\n')}\n`;
  const expected = [
    [' ', oldLines[3]], ['-', oldLines[4]], [' ', oldLines[5]],
    [' ', oldLines[10]], ['-', oldLines[11]], ['+', replacement], [' ', oldLines[12]],
    [' ', oldLines[19]], ['+', inserted], [' ', oldLines[20]],
  ].map(([sign, text]) => [(sign === '-' ? oldLines : newLines).indexOf(text) + 1, sign, text]);
  const file = 'src/coordinate-edit.txt';
  const commit = { ...model.head, oid: sha(patch), parent: model.head.oid, message: 'Edit source coordinates', added: [file], files: { [file]: `${newLines.join('\n')}\n` } };
  return { oldLines, newLines, patch, expected, file, commit };
}

function patchWindowsIssue(files) {
  const lengths = files.map(file => Array.from(file.patch).length);
  const total = lengths.reduce((sum, length) => sum + length, 0);
  let offset = 0;
  for (const [index, file] of files.entries()) {
    const page = file.patchPagination;
    if (!page && files.length === 1) return null;
    if (!page || page.unit !== 'chars' || page.offset !== offset || page.length !== lengths[index]
      || page.total !== total || page.hasMore !== (index < files.length - 1)) return 'patch windows overlap, omit characters, or do not terminate';
    offset += lengths[index];
  }
  return null;
}

async function editCoordinates(ctx, surface, out, repo) {
  const fixture = editFixture(ctx.mock.model);
  const token = tokenFor('ghGetHistoryItem', surface);
  const responders = ctx.mock.model.githubResponders;
  const previous = responders.get(token);
  responders.set(token, request => {
    if (request.p !== `/repos/${repo.owner}/${repo.repo}/commits/${fixture.commit.oid}`) return previous?.(request) ?? null;
    if (String(request.headers.accept).includes('vnd.github.sha')) return { body: fixture.commit.oid };
    const body = commitJson(request.base, ctx.mock.model.repos[0], fixture.commit, ctx.mock.model);
    return { body: { ...body, stats: { additions: 2, deletions: 2, total: 4 },
      files: body.files.map(file => ({ ...file, status: 'modified', additions: 2, deletions: 2, changes: 4, patch: fixture.patch })) } };
  });
  try {
    for (const responseChars of [50000, 300]) {
      const query = { ...repo, operation: 'commit', ref: fixture.commit.oid, sections: ['patches'],
        include: [fixture.file], matchString: ['needle'], contextLines: { before: 1, after: 1 } };
      const report = await ctx.explore('ghGetHistoryItem', { queries: [query], responseChars }, surface);
      const commits = report.responses.flatMap(response => response.result.json?.results?.flatMap(row => commitRows(row.data)) ?? []);
      const files = commits.flatMap(commit => inventoryRows(commit.files)).filter(file => file.patch !== undefined);
      const actual = files.flatMap(file => file.patch.split('\n').flatMap(line => {
        const match = /^(\d+)\t([ +-])(.*)$/.exec(line);
        return match ? [[Number(match[1]), match[2], match[3]]] : [];
      }));
      const coordinateErrors = [patchWindowsIssue(files), patchNumbersIssue(files.map(file => file.patch).join(''), fixture)].filter(Boolean);
      out.push([`advanced.commit.patchCoordinates.${responseChars}.${surface}`, !report.failures.length
        && commits.every(commit => commit.commitSha === fixture.commit.oid)
        && files.every(file => file.path === fixture.file) && !coordinateErrors.length
        && isDeepStrictEqual(actual, fixture.expected) && (responseChars !== 300 || report.responses.length > 1),
      JSON.stringify({ actual, expected: fixture.expected, coordinateErrors, calls: report.responses.length, failures: report.failures })]);
    }
  } finally {
    if (previous) responders.set(token, previous); else responders.delete(token);
  }
}

export async function advancedHistoryForms(ctx) {
  const out = [];
  const { owner, name } = ctx.mock.model.repos[0];
  const repo = { owner, repo: name };
  const source = addedLines(ctx.mock.model.files['src/greeter.ts']);
  const expected = [6, 7, 8, 14, 15, 16].map(line => `+${source[line - 1]}`);
  for (const surface of ctx.surfaces) {
    await collections(ctx, surface, out, repo);
    await indexedFiles(ctx, surface, out, repo);
    await editCoordinates(ctx, surface, out, repo);
    for (const target of [
      { operation: 'commit', ref: 'main' },
      { operation: 'compare', baseRef: ctx.mock.model.commits[0].oid, headRef: 'main' },
    ]) for (const responseChars of [50000, 300]) {
      const query = { ...repo, ...target, sections: ['patches'], include: ['src/greeter.ts'], matchString: ['greeter-1', 'greeter-2'], contextLines: { before: 1, after: 1 } };
      const report = await ctx.explore('ghGetHistoryItem', { queries: [query], responseChars }, surface);
      const patches = report.responses.filter(row => row.tool === 'ghGetHistoryItem').flatMap(row => row.result.json?.results?.flatMap(item => inventoryRows(item.data?.files ?? item.data?.commits?.[0]?.files)) ?? []).filter(file => file.patch !== undefined);
      const lines = patches.flatMap(file => rawPatch(file.patch).split('\n').filter(line => line.startsWith('+')));
      out.push([`advanced.${target.operation}.patchMatch.${responseChars}.${surface}`, !report.failures.length
        && patches.every(file => file.path === 'src/greeter.ts') && isDeepStrictEqual(lines, expected)
        && (responseChars !== 300 || report.responses.length > 1),
      `${lines.length} selected/context patch lines, ${report.responses.length} calls; ${JSON.stringify(report.failures)}`]);
    }
  }
  return out;
}
