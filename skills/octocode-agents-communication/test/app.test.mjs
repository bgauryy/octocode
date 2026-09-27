import test from 'node:test';
import assert from 'node:assert/strict';

test('browser controller buffers one turn, pauses, resumes, and resets', async () => {
  const listeners = new Map();
  const draws = [];
  const nodes = new Map();
  const node = name => {
    if (!nodes.has(name)) nodes.set(name, {textContent:'', disabled:false, dataset:{}, addEventListener(type, fn) {listeners.set(`${name}:${type}`, fn);}});
    return nodes.get(name);
  };
  const ctx = {fillRect(...rect) {draws.push(rect);}};
  Object.assign(node('#board'), {width:400,height:400,getContext:()=>ctx});
  const buttons = ['up','down','left','right'].map(direction => Object.assign(node(direction), {dataset:{direction}}));
  let tick;
  const originals = Object.fromEntries(['document','window','setInterval','clearInterval'].map(k=>[k,Object.getOwnPropertyDescriptor(globalThis,k)]));
  globalThis.document = {querySelector:node,querySelectorAll:()=>buttons};
  globalThis.window = {addEventListener(type,fn){listeners.set(type,fn);}};
  globalThis.setInterval = fn => {tick=fn;return 1;};
  globalThis.clearInterval = () => {tick=undefined;};
  const click = name => listeners.get(`${name}:click`)({preventDefault(){}});
  const key = value => listeners.get('keydown')({key:value,preventDefault(){}});
  try {
    await import('./app.mjs');
    assert.equal(node('#status').textContent,'paused');
    click('#start');
    assert.equal(node('#status').textContent,'playing');
    assert.equal(node('#pause').disabled,false);
    key('ArrowUp');
    key('a'); // second turn in same tick must not change the queued direction
    draws.length=0;
    tick();
    assert.deepEqual(draws[2].slice(0,2),[181,181]);
    click('#pause');
    assert.equal(tick,undefined);
    assert.equal(node('#status').textContent,'paused');
    click('#start');
    click('left'); // the same click event covers pointer and keyboard activation
    draws.length=0;
    tick();
    assert.deepEqual(draws[2].slice(0,2),[161,181]);
    click('#restart');
    assert.equal(tick,undefined);
    assert.equal(node('#score').textContent,'0');
    assert.equal(node('#status').textContent,'paused');
    assert.equal(node('#start').disabled,false);
  } finally {
    for (const [key,descriptor] of Object.entries(originals)) {
      if(descriptor) Object.defineProperty(globalThis,key,descriptor); else delete globalThis[key];
    }
  }
});
