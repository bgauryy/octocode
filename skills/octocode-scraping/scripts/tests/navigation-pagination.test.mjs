import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { chunkTextByBytes, cleanForAgent } from '../lib/text.mjs';
import { initCorpus, writePage } from '../lib/corpus.mjs';

const scripts = fileURLToPath(new URL('../', import.meta.url));
const invoke = (command, args) => { const result = spawnSync(command, args, { encoding: 'utf8' }); assert.equal(result.status, 0, result.stderr); return JSON.parse(result.stdout); };
test('all corpus query lists have stable complete continuations and preserve distinct routes', () => {
  const dir = mkdtempSync(join(tmpdir(), 'octo-scrape-pagination-'));
  try {
    for (const folder of ['graph', 'indexes', 'extracts']) mkdirSync(join(dir, folder));
    const rows = Array.from({length:57},(_,i)=>({pageId:'p'+i,id:'p'+i,kind:'button',_file:'buttons',text:'Docs '+i,href:'https://site.test/docs/'+i,score:i,workflowHint:'docs',workflowType:'docs'}));
    writeFileSync(join(dir,'page-map.json'),JSON.stringify({pages:rows.map(r=>({...r,url:r.href,files:{textParts:[]}}))}));
    writeFileSync(join(dir,'AGENT_INDEX.json'),JSON.stringify({sessionId:'test',pages:[],warnings:[]}));
    writeFileSync(join(dir,'graph/workflows.json'),JSON.stringify({workflows:rows}));
    writeFileSync(join(dir,'graph/site-graph.json'),JSON.stringify({rootUrl:'https://site.test',pages:[],edges:[]}));
    writeFileSync(join(dir,'graph/graph.json'),JSON.stringify({nodes:rows,edges:rows.map((r,i)=>({from:r.id,to:'p0',kind:'navigates_to',label:'same destination',score:i}))}));
    for (const file of ['indexes/top-links.jsonl','extracts/links.jsonl','extracts/elements.jsonl']) writeFileSync(join(dir,file),rows.map(r=>JSON.stringify(r)).join('\n'));
    for (const [script,field] of [['corpus-inspect','pageRows'],['corpus-inspect','workflows'],['corpus-inspect','topLinks'],['graph-navigate','routes'],['graph-navigate','actionNodes'],['dom-find','matches'],['resource-list','resources']]) {
      let page = invoke(process.execPath,[join(scripts,script+'.mjs'),'--session-dir',dir,'--view',field,'--limit','3']), found = [];
      for (;;) { assert(Buffer.byteLength(JSON.stringify(page)) < 22000); found.push(...page[field]); if (!page.next?.[field]) break; page=invoke(page.next[field].command,page.next[field].args); }
      assert.equal(found.length,57,script+' '+field); assert.equal(new Set(found.map(r=>r.from ?? r.id ?? r.pageId)).size,57);
    }
    const first=invoke(process.execPath,[join(scripts,'dom-find.mjs'),'--session-dir',dir,'--limit','1']);
    writeFileSync(join(dir,'extracts/elements.jsonl'),'{}\n');
    assert.notEqual(spawnSync(first.next.matches.command,first.next.matches.args).status,0);
  } finally { rmSync(dir,{recursive:true,force:true}); }
});
test('oversized query values have reconstructable source pages', () => {
  const dir=mkdtempSync(join(tmpdir(),'octo-scrape-big-'));
  try {
    mkdirSync(join(dir,'extracts')); const row={kind:'button',text:'🧠'.repeat(10000)};
    writeFileSync(join(dir,'extracts/elements.jsonl'),JSON.stringify(row));
    let page=invoke(process.execPath,[join(scripts,'dom-find.mjs'),'--session-dir',dir]);
    const continuation=page.matches[0].next.continue; page=invoke(continuation.command,continuation.args); const parts=[];
    for(;;){parts.push(Buffer.from(page.content,'base64'));if(!page.next)break;page=invoke(page.next.continue.command,page.next.continue.args);}
    assert.deepEqual(JSON.parse(Buffer.concat(parts)),{...row,pageUrl:null,evidenceFile:'extracts/elements.jsonl',textEvidence:null});
  } finally {rmSync(dir,{recursive:true,force:true});}
});
test('downloaded raw and cleaned text survive configured view sizes and Unicode chunk boundaries', async () => {
  const dir=mkdtempSync(join(tmpdir(),'octo-scrape-retain-'));
  try {
    const body='<h1>Guide</h1><p>'+('emoji 🧠 and spaces  '.repeat(2000))+'</p>';
    const config={outBase:dir,sessionId:'test',mode:'html',provider:'direct',maxRawBytes:100,maxTextBytes:2000,chunkBytes:1000};
    const sessionDir=await initCorpus(config);
    const written=await writePage({sessionDir,config,response:{pageId:'page-001',url:'https://site.test',contentType:'text/html',status:200,body},pageIndex:1});
    assert.equal(readFileSync(join(sessionDir,written.sourceRow.raw),'utf8'),body);
    const restored=written.sourceRow.textParts.map(p=>readFileSync(join(sessionDir,p),'utf8')).join('');
    assert(restored.endsWith('emoji 🧠 and spaces')); assert(!written.sourceRow.textTruncated);
    const exact=' a 🧠 '.repeat(500); assert.equal(chunkTextByBytes(exact,1000).join(''),exact);
  } finally {rmSync(dir,{recursive:true,force:true});}
});

