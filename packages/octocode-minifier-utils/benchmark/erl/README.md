# Erlang (.erl)

Source sample: `erl/erlang-lists.erl`

Strategy: `aggressive`

Agent rating: **7.8/10 (good)**

Agent understanding from minified output: **9.4/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 123312 | - | - | - |
| content-view | 116175 | 5.8% | 25.294 ms | 7.8/10 |
| applyMinification | 94098 | 23.7% | 27.365 ms | 7.8/10 |
| sync minify | 94098 | 23.7% | 26.75 ms | 7.8/10 |
| async minify | 94098 | 23.7% | 26.647 ms | 7.8/10 |
| symbols | 143752 | -16.6% | 6.452 ms | n/a |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 7/10 |
| symbol context | 7/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 123312 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 116175 | 5.8% | 9.4/10 excellent | 10/10 | 10/10 |
| minify | 94098 | 23.7% | 9.5/10 excellent | 10/10 | 10/10 |
| symbols | 143752 | -16.6% | 9/10 excellent | 10/10 | 10/10 |

## Notes

- aggressive text strategy.
- symbols are not implemented for this extension.

## Before Excerpt

```erl
%%
%% %CopyrightBegin%
%%
%% SPDX-License-Identifier: Apache-2.0
%%
%% Copyright Ericsson AB 1996-2026. All Rights Reserved.
%%
%% Licensed under the Apache License, Version 2.0 (the "License");
%% you may not use this file except in compliance with the License.
%% You may obtain a copy of the License at
%%
%%     http://www.apache.org/licenses/LICENSE-2.0
%%
%% Unless required by applicable law or agreed to in writing, software
%% distributed under the License is distributed on an "AS IS" BASIS,
%% WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
%% See the License for the specific language governing permissions and
%% limitations under the License.
%%
%% %CopyrightEnd%
%%
-module(lists).
-moduledoc """
List processing functions.

This module contains functions for list processing.

Unless otherwise stated, all functions assume that position numbering starts
at 1. That is, the first element of a list is at position 1.

Two terms `T1` and `T2` compare equal if `T1 == T2` evaluates to `true`. They
match if `T1 =:= T2` evaluates to `true`.

Whenever an _ordering function_{: #ordering_function } `F` is expected as
argument, it is assumed that the following properties hold of `F` for a

... [truncated 121508 chars] ...

| M], H2);
        false ->
            case Fun(H2M, H1) of
                true -> % H2M equal to H1
                    rufmerge2_1(T1, H2, Fun, T2, [H1 | M]);
                false ->
                    rufmerge2_1(T1, H2, Fun, T2, [H1, H2M | M])
            end
    end;
rufmerge2_2(H1, T1, Fun, [], M, H2M) ->
    case Fun(H2M, H1) of
        true -> 
            lists:reverse(T1, [H1 | M]);
        false ->
            lists:reverse(T1, [H1, H2M | M])
    end.

%%%
%%% Don't place new functions here; place them before the
%%% implementation of sort functions.
%%%

```

## Content-View Excerpt

