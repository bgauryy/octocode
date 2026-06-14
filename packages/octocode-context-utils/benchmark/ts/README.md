# TypeScript (.ts)

Source sample: `ts/00-typescript-core.ts`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 92419 | - | - |
| content-view | 37299 | 59.6% | 1.515 ms |
| applyMinification | 37299 | 59.6% | 1.312 ms |
| sync minify | 37299 | 59.6% | 1.226 ms |
| async minify | 37299 | 59.6% | 1.205 ms |
| symbols | 28507 | 69.2% | 12.975 ms |

## Notes

- engine-backed or parser-backed path.

## Before Excerpt

```ts
import {
    CharacterCodes,
    Comparer,
    Comparison,
    Debug,
    EqualityComparer,
    MapLike,
    Queue,
    SortedArray,
    SortedReadonlyArray,
    TextSpan,
} from "./_namespaces/ts.js";

/* eslint-disable @typescript-eslint/prefer-for-of */

/** @internal */
export const emptyArray: never[] = [] as never[];
/** @internal */
export const emptyMap: ReadonlyMap<never, never> = new Map<never, never>();

/** @internal */
export function length(array: readonly any[] | undefined): number {
    return array !== undefined ? array.length : 0;
}

/**
 * Iterates through 'array' by index and performs the callback on each element of array until the callback
 * returns a truthy value, then returns that value.
 * If no such value is found, the callback is applied to each element of array and undefined is returned.
 *
 * @internal
 */
export function forEach<T, U>(array: readonly T[] | undefined, callback: (element: T, index: number) => U | undefined): U | undefined {
    if (array !== undefined) {
        for (let i = 0; i < array.length; i++) {
            const result = callback(array[i], i);
            if (result) {
                return result;
            }


... [truncated 90615 chars] ...

ay !== undefined) {
        const len = array.length;
        let index = 0;
        while (index < len && predicate(array[index])) {
            index++;
        }
        return array.slice(index) as Exclude<T, U>[];
    }
}

/** @internal */
export function isNodeLikeSystem(): boolean {
    // This is defined here rather than in sys.ts to prevent a cycle from its
    // use in performanceCore.ts.
    return typeof process !== "undefined"
        && !!process.nextTick
        && !(process as any).browser
        && typeof require !== "undefined";
}

```

## Content-View Excerpt

```ts
import{CharacterCodes,Comparer,Comparison,Debug,EqualityComparer,MapLike,Queue,SortedArray,SortedReadonlyArray,TextSpan}from"./_namespaces/ts.js";export const emptyArray:never[]=[]as never[];export const emptyMap:ReadonlyMap<never,never>=new Map<never,never>;export function length(array:readonly any[]|undefined):number{return array===void 0?0:array.length}export function forEach<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result)return result}}export function forEachRight<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=array.length-1;i>=0;i--){let result=callback(array[i],i);if(result)return result}}export function firstDefined<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result!==void 0)return result}}export function firstDefinedIterator<T,U>(iter:Iterable<T>,callback:(element:T)=>U|undefined):U|undefined{for(let value of iter){let result=callback(value);if(result!==v

... [truncated 35499 chars] ...

nt:T)=>element is U):U[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(0,index)as U[]}}export function skipWhile<T,U extends T>(array:readonly T[]|undefined,predicate:(element:T)=>element is U):Exclude<T,U>[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(index)as Exclude<T,U>[]}}export function isNodeLikeSystem():boolean{return typeof process<`u`&&!!process.nextTick&&!(process as any).browser&&typeof require<`u`}
```

## Apply Minification Excerpt

```ts
import{CharacterCodes,Comparer,Comparison,Debug,EqualityComparer,MapLike,Queue,SortedArray,SortedReadonlyArray,TextSpan}from"./_namespaces/ts.js";export const emptyArray:never[]=[]as never[];export const emptyMap:ReadonlyMap<never,never>=new Map<never,never>;export function length(array:readonly any[]|undefined):number{return array===void 0?0:array.length}export function forEach<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result)return result}}export function forEachRight<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=array.length-1;i>=0;i--){let result=callback(array[i],i);if(result)return result}}export function firstDefined<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result!==void 0)return result}}export function firstDefinedIterator<T,U>(iter:Iterable<T>,callback:(element:T)=>U|undefined):U|undefined{for(let value of iter){let result=callback(value);if(result!==v

... [truncated 35499 chars] ...

nt:T)=>element is U):U[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(0,index)as U[]}}export function skipWhile<T,U extends T>(array:readonly T[]|undefined,predicate:(element:T)=>element is U):Exclude<T,U>[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(index)as Exclude<T,U>[]}}export function isNodeLikeSystem():boolean{return typeof process<`u`&&!!process.nextTick&&!(process as any).browser&&typeof require<`u`}
```

## Sync Minify Excerpt

