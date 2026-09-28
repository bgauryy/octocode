export function envelopes(result) {
 const out=[]; if(result?.structuredContent)out.push(result.structuredContent);
 for(const block of result?.content??[])if(block.type==='text'){try{out.push(JSON.parse(block.text));}catch{}}
 return out;
}
function sourceObserved(proof, task, calls) {
 for(const call of calls){
  if(!call.admitted||call.result?.isError||call.name!=='ghGetFileContent')continue;
  const queries=call.args.queries??[call.args];
  for(const envelope of envelopes(call.result))for(const row of envelope.results??[]){
   const q=queries[row.index??0];if(q?.owner!==task.owner||q.repo!==task.repo||q.branch!==task.ref||q.path!==proof.path||row.error)continue;
   for(const file of row.data?.files??[]){
    if(file.path!==proof.path)continue;
    const ranges=file.sourceLineRanges??[];
    const lines=file.content?.split('\n').filter(l=>!/^\.\.\. \[lines \d+-\d+ omitted\] \.\.\.$/.test(l));if(!lines)continue;
    const count=ranges.reduce((s,r)=>s+r.end-r.start+1,0);if(lines.length===count+1&&lines.at(-1)==='')lines.pop();if(lines.length!==count)continue;
    let offset=0;for(const range of ranges){if(proof.line>=range.start&&proof.line<=range.end&&lines[offset+proof.line-range.start]?.trim()===proof.text.trim())return true;offset+=range.end-range.start+1;}
   }
  }
  // The runtime may render numbered source without structured source bodies.
  if(queries.length===1){const q=queries[0];if(q.owner!==task.owner||q.repo!==task.repo||q.branch!==task.ref||q.path!==proof.path)continue;
   for(const block of call.result.content??[])if(block.type==='text'&&block.text.split('\n').some(l=>l===`${proof.line}: ${proof.text}`))return true;
  }
 }
 return false;
}
const canonical=v=>typeof v==='string'?v.trim().replace(/\s*,\s*/g,','):null;
export function grade(task, answer, calls){
 if(!answer||canonical(answer.value)!==canonical(task.expected.value)||!Array.isArray(answer.evidence))return false;
 if(task.history){const url=`https://github.com/${task.owner}/${task.repo}/commit/${task.ref}`;
  return answer.evidence.some(c=>c.url===url)&&calls.some(c=>c.admitted&&!c.result?.isError&&c.name==='ghGetHistoryItem'&&envelopes(c.result).some(e=>(e.results??[]).some(r=>r.data?.sha===task.ref&&r.data?.owner===task.owner&&r.data?.repo===task.repo&&r.data?.messageHeadline===task.expected.value)));
 }
 return task.expected.proofs.every(p=>answer.evidence.some(c=>c.url.replace(/#L\d+(?:-L\d+)?$/,'')===`https://github.com/${task.owner}/${task.repo}/blob/${task.ref}/${p.path}`&&c.line===p.line)&&sourceObserved(p,task,calls));
}