```erl
-module(lists).
-moduledoc """
List processing functions.

This module contains functions for list processing.

Unless otherwise stated, all functions assume that position numbering starts
at 1. That is, the first element of a list is at position 1.

Two terms `T1` and `T2` compare equal if `T1 == T2` evaluates to `true`. They
match if `T1 =:= T2` evaluates to `true`.

Whenever an _ordering function_{: #ordering_function } `F` is expected as
argument, it is assumed that the following properties hold of `F` for all x, y,
and z:

- If x `F` y and y `F` x, then x = y (`F` is antisymmetric).
- If x `F` y and y `F` z, then x `F` z (`F` is transitive).
- x `F` y or y `F` x (`F` is total).

An example of a typical ordering function is less than or equal to: `=</2`.
""".

-compile({no_auto_import,[max/2]}).
-compile({no_auto_import,[min/2]}).

-export([keyfind/3, keymember/3, keysearch/3, member/2, reverse/2]).

-export([append/1, append/2, concat/1,
         delete/2, droplast/1, duplicate/2,
         enumerate/1, enumerate/2, enumerate/3,
         flatlength/1, flatten/1, flatten/2,
         join/2, last/1, min/1, max/1,
         nth/2, nthtail/2,
         prefix/2, reverse/1, seq/2, seq/3,
         split/2, su

... [truncated 114371 chars] ...

2_2(H1, T1, Fun, [H2 | T2], M, H2M) ->
    case Fun(H1, H2) of
        true ->
            rufmerge2_2(H1, T1, Fun, T2, [H2M | M], H2);
        false ->
            case Fun(H2M, H1) of
                true ->
                    rufmerge2_1(T1, H2, Fun, T2, [H1 | M]);
                false ->
                    rufmerge2_1(T1, H2, Fun, T2, [H1, H2M | M])
            end
    end;
rufmerge2_2(H1, T1, Fun, [], M, H2M) ->
    case Fun(H2M, H1) of
        true ->
            lists:reverse(T1, [H1 | M]);
        false ->
            lists:reverse(T1, [H1, H2M | M])
    end.
```

## Apply Minification Excerpt

```erl
-module(lists). -moduledoc """ List processing functions. This module contains functions for list processing. Unless otherwise stated,all functions assume that position numbering starts at 1. That is,the first element of a list is at position 1. Two terms `T1` and `T2` compare equal if `T1 == T2` evaluates to `true`. They match if `T1 =:= T2` evaluates to `true`. Whenever an _ordering function_{:#ordering_function}`F` is expected as argument,it is assumed that the following properties hold of `F` for all x,y,and z:- If x `F` y and y `F` x,then x = y (`F` is antisymmetric). - If x `F` y and y `F` z,then x `F` z (`F` is transitive). - x `F` y or y `F` x (`F` is total). An example of a typical ordering function is less than or equal to:`=</2`. """. -compile({no_auto_import,[max/2]}). -compile({no_auto_import,[min/2]}). -export([keyfind/3,keymember/3,keysearch/3,member/2,reverse/2]). -export([append/1,append/2,concat/1,delete/2,droplast/1,duplicate/2,enumerate/1,enumerate/2,enumerate/3,flatlength/1,flatten/1,flatten/2,join/2,last/1,min/1,max/1,nth/2,nthtail/2,prefix/2,reverse/1,seq/2,seq/3,split/2,sublist/2,sublist/3,subtract/2,suffix/2,sum/1,uniq/1,unzip/1,unzip3/1,zip/2,zip/3,zip3/3,zip3/4]). -export([keyde

... [truncated 92298 chars] ...

. rufmerge2_1([H1 | T1],H2,Fun,T2,M) -> case Fun(H1,H2) of true -> rufmerge2_2(H1,T1,Fun,T2,M,H2);false -> rufmerge2_1(T1,H2,Fun,T2,[H1 | M]) end;rufmerge2_1([],H2,_Fun,T2,M) -> lists:reverse(T2,[H2 | M]). rufmerge2_2(H1,T1,Fun,[H2 | T2],M,H2M) -> case Fun(H1,H2) of true -> rufmerge2_2(H1,T1,Fun,T2,[H2M | M],H2);false -> case Fun(H2M,H1) of true -> rufmerge2_1(T1,H2,Fun,T2,[H1 | M]);false -> rufmerge2_1(T1,H2,Fun,T2,[H1,H2M | M]) end end;rufmerge2_2(H1,T1,Fun,[],M,H2M) -> case Fun(H2M,H1) of true -> lists:reverse(T1,[H1 | M]);false -> lists:reverse(T1,[H1,H2M | M]) end.
```

## Sync Minify Excerpt

