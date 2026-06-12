# Elixir (.ex)

Source sample: `ex/elixir-enum.ex`

Strategy: `aggressive`

Agent rating: **8.6/10 (strong)**

Agent understanding from minified output: **9.6/10 (excellent)**

Artifacts:

- `raw/source.excerpt.txt`
- `minified/content-view.excerpt.txt`
- `minified/apply-minification.excerpt.txt`
- `minified/minify-content-sync.excerpt.txt`
- `minified/minify-content-async.excerpt.txt`
- `symbol/signatures.txt`

| Tool | Bytes | Cut | Time | Rating |
| --- | ---: | ---: | ---: | ---: |
| input | 154291 | - | - | - |
| content-view | 152002 | 1.5% | 24.207 ms | 7.8/10 |
| applyMinification | 129139 | 16.3% | 25.871 ms | 7.8/10 |
| sync minify | 129139 | 16.3% | 26.467 ms | 7.8/10 |
| async minify | 129139 | 16.3% | 26.252 ms | 7.8/10 |
| symbols | 28 | 100% | 1.25 ms | 10/10 |

## Agent Understanding

Measured from `standard` minified output.

| Component | Score |
| --- | ---: |
| syntax anchors | 10/10 (3/3) |
| delimiter structure | 10/10 |
| output health | 10/10 |
| context budget | 6/10 |
| symbol context | 10/10 |
| signals passed | 6/6 |

## Agent Observation By Output Level

Ratings are computed from the actual raw, standard, minify, and symbol outputs
for this language sample.

| Level | Bytes | Cut | Agent observation | Syntax anchors | Structure |
| --- | ---: | ---: | ---: | ---: | ---: |
| none | 154291 | 0% | 10/10 excellent | 10/10 | 10/10 |
| standard | 152002 | 1.5% | 9.6/10 excellent | 10/10 | 10/10 |
| minify | 129139 | 16.3% | 9.8/10 excellent | 10/10 | 10/10 |
| symbols | 28 | 100% | 6.7/10 fair | 3.3/10 | 8/10 |

## Notes

- aggressive text strategy.

## Before Excerpt

```ex
# SPDX-License-Identifier: Apache-2.0
# SPDX-FileCopyrightText: 2021 The Elixir Team
# SPDX-FileCopyrightText: 2012 Plataformatec

defprotocol Enumerable do
  @moduledoc """
  Enumerable protocol used by `Enum` and `Stream` modules.

  When you invoke a function in the `Enum` module, the first argument
  is usually a collection that must implement this protocol.
  For example, the expression `Enum.map([1, 2, 3], &(&1 * 2))`
  invokes `Enumerable.reduce/3` to perform the reducing operation that
  builds a mapped list by calling the mapping function `&(&1 * 2)` on
  every element in the collection and consuming the element with an
  accumulated list.

  Internally, `Enum.map/2` is implemented as follows:

      def map(enumerable, fun) do
        reducer = fn x, acc -> {:cont, [fun.(x) | acc]} end
        Enumerable.reduce(enumerable, {:cont, []}, reducer) |> elem(1) |> :lists.reverse()
      end

  Note that the user-supplied function is wrapped into a `t:reducer/0` function.
  The `t:reducer/0` function must return a tagged tuple after each step,
  as described in the `t:acc/0` type. At the end, `Enumerable.reduce/3`
  returns `t:result/0`.

  This protocol uses tagged tuples to exchange information betwe

... [truncated 152491 chars] ...

d

  def slice(first.._//step = range) do
    {:ok, Range.size(range), &slice(first + &1 * step, step * &3, &2)}
  end

  # TODO: Remove me on v2.0

  slice =
    quote generated: true do
      slice(%{__struct__: Range, first: var!(first), last: var!(last)} = var!(range))
    end

  def unquote(slice) do
    step = if first <= last, do: 1, else: -1
    slice(Map.put(range, :step, step))
  end

  defp slice(current, _step, 1), do: [current]

  defp slice(current, step, remaining) when remaining > 1 do
    [current | slice(current + step, step, remaining - 1)]
  end
end

```

