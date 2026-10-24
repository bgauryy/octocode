import test from 'node:test';
import assert from 'node:assert/strict';
import { graphqlAnswer } from './mock-graphql.mjs';

const files = { 'src/a.ts': 'a', 'src/nested/b.ts': 'b' };
const first = { oid: 'first', date: '2025-01-01T00:00:00Z', added: ['src/a.ts'] };
const head = { oid: 'head', date: '2025-01-02T00:00:00Z', added: ['src/nested/b.ts'], parent: first.oid };
const model = { files, head, commits: [first, head], branchCommits: [], repos: [{ owner: 'o', name: 'r', files }] };
const query = 'query($owner:String!,$name:String!,$oid:GitObjectID!,$p0:String!){repository(owner:$owner,name:$name){object(oid:$oid){...on Commit{committedDate p0:history(first:1,path:$p0){nodes{committedDate author{name user{login}}}}}}}}';

test('path dates follow the selected commit ancestry, including directory paths and absent history', () => {
  const answer = graphqlAnswer(model, query, { owner: 'o', name: 'r', oid: 'head', p0: 'src/a.ts', p1: 'src', p2: 'absent' });
  assert.equal(answer.data.repository.object.committedDate, head.date);
  assert.equal(answer.data.repository.object.p0.nodes[0].committedDate, first.date);
  assert.equal(answer.data.repository.object.p1.nodes[0].committedDate, head.date);
  assert.deepEqual(answer.data.repository.object.p2.nodes, []);
  const earlier = graphqlAnswer(model, query, { owner: 'o', name: 'r', oid: 'first', p0: 'src/nested/b.ts' });
  assert.deepEqual(earlier.data.repository.object.p0.nodes, []);
});