```erl
-module(lists). -moduledoc """ List processing functions. This module contains functions for list processing. Unless otherwise stated,all functions assume that position numbering starts at 1. That is,the first element of a list is at position 1. Two terms `T1` and `T2` compare equal if `T1 == T2` evaluates to `true`. They match if `T1 =:= T2` evaluates to `true`. Whenever an _ordering function_{:#ordering_function}`F` is expected as argument,it is assumed that the following properties hold of `F` for all x,y,and z:- If x `F` y and y `F` x,then x = y (`F` is antisymmetric). - If x `F` y and y `F` z,then x `F` z (`F` is transitive). - x `F` y or y `F` x (`F` is total). An example of a typical ordering function is less than or equal to:`=</2`. """. -compile({no_auto_import,[max/2]}). -compile({no_auto_import,[min/2]}). -export([keyfind/3,keymember/3,keysearch/3,member/2,reverse/2]). -export([append/1,append/2,concat/1,delete/2,droplast/1,duplicate/2,enumerate/1,enumerate/2,enumerate/3,flatlength/1,flatten/1,flatten/2,join/2,last/1,min/1,max/1,nth/2,nthtail/2,prefix/2,reverse/1,seq/2,seq/3,split/2,sublist/2,sublist/3,subtract/2,suffix/2,sum/1,uniq/1,unzip/1,unzip3/1,zip/2,zip/3,zip3/3,zip3/4]). -export([keyde

... [truncated 92298 chars] ...

. rufmerge2_1([H1 | T1],H2,Fun,T2,M) -> case Fun(H1,H2) of true -> rufmerge2_2(H1,T1,Fun,T2,M,H2);false -> rufmerge2_1(T1,H2,Fun,T2,[H1 | M]) end;rufmerge2_1([],H2,_Fun,T2,M) -> lists:reverse(T2,[H2 | M]). rufmerge2_2(H1,T1,Fun,[H2 | T2],M,H2M) -> case Fun(H1,H2) of true -> rufmerge2_2(H1,T1,Fun,T2,[H2M | M],H2);false -> case Fun(H2M,H1) of true -> rufmerge2_1(T1,H2,Fun,T2,[H1 | M]);false -> rufmerge2_1(T1,H2,Fun,T2,[H1,H2M | M]) end end;rufmerge2_2(H1,T1,Fun,[],M,H2M) -> case Fun(H2M,H1) of true -> lists:reverse(T1,[H1 | M]);false -> lists:reverse(T1,[H1,H2M | M]) end.
```

## Async Minify Excerpt

```erl
-module(lists). -moduledoc """ List processing functions. This module contains functions for list processing. Unless otherwise stated,all functions assume that position numbering starts at 1. That is,the first element of a list is at position 1. Two terms `T1` and `T2` compare equal if `T1 == T2` evaluates to `true`. They match if `T1 =:= T2` evaluates to `true`. Whenever an _ordering function_{:#ordering_function}`F` is expected as argument,it is assumed that the following properties hold of `F` for all x,y,and z:- If x `F` y and y `F` x,then x = y (`F` is antisymmetric). - If x `F` y and y `F` z,then x `F` z (`F` is transitive). - x `F` y or y `F` x (`F` is total). An example of a typical ordering function is less than or equal to:`=</2`. """. -compile({no_auto_import,[max/2]}). -compile({no_auto_import,[min/2]}). -export([keyfind/3,keymember/3,keysearch/3,member/2,reverse/2]). -export([append/1,append/2,concat/1,delete/2,droplast/1,duplicate/2,enumerate/1,enumerate/2,enumerate/3,flatlength/1,flatten/1,flatten/2,join/2,last/1,min/1,max/1,nth/2,nthtail/2,prefix/2,reverse/1,seq/2,seq/3,split/2,sublist/2,sublist/3,subtract/2,suffix/2,sum/1,uniq/1,unzip/1,unzip3/1,zip/2,zip/3,zip3/3,zip3/4]). -export([keyde

... [truncated 92298 chars] ...

. rufmerge2_1([H1 | T1],H2,Fun,T2,M) -> case Fun(H1,H2) of true -> rufmerge2_2(H1,T1,Fun,T2,M,H2);false -> rufmerge2_1(T1,H2,Fun,T2,[H1 | M]) end;rufmerge2_1([],H2,_Fun,T2,M) -> lists:reverse(T2,[H2 | M]). rufmerge2_2(H1,T1,Fun,[H2 | T2],M,H2M) -> case Fun(H1,H2) of true -> rufmerge2_2(H1,T1,Fun,T2,[H2M | M],H2);false -> case Fun(H2M,H1) of true -> rufmerge2_1(T1,H2,Fun,T2,[H1 | M]);false -> rufmerge2_1(T1,H2,Fun,T2,[H1,H2M | M]) end end;rufmerge2_2(H1,T1,Fun,[],M,H2M) -> case Fun(H2M,H1) of true -> lists:reverse(T1,[H1 | M]);false -> lists:reverse(T1,[H1,H2M | M]) end.
```

