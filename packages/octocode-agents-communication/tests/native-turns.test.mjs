import {test} from 'node:test';
import assert from 'node:assert/strict';
import {nativeTurnFinished,nativeTurnFailure} from '../scripts/native-turns.mjs';
test('Claude handling ACK is not final usage, and another session cannot settle it',()=>{
 const result={type:'result',session_id:'self',is_error:false,usage:{input_tokens:5}};
 const events=[result,{type:'assistant',session_id:'self'},{type:'user',session_id:'self'}];
 assert.equal(nativeTurnFinished('claude',events,'self'),false);
 assert.equal(nativeTurnFinished('claude',[...events,{...result,session_id:'other'}],'self'),false);
 assert.equal(nativeTurnFinished('claude',[...events,result],'self'),true);
 for(const bad of [{...result,is_error:true},{...result,usage:undefined}])assert.equal(nativeTurnFinished('claude',[...events,bad],'self'),false);
});
test('Codex waits for every started turn, not an earlier completion or usage event',()=>{
 const start=id=>({method:'turn/started',params:{threadId:'self',turn:{id}}});
 const end=id=>({method:'turn/completed',params:{threadId:'self',turn:{id,status:'completed'}}});
 const events=[start('1'),end('1'),start('2')];
 assert.equal(nativeTurnFinished('codex',events,'self'),false);
 assert.equal(nativeTurnFinished('codex',[...events,end('2')],'self'),true);
 assert.equal(nativeTurnFinished('codex',[],'self'),false);
});
test('terminal native failures abort evaluation without copying credential-bearing errors',()=>{
 const failed={method:'turn/completed',params:{threadId:'self',turn:{status:'failed',error:{message:'401 Unauthorized: secret-sentinel'}}}};
 assert.match(nativeTurnFailure('codex',[failed],'self'),/HTTP 401/);
 assert.ok(!nativeTurnFailure('codex',[failed],'self').includes('secret-sentinel'));
 assert.equal(nativeTurnFailure('codex',[failed],'other'),null);
 assert.equal(nativeTurnFailure('codex',[{method:'error',params:{threadId:'self',willRetry:true}}],'self'),null);
 assert.match(nativeTurnFailure('claude',[{type:'result',session_id:'self',is_error:true}],'self'),/failed/);
});
