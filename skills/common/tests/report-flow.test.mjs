import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { buildReport, manifestPage } from '../scripts/lib/report/render.mjs';
// Paths from this file, so the test runs from the repo root or its package.
const skillsRoot = resolve(import.meta.dirname, '../..');
const renderer = resolve(skillsRoot, 'common/scripts/render-report.mjs');
function workspace(t) { const root=mkdtempSync(join(tmpdir(),'report-flow-'));t.after(()=>rmSync(root,{recursive:true,force:true}));return root; }
const today = new Date().toLocaleDateString('en-CA');
for (const entry of readdirSync(skillsRoot)) {
  let source;try {source=readFileSync(join(skillsRoot,entry,'output.md'),'utf8');}catch {continue;}
  const match=source.match(/<!-- report-contract ([\s\S]*?) -->/);if(!match) continue;
  const contract=JSON.parse(match[1]);
  test(`${entry}: explicit contract rejects missing sections, including headings inside fences`,t=>{
    assert.deepEqual(source.match(/^status: (.+)$/m)[1].split(' | '),contract.statuses);
    const root=workspace(t),dir=join(root,'.octocode',entry);mkdirSync(dir,{recursive:true});const file=join(dir,`${today}-report.md`);
    const sections=[...contract.required,...contract.anyOf.map(g=>g[0])];
    const header=`---\nstatus: ${contract.statuses[0]}\nquestion: Q?\n---\n# Report\n\nAnswer.\n\n`;
    writeFileSync(file,header+sections.map(h=>`## ${h}\n\nEvidence.\n`).join('\n'));
    assert.deepEqual(buildReport(file,{workspace:root}).warnings,[]);
    for (const heading of sections) {
      writeFileSync(file,header+sections.filter(h=>h!==heading).map(h=>`## ${h}\n\nEvidence.\n`).join('\n')+`\n\n\`\`\`md\n## ${heading}\n\`\`\`\n`);
      assert.ok(buildReport(file,{workspace:root}).warnings.some(w=>w.includes(`## ${heading}`)),heading);
    }
  });
}
test('compact discovery pages reach all matches once and full output is explicit',t=>{
  const root=workspace(t);for(let i=0;i<27;i++)writeFileSync(join(root,`${i}.md`),`# Topic ${i}\n\nContent.`);
  const first=manifestPage(root), second=manifestPage(root,{offset:20});
  assert.equal(first.count,20);assert.equal(second.count,7);assert.equal(first.total,27);assert.equal(second.next,null);
  assert.equal(new Set([...first.documents,...second.documents].map(d=>d.path)).size,27);
  assert.deepEqual(first.next,[root,'--manifest','--limit','20','--offset','20']);
  assert.equal(first.documents[0].headings,undefined);
  assert.equal(manifestPage(root,{all:true}).documents.length,27);
  assert.equal(manifestPage(root,{query:'Topic 26'}).count,1);
});
test('strict CLI refuses bad data without replacing an older artifact and marks successful generation unverified',t=>{
  const root=workspace(t),md=join(root,'input.md'),html=join(root,'report.html');writeFileSync(html,'previous artifact');
  writeFileSync(md,'# Broken\n\n```chart\n{}\n```');
  const run=()=>spawnSync(process.execPath,[renderer,md,'--out',html,'--json','--strict'],{encoding:'utf8'});
  assert.equal(run().status,2);assert.equal(readFileSync(html,'utf8'),'previous artifact');
  writeFileSync(md,'# Valid\n\nContent.');const valid=run();assert.equal(valid.status,0);assert.equal(JSON.parse(valid.stdout).render,'unverified');
  const page=readFileSync(html,'utf8');assert.ok(page.includes('id="verify-render"'));assert.ok(!page.includes('class="meta-item"'));
});