## Symbols

```txt
   1| %%
   2| %% %CopyrightBegin%
   3| %%
   4| %% SPDX-License-Identifier: Apache-2.0
   5| %%
   6| %% Copyright Ericsson AB 1996-2026. All Rights Reserved.
   7| %%
   8| %% Licensed under the Apache License, Version 2.0 (the "License");
   9| %% you may not use this file except in compliance with the License.
  10| %% You may obtain a copy of the License at
  11| %%
  12| %%     http://www.apache.org/licenses/LICENSE-2.0
  13| %%
  14| %% Unless required by applicable law or agreed to in writing, software
  15| %% distributed under the License is distributed on an "AS IS" BASIS,
  16| %% WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
  17| %% See the License for the specific language governing permissions and
  18| %% limitations under the License.
  19| %%
  20| %% %CopyrightEnd%
  21| %%
  22| -module(lists).
  23| -moduledoc """
  24| List processing functions.
  26| This module contains functions for list processing.
  28| Unless otherwise stated, all functions assume that position numbering starts
  29| at 1. That is, the first element of a list is at position 1.
  31| Two terms `T1` and `T2` compare equal if `T1 == T2` evaluates to `true`. They
  32| match if `T1 =:= T2` evaluates to `true`.
  34| Whenever an _ordering function_{: #ordering_function } `F` is expected as
  35| argument, it is assumed that the following properties hold of `F` for all x, y,
  36| and z:
  38| - If x `F` y and y `F` x, then x = y (`F` is antisymmetric).
  39| - If x `F` y and y `F` z, then x `F` z (`F` is transitive).
  40| - x `F` y or y `F` x (`F` is total).
  42| An example of a typical ordering function is less than or equal to: `=</2`.
  43| """.
  45| -compile({no_auto_import,[max/2]}).
  46| -compile({no_auto_impor

... [truncated 141148 chars] ...

ge2_2(H1, T1, Fun, [H2 | T2], M, H2M) ->
4347|     case Fun(H1, H2) of
4348|         true ->
4349|             rufmerge2_2(H1, T1, Fun, T2, [H2M | M], H2);
4350|         false ->
4351|             case Fun(H2M, H1) of
4352|                 true -> % H2M equal to H1
4353|                     rufmerge2_1(T1, H2, Fun, T2, [H1 | M]);
4354|                 false ->
4355|                     rufmerge2_1(T1, H2, Fun, T2, [H1, H2M | M])
4356|             end
4357|     end;
4358| rufmerge2_2(H1, T1, Fun, [], M, H2M) ->
4359|     case Fun(H2M, H1) of
4360|         true ->
4361|             lists:reverse(T1, [H1 | M]);
4362|         false ->
4363|             lists:reverse(T1, [H1, H2M | M])
4364|     end.
4366| %%%
4367| %%% Don't place new functions here; place them before the
4368| %%% implementation of sort functions.
4369| %%%
```
