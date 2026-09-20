import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';

// Candidate only. The controller reserves each case exclusively and checks frozen artifacts.
const controller=fileURLToPath(new URL('./campaign.mjs',import.meta.url));
let next=1;
let stop=false;
async function worker(){
  while(!stop&&next<=30){
    const id=`Q${next++}`;
    console.log(JSON.stringify({starting:id}));
    const child=spawn(process.execPath,[controller,'run',id],{stdio:'inherit'});
    const code=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',resolve);});
    if(code!==0){stop=true;process.exitCode=1;console.error(JSON.stringify({stoppedAfter:id,exitCode:code,reason:'Controller gate failed; no further cases launched.'}));}
  }
}
await Promise.all([worker(),worker()]);
