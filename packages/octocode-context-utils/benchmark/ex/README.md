# Elixir (.ex)

Source sample: `ex/elixir-enum.ex`

Strategy: `aggressive`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 154291 | - | - |
| content-view | 152002 | 1.5% | 3.609 ms |
| applyMinification | 129139 | 16.3% | 3.891 ms |
| sync minify | 129139 | 16.3% | 4.049 ms |
| async minify | 129139 | 16.3% | 3.956 ms |
| symbols | 28991 | 81.2% | 0.39 ms |

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
  19|       def map(enumerable, fun) do
  51|         def count(struct), do: {:ok, length(struct.items)}
  52|         def member?(struct, value), do: {:ok, value in struct.items}
  53|         def slice(struct), do: {:error, __MODULE__}
  54|         def reduce(struct, acc, fun), do: Enumerable.List.reduce(struct.items, acc, fun)
  67|       def integers_to_strings(integers) do
 177|       def reduce(_list, {:halt, acc}, _fun), do: {:halted, acc}
 178|       def reduce(list, {:suspend, acc}, fun), do: {:suspended, acc, &reduce(list, &1, fun)}
 179|       def reduce([], {:cont, acc}, _fun), do: {:done, acc}
 180|       def reduce([head | tail], {:cont, acc}, fun), do: reduce(tail, fun.(head, acc), fun)
 184|   def reduce(enumerable, acc, fun)
 196|   def count(enumerable)
 212|   def member?(enumerable, element)
 248|   def slice(enumerable)
 251| defmodule Enum do
 355|   def all?(enumerable) when is_list(enumerable) do
 359|   def all?(enumerable) do
 391|   def all?(enumerable, fun) when is_list(enumerable) do
 395|   def all?(first..last//step, fun) do
 399|   def all?(enumerable, fun) do
 425|   def any?(enumerable) when is_list(enumerable) do
 429|   def any?(enumerable) do
 456|   def any?(enumerable, fun) when is_list(enumerable) do
 460|   def any?(first..last//step, fun) do
 464|   def any?(enumerable, fun) do
 496|   def at(enumerable, index, default \\ nil) when is_integer(index) do
 505|   def chunk(enumerable, count), do: chunk(enumerable, count, count, nil)
 509|   def chunk(enum, n, step) do
 515|   def chunk(enumerable, count, step, leftover) do
 524|   def chunk_every(enumerable, count), do: chunk_every(enumerable, count, count, [])
 568|   def chunk_every(enumerable, count, step, leftover

... [truncated 26391 chars] ...

5161|   def reduce(first..last//step, acc, fun) do
5175|   def unquote(reduce) do
5180|   defp reduce(_first, _last, {:halt, acc}, _fun, _step) do
5184|   defp reduce(first, last, {:suspend, acc}, fun, step) do
5188|   defp reduce(first, last, {:cont, acc}, fun, step)
5194|   defp reduce(_, _, {:cont, acc}, _fun, _up) do
5198|   def member?(first..last//step, value) when is_integer(value) and step > 0 do
5202|   def member?(first..last//step, value) when is_integer(value) and step < 0 do
5207|   def member?(%{__struct__: Range, first: first, last: last} = range, value)
5213|   def member?(_, _value) do
5217|   def count(range) do
5221|   def slice(first.._//step = range) do
5232|   def unquote(slice) do
5237|   defp slice(current, _step, 1), do: [current]
5239|   defp slice(current, step, remaining) when remaining > 1 do
```
