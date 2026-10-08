import { createReadStream } from 'node:fs';

// Validate the JSON envelope and yield byte ranges of one selected array's rows.
// Unselected string values are skipped; memory follows nesting and key size.
export async function* arrayRanges(file, pointer = '') {
  const encode = key => key.replace(/~/g, '~0').replace(/\//g, '~1');
  if (pointer && (!pointer.startsWith('/') || /~(?![01])/u.test(pointer)))
    throw new Error('Invalid JSON pointer');
  const stack = [];
  let rootDone = false,
    found = false,
    active,
    ready;
  let mode = '',
    start = 0,
    keyBytes = null,
    escaped = false,
    hex = 0,
    primitive = '',
    position = 0;
  const error = () => {
    throw new Error(`Invalid JSON near byte ${position}`);
  };
  const finish = end => {
    const parent = stack.at(-1);
    if (parent) {
      parent.expect = 'comma';
      if (parent.type === '[') parent.index++;
    } else rootDone = true;
    if (active && stack.length === active.depth) {
      ready = {
        offset: active.offset,
        length: end - active.offset,
        index: active.index,
      };
      active = null;
    }
  };
  const token = (type: string, end: number, key?: string) => {
    const parent = stack.at(-1);
    if (type === '}' || type === ']') {
      if (
        !parent ||
        (parent.type === '[' ? type !== ']' : type !== '}') ||
        !['comma', 'first'].includes(parent.expect)
      )
        error();
      stack.pop();
      finish(end);
      return;
    }
    if (type === ',') {
      if (!parent || parent.expect !== 'comma') error();
      parent.expect = parent.type === '[' ? 'value' : 'key';
      return;
    }
    if (type === ':') {
      if (!parent || parent.expect !== 'colon') error();
      parent.expect = 'value';
      return;
    }
    if (parent?.type === '{' && ['key', 'first'].includes(parent.expect)) {
      if (type !== 'string') error();
      parent.key = key;
      parent.expect = 'colon';
      return;
    }
    if (parent ? !['value', 'first'].includes(parent.expect) : rootDone)
      error();
    const path = parent
      ? `${parent.path}/${parent.type === '[' ? parent.index : encode(parent.key)}`
      : '';
    if (parent?.selected)
      active = { offset: start, depth: stack.length, index: parent.index };
    if (path === pointer) {
      if (type !== '[' || found)
        throw new Error(
          'Select exactly one array with --pointer; duplicate selected paths are ambiguous'
        );
      found = true;
    }
    if (type === '[' || type === '{')
      stack.push({
        type,
        path,
        index: 0,
        expect: 'first',
        selected: type === '[' && path === pointer,
      });
    else finish(end);
  };
  for await (const chunk of createReadStream(file, { highWaterMark: 65536 })) {
    for (let i = 0; i < chunk.length; i++, position++) {
      const b = chunk[i];
      if (mode === 'string') {
        if (keyBytes) keyBytes.push(b);
        if (hex) {
          if (!/[0-9a-f]/i.test(String.fromCharCode(b))) error();
          hex--;
          continue;
        }
        if (escaped) {
          escaped = false;
          if (b === 117) hex = 4;
          else if (![34, 92, 47, 98, 102, 110, 114, 116].includes(b)) error();
          continue;
        }
        if (b === 92) {
          escaped = true;
          continue;
        }
        if (b < 32) error();
        if (b === 34) {
          mode = '';
          token(
            'string',
            position + 1,
            keyBytes
              ? JSON.parse(Buffer.from(keyBytes).toString('utf8'))
              : undefined
          );
          keyBytes = null;
        }
      } else {
        if (mode === 'primitive') {
          if (![9, 10, 13, 32, 44, 93, 125].includes(b)) {
            primitive += String.fromCharCode(b);
            continue;
          }
          if (
            !/^(?:true|false|null|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)$/.test(
              primitive
            )
          )
            error();
          mode = '';
          token('primitive', position);
          if (ready) {
            yield ready;
            ready = null;
          }
        }
        if ([9, 10, 13, 32].includes(b)) continue;
        start = position;
        if (b === 34) {
          mode = 'string';
          const p = stack.at(-1);
          keyBytes =
            p?.type === '{' && ['key', 'first'].includes(p.expect)
              ? [34]
              : null;
        } else if ([123, 125, 91, 93, 44, 58].includes(b))
          token(String.fromCharCode(b), position + 1);
        else if (
          b === 45 ||
          (b >= 48 && b <= 57) ||
          [116, 102, 110].includes(b)
        ) {
          mode = 'primitive';
          primitive = String.fromCharCode(b);
        } else error();
      }
      if (ready) {
        yield ready;
        ready = null;
      }
    }
  }
  if (mode === 'primitive') {
    if (
      !/^(?:true|false|null|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)$/.test(
        primitive
      )
    )
      error();
    token('primitive', position);
  }
  if (mode === 'string' || stack.length || !rootDone) error();
  if (!found)
    throw new Error(
      'Select an array with --pointer; use artifact-query for arbitrary values'
    );
  if (ready) yield ready;
}
