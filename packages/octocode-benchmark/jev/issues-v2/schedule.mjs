import {spawn} from 'node:child_process';
import {readFileSync} from 'node:fs';
import {dirname,join} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=dirname(fileURLToPath(import.meta.url));
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
// The first pair was launched interactively. Wait for both before continuing.
while(['baseline','treatment'].some(arm=>JSON.parse(readFileSync(join(root,'runs',arm,'R37637','host.json'))).status==='running'))await sleep(5000);
for(const id of ['R37619','L40592','L40590','O151639','O151637']){
 await Promise.all(['baseline','treatment'].map(arm=>new Promise(resolve=>{
  const child=spawn(process.execPath,[join(root,'launch.mjs'),arm,id],{cwd:root,stdio:'inherit'});
  child.on('close',code=>resolve({arm,id,code}));
 })));
}
console.log('All six primary pairs finished; inspect failures before grading.');