## Content-View Excerpt

```ex
defprotocol Enumerable do
  @moduledoc """
  Enumerable protocol used by `Enum` and `Stream` modules.

  When you invoke a function in the `Enum` module, the first argument
  is usually a collection that must implement this protocol.
  For example, the expression `Enum.map([1, 2, 3], &(&1 * 2))`
  invokes `Enumerable.reduce/3` to perform the reducing operation that
  builds a mapped list by calling the mapping function `&(&1 * 2)` on
  every element in the collection and consuming the element with an
  accumulated list.

  Internally, `Enum.map/2` is implemented as follows:

      def map(enumerable, fun) do
        reducer = fn x, acc -> {:cont, [fun.(x) | acc]} end
        Enumerable.reduce(enumerable, {:cont, []}, reducer) |> elem(1) |> :lists.reverse()
      end

  Note that the user-supplied function is wrapped into a `t:reducer/0` function.
  The `t:reducer/0` function must return a tagged tuple after each step,
  as described in the `t:acc/0` type. At the end, `Enumerable.reduce/3`
  returns `t:result/0`.

  This protocol uses tagged tuples to exchange information between the
  reducer function and the data type that implements the protocol. This
  allows enumeration of resources, such as files, to

... [truncated 150202 chars] ...

 {:ok, Range.size(range)}
  end

  def slice(first.._//step = range) do
    {:ok, Range.size(range), &slice(first + &1 * step, step * &3, &2)}
  end

  slice =
    quote generated: true do
      slice(%{__struct__: Range, first: var!(first), last: var!(last)} = var!(range))
    end

  def unquote(slice) do
    step = if first <= last, do: 1, else: -1
    slice(Map.put(range, :step, step))
  end

  defp slice(current, _step, 1), do: [current]

  defp slice(current, step, remaining) when remaining > 1 do
    [current | slice(current + step, step, remaining - 1)]
  end
end
```

## Apply Minification Excerpt

```ex
defprotocol Enumerable do @moduledoc """ Enumerable protocol used by `Enum` and `Stream` modules. When you invoke a function in the `Enum` module,the first argument is usually a collection that must implement this protocol. For example,the expression `Enum.map([1,2,3],&(&1 * 2))` invokes `Enumerable.reduce/3` to perform the reducing operation that builds a mapped list by calling the mapping function `&(&1 * 2)` on every element in the collection and consuming the element with an accumulated list. Internally,`Enum.map/2` is implemented as follows:def map(enumerable,fun) do reducer = fn x,acc ->{:cont,[fun.(x) | acc]}end Enumerable.reduce(enumerable,{:cont,[]},reducer) |> elem(1) |>:lists.reverse() end Note that the user-supplied function is wrapped into a `t:reducer/0` function. The `t:reducer/0` function must return a tagged tuple after each step,as described in the `t:acc/0` type. At the end,`Enumerable.reduce/3` returns `t:result/0`. This protocol uses tagged tuples to exchange information between the reducer function and the data type that implements the protocol. This allows enumeration of resources,such as files,to be done efficiently while also guaranteeing the resource will be closed at the end of 

... [truncated 127339 chars] ...

nge,:step,step),value) end def member?(_,_value) do{:ok,false}end def count(range) do{:ok,Range.size(range)}end def slice(first.._//step = range) do{:ok,Range.size(range),&slice(first + &1 * step,step * &3,&2)}end slice = quote generated:true do slice(%{__struct__:Range,first:var!(first),last:var!(last)}= var!(range)) end def unquote(slice) do step = if first<= last,do:1,else:-1 slice(Map.put(range,:step,step)) end defp slice(current,_step,1),do:[current] defp slice(current,step,remaining) when remaining> 1 do [current | slice(current + step,step,remaining - 1)] end end
```

