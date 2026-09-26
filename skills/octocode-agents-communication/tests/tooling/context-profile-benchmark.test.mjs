import {test} from 'node:test';
import assert from 'node:assert/strict';
import {normalizeUsage,evaluate} from '../../src/context-profile-benchmark.mjs';
test('context accounting separates exclusive Claude counters from inclusive input',()=>{
 const rows=[{inputTokens:10,cachedInputTokens:100,cacheWriteTokens:20,outputTokens:3}];
 assert.equal(normalizeUsage('claude',rows).input,130);
 assert.equal(normalizeUsage('codex',rows).input,10);
 assert.equal(normalizeUsage('grok',rows).input,10);
 assert.equal(normalizeUsage('claude',[{inputTokens:10}]).input,null);
 assert.equal(normalizeUsage('codex',[]).input,null);
 assert.equal(normalizeUsage('grok',[{inputTokens:-1}]).input,null);
});
const fixture=()=>['review','handoff'].flatMap(family=>[1,2].flatMap(pair=>['full','scoped'].map(arm=>({family,pair,arm,code:0,passed:true,childrenReaped:true,pending:0,handledRoundMs:100,usage:Object.fromEntries(['claude','codex','grok'].map(v=>[v,{input:arm==='full'?1000:700}]))}))));
test('keep gate requires completed paired tasks, known usage and bounded latency',()=>{
 const valid=fixture();assert.equal(evaluate(valid).vendors.claude.accepted,true);
 for(const mutate of [r=>r.pop(),r=>r[0].passed=false,r=>r[0].childrenReaped=false,r=>r[0].pending=1,r=>r[0].code=1,r=>r[1].handledRoundMs=121,r=>r[1].handledRoundMs=null,r=>r[1].usage.claude.input=null,r=>r[1]=r[0]]){
  const rows=fixture();mutate(rows);assert.equal(evaluate(rows).vendors.claude.accepted,false);
 }
 assert.equal(evaluate([]).allPassed,false);
});
