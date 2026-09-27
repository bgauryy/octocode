import {test} from 'node:test';
import assert from 'node:assert/strict';
import {interviewEvidence, grokInterviewIdle} from '../../src/interview-evidence.mjs';

test('Grok interview requires latest exact-session idle queue evidence', () => {
  const queue = (sessionId, state) => ({method:'_x.ai/queue/changed',params:{sessionId,...state}});
  const idle = queue('own',{entries:[]});
  assert.equal(grokInterviewIdle([idle],'own'),true);
  assert.equal(grokInterviewIdle([idle,queue('own',{entries:[],runningPromptId:'work',runningKind:'prompt'})],'own'),false);
  assert.equal(grokInterviewIdle([idle,queue('own',{entries:[{id:'queued'}]})],'own'),false);
  assert.equal(grokInterviewIdle([queue('own',{entries:[],runningKind:'prompt'})],'own'),false);
  assert.equal(grokInterviewIdle([queue('own',{entries:[],runningPromptId:'work'})],'own'),false);
  assert.equal(grokInterviewIdle([idle,queue('other',{entries:[{id:'unrelated'}]})],'own'),true);
  assert.equal(grokInterviewIdle([queue('other',{entries:[]})],'own'),false);
  assert.equal(grokInterviewIdle([],'own'),false);
  assert.equal(grokInterviewIdle([idle,queue('own',{})],'own'),false);
  assert.equal(grokInterviewIdle([idle,queue('own',{entries:'unknown'})],'own'),false);
});

test('Claude and Pi mixed string/array native content preserves only assistant reflection', () => {
  const events = [
    {type:'user',message:{role:'user',content:'Interview instruction'}},
    {type:'message_end',message:{role:'user',content:''}},
    {type:'assistant',message:{role:'assistant',content:[{type:'text',text:'Claude reflection.'}]}},
    {type:'message_end',message:{role:'assistant',content:'Pi plain reflection.'}},
    {type:'message_end',message:{role:'assistant',content:[{type:'text',text:'Pi block reflection.'}]}},
    {type:'message_end',message:{role:'assistant',content:null}},
  ];
  assert.deepEqual(interviewEvidence(events), {text:'Claude reflection.\nPi plain reflection.\nPi block reflection.',toolsUsed:false});
});

test('string content never hides actual tool use in mixed native events', () => {
  const text = {type:'user',message:{role:'user',content:'No tools'}};
  for (const event of [
    {type:'assistant',message:{role:'assistant',content:[{type:'tool_use',name:'complete'}]}},
    {type:'message_end',message:{role:'assistant',content:[{type:'toolCall',name:'complete'}]}},
    {type:'tool_execution_start'},
    {method:'item/completed',params:{item:{type:'mcpToolCall'}}},
    {method:'session/update',params:{update:{sessionUpdate:'tool_call'}}},
  ]) assert.equal(interviewEvidence([text,event]).toolsUsed,true);
  assert.equal(interviewEvidence([text],{parts:[{type:'tool',tool:'complete'}]}).toolsUsed,true);
});

test('Codex, Grok and OpenCode reflection extraction remains supported', () => {
  assert.deepEqual(interviewEvidence([
    {method:'item/completed',params:{item:{type:'agentMessage',text:'Codex'}}},
    {method:'session/update',params:{update:{sessionUpdate:'agent_message_chunk',content:{type:'text',text:'Grok'}}}},
  ],{parts:[{type:'text',text:'OpenCode'}]}),{text:'Codex\nGrok\nOpenCode',toolsUsed:false});
});

test('Grok streamed token fragments concatenate without invented whitespace', () => {
  const events = ['The',' request',', notice',' and complete',' split.\n','Keep it short.'].map(text =>
    ({method:'session/update',params:{update:{sessionUpdate:'agent_message_chunk',content:{type:'text',text}}}}));
  assert.deepEqual(interviewEvidence(events),{text:'The request, notice and complete split.\nKeep it short.',toolsUsed:false});
});