## Sync Minify Excerpt

```ex
defprotocol Enumerable do @moduledoc """ Enumerable protocol used by `Enum` and `Stream` modules. When you invoke a function in the `Enum` module,the first argument is usually a collection that must implement this protocol. For example,the expression `Enum.map([1,2,3],&(&1 * 2))` invokes `Enumerable.reduce/3` to perform the reducing operation that builds a mapped list by calling the mapping function `&(&1 * 2)` on every element in the collection and consuming the element with an accumulated list. Internally,`Enum.map/2` is implemented as follows:def map(enumerable,fun) do reducer = fn x,acc ->{:cont,[fun.(x) | acc]}end Enumerable.reduce(enumerable,{:cont,[]},reducer) |> elem(1) |>:lists.reverse() end Note that the user-supplied function is wrapped into a `t:reducer/0` function. The `t:reducer/0` function must return a tagged tuple after each step,as described in the `t:acc/0` type. At the end,`Enumerable.reduce/3` returns `t:result/0`. This protocol uses tagged tuples to exchange information between the reducer function and the data type that implements the protocol. This allows enumeration of resources,such as files,to be done efficiently while also guaranteeing the resource will be closed at the end of 

... [truncated 127339 chars] ...

nge,:step,step),value) end def member?(_,_value) do{:ok,false}end def count(range) do{:ok,Range.size(range)}end def slice(first.._//step = range) do{:ok,Range.size(range),&slice(first + &1 * step,step * &3,&2)}end slice = quote generated:true do slice(%{__struct__:Range,first:var!(first),last:var!(last)}= var!(range)) end def unquote(slice) do step = if first<= last,do:1,else:-1 slice(Map.put(range,:step,step)) end defp slice(current,_step,1),do:[current] defp slice(current,step,remaining) when remaining> 1 do [current | slice(current + step,step,remaining - 1)] end end
```

## Async Minify Excerpt

```ex
defprotocol Enumerable do @moduledoc """ Enumerable protocol used by `Enum` and `Stream` modules. When you invoke a function in the `Enum` module,the first argument is usually a collection that must implement this protocol. For example,the expression `Enum.map([1,2,3],&(&1 * 2))` invokes `Enumerable.reduce/3` to perform the reducing operation that builds a mapped list by calling the mapping function `&(&1 * 2)` on every element in the collection and consuming the element with an accumulated list. Internally,`Enum.map/2` is implemented as follows:def map(enumerable,fun) do reducer = fn x,acc ->{:cont,[fun.(x) | acc]}end Enumerable.reduce(enumerable,{:cont,[]},reducer) |> elem(1) |>:lists.reverse() end Note that the user-supplied function is wrapped into a `t:reducer/0` function. The `t:reducer/0` function must return a tagged tuple after each step,as described in the `t:acc/0` type. At the end,`Enumerable.reduce/3` returns `t:result/0`. This protocol uses tagged tuples to exchange information between the reducer function and the data type that implements the protocol. This allows enumeration of resources,such as files,to be done efficiently while also guaranteeing the resource will be closed at the end of 

... [truncated 127339 chars] ...

nge,:step,step),value) end def member?(_,_value) do{:ok,false}end def count(range) do{:ok,Range.size(range)}end def slice(first.._//step = range) do{:ok,Range.size(range),&slice(first + &1 * step,step * &3,&2)}end slice = quote generated:true do slice(%{__struct__:Range,first:var!(first),last:var!(last)}= var!(range)) end def unquote(slice) do step = if first<= last,do:1,else:-1 slice(Map.put(range,:step,step)) end defp slice(current,_step,1),do:[current] defp slice(current,step,remaining) when remaining> 1 do [current | slice(current + step,step,remaining - 1)] end end
```

## Symbols

```txt
5| defprotocol Enumerable do
```