test('body search reaches later parts and regex pages every hit including zero-width matches', () => {
  const dir=mkdtempSync(join(tmpdir(),'octo-scrape-search-'));
  try {
    for(const folder of ['text','extracts','graph'])mkdirSync(join(dir,folder));
    writeFileSync(join(dir,'text/first.md'),'unrelated '.repeat(9000));writeFileSync(join(dir,'text/last.md'),'late-needle');
    writeFileSync(join(dir,'sources.jsonl'),JSON.stringify({pageId:'p1',url:'https://site.test',textParts:['text/first.md','text/last.md']})+'\n');
    writeFileSync(join(dir,'AGENT_INDEX.json'),JSON.stringify({sessionId:'test',pages:[]}));
    const found=invoke(process.execPath,[join(scripts,'corpus-find.mjs'),'--session-dir',dir,'--query','late-needle']);
    assert(found.matches.some(row=>row.pageId==='p1'));
    writeFileSync(join(dir,'extracts/hits.jsonl'),'needle\n'.repeat(31));
    let page=invoke(process.execPath,[join(scripts,'corpus-run.mjs'),'--session-dir',dir,'--roots','extracts','--regex','needle','--limit','2','--view','matches']),total=0;
    for(;;){total+=page.matches.length;if(!page.next?.matches)break;page=invoke(page.next.matches.command,page.next.matches.args);}
    assert.equal(total,31);
    const zero=invoke(process.execPath,[join(scripts,'corpus-run.mjs'),'--session-dir',dir,'--roots','extracts','--regex','(?=needle)','--view','matches']);assert.equal(zero.matchCount,31);
  }finally{rmSync(dir,{recursive:true,force:true});}
});

test('appending a session retains its earlier extracted links', () => {
  const dir=mkdtempSync(join(tmpdir(),'octo-scrape-append-'));
  try {
    const fixture=join(dir,'fixture.html');writeFileSync(fixture,'<h1>Guide</h1><a href="https://site.test/docs">Docs</a>');
    const args=[join(scripts,'fetch.mjs'),'--url','https://site.test/first','--provider','direct','--mock-status','200','--mock-content-type','text/html','--mock-body-file',fixture,'--session','append-proof'];
    // Fetch anchors output under the cwd's .octocode; use a private cwd.
    const call=argv=>{const result=spawnSync(process.execPath,argv,{cwd:dir,encoding:'utf8'});assert.equal(result.status,0,result.stdout+result.stderr);return JSON.parse(result.stdout);};
    const first=call(args); const before=readFileSync(first.sessionDir+'/extracts/links.jsonl','utf8').trim().split('\n');
    const next=[...args];next[next.indexOf('--url')+1]='https://site.test/second';call([...next,'--append']);
    const after=readFileSync(first.sessionDir+'/extracts/links.jsonl','utf8').trim().split('\n').map(JSON.parse);
    assert.equal(after.length,before.length*2);assert(after.some(row=>row.pageId==='page-001'));assert(after.some(row=>row.pageId==='page-002'));
  }finally{rmSync(dir,{recursive:true,force:true});}
});

test('specialized script results retain every field through bounded source pages', () => {
  const dir=mkdtempSync(join(tmpdir(),'octo-scrape-script-'));
  try {
    mkdirSync(join(dir,'extracts'));writeFileSync(join(dir,'extracts/one.json'),'{}');
    const expected={ok:true,extra:'🧠'.repeat(10000),findings:[{text:'complete'}]};
    const script=join(dir,'custom.mjs');writeFileSync(script,'export async function run(){return '+JSON.stringify(expected)+';}');
    const result=invoke(process.execPath,[join(scripts,'corpus-run.mjs'),'--session-dir',dir,'--script',script]);
    assert(Buffer.byteLength(JSON.stringify(result))<22000);
    let page=invoke(result.script.result.next.continue.command,result.script.result.next.continue.args);const parts=[];
    for(;;){parts.push(Buffer.from(page.content,'base64'));if(!page.next)break;page=invoke(page.next.continue.command,page.next.continue.args);}
    assert.deepEqual(JSON.parse(Buffer.concat(parts)),expected);
  }finally{rmSync(dir,{recursive:true,force:true});}
});