```ts
import{CharacterCodes,Comparer,Comparison,Debug,EqualityComparer,MapLike,Queue,SortedArray,SortedReadonlyArray,TextSpan}from"./_namespaces/ts.js";export const emptyArray:never[]=[]as never[];export const emptyMap:ReadonlyMap<never,never>=new Map<never,never>;export function length(array:readonly any[]|undefined):number{return array===void 0?0:array.length}export function forEach<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result)return result}}export function forEachRight<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=array.length-1;i>=0;i--){let result=callback(array[i],i);if(result)return result}}export function firstDefined<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result!==void 0)return result}}export function firstDefinedIterator<T,U>(iter:Iterable<T>,callback:(element:T)=>U|undefined):U|undefined{for(let value of iter){let result=callback(value);if(result!==v

... [truncated 35499 chars] ...

nt:T)=>element is U):U[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(0,index)as U[]}}export function skipWhile<T,U extends T>(array:readonly T[]|undefined,predicate:(element:T)=>element is U):Exclude<T,U>[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(index)as Exclude<T,U>[]}}export function isNodeLikeSystem():boolean{return typeof process<`u`&&!!process.nextTick&&!(process as any).browser&&typeof require<`u`}
```

## Async Minify Excerpt

```ts
import{CharacterCodes,Comparer,Comparison,Debug,EqualityComparer,MapLike,Queue,SortedArray,SortedReadonlyArray,TextSpan}from"./_namespaces/ts.js";export const emptyArray:never[]=[]as never[];export const emptyMap:ReadonlyMap<never,never>=new Map<never,never>;export function length(array:readonly any[]|undefined):number{return array===void 0?0:array.length}export function forEach<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result)return result}}export function forEachRight<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=array.length-1;i>=0;i--){let result=callback(array[i],i);if(result)return result}}export function firstDefined<T,U>(array:readonly T[]|undefined,callback:(element:T,index:number)=>U|undefined):U|undefined{if(array!==void 0)for(let i=0;i<array.length;i++){let result=callback(array[i],i);if(result!==void 0)return result}}export function firstDefinedIterator<T,U>(iter:Iterable<T>,callback:(element:T)=>U|undefined):U|undefined{for(let value of iter){let result=callback(value);if(result!==v

... [truncated 35499 chars] ...

nt:T)=>element is U):U[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(0,index)as U[]}}export function skipWhile<T,U extends T>(array:readonly T[]|undefined,predicate:(element:T)=>element is U):Exclude<T,U>[]|undefined{if(array!==void 0){let len=array.length,index=0;for(;index<len&&predicate(array[index]);)index++;return array.slice(index)as Exclude<T,U>[]}}export function isNodeLikeSystem():boolean{return typeof process<`u`&&!!process.nextTick&&!(process as any).browser&&typeof require<`u`}
```

## Symbols

```txt
   1| import {
   2|     CharacterCodes,
   3|     Comparer,
   4|     Comparison,
   5|     Debug,
   6|     EqualityComparer,
   7|     MapLike,
   8|     Queue,
   9|     SortedArray,
  10|     SortedReadonlyArray,
  11|     TextSpan,
  12| } from "./_namespaces/ts.js";
  17| export const emptyArray: never[] = [] as never[];
  19| export const emptyMap: ReadonlyMap<never, never> = new Map<never, never>();
  22| export function length(array: readonly any[] | undefined): number {
  33| export function forEach<T, U>(array: readonly T[] | undefined, callback: (element: T, index: number) => U | undefined): U | undefined {
  50| export function forEachRight<T, U>(array: readonly T[] | undefined, callback: (element: T, index: number) => U | undefined): U | undefined {
  67| export function firstDefined<T, U>(array: readonly T[] | undefined, callback: (element: T, index: number) => U | undefined): U | undefined {
  82| export function firstDefinedIterator<T, U>(iter: Iterable<T>, callback: (element: T) => U | undefined): U | undefined {
  93| export function reduceLeftIterator<T, U>(iterator: Iterable<T> | undefined, f: (memo: U, value: T, i: number) => U, initial: U): U {
 106| export function zipWith<T, U, V>(arrayA: readonly T[], arrayB: readonly U[], callback: (a: T, b: U, index: number) => V): V[] {
 121| export function intersperse<T>(input: T[], element: T): T[] {
 140| export function every<T, U extends T>(array: readonly T[], callback: (element: T, index: number) => element is U): array is readonly U[];
 142| export function every<T, U extends T>(array: readonly T[] | undefined, callback: (element: T, index: number) => element is U): array is readonly U[] | undefined;
 144| export function every<T>(array: readonly T[] | undefined, cal

... [truncated 25907 chars] ...

rray: readonly T[], predicate: (element: T) => element is U): U[];
2559| export function takeWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): U[] | undefined;
2560| export function takeWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): U[] | undefined {
2572| export function skipWhile<T, U extends T>(array: readonly T[], predicate: (element: T) => element is U): Exclude<T, U>[];
2574| export function skipWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): Exclude<T, U>[] | undefined;
2576| export function skipWhile<T, U extends T>(array: readonly T[] | undefined, predicate: (element: T) => element is U): Exclude<T, U>[] | undefined {
2588| export function isNodeLikeSystem(): boolean {
```
