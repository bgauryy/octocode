// header comment
/// <reference path="./types.d.ts" />
// @ts-ignore
import type { Foo } from './foo';
/* block
   foo();
*/
foo();
const url = "http://example.com/a  b"; // trailing
const re = /a  b\/\/c/g; // regex
const tpl = `line ${a  +  b}
  // not a comment
  end`;
const div = a / b / c; // div
export function add(a: number , b: number): number {
    return a  +  b ;
}
const s = 'it''s';
const generic = new Map<string, Array<number>>();
