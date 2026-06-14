# PHP (.php)

Source sample: `php/Arr.php`

Strategy: `conservative`

| Tool | Bytes | Cut | Time |
| --- | ---: | ---: | ---: |
| input | 35469 | - | - |
| content-view | 20778 | 41.4% | 1.703 ms |
| applyMinification | 20842 | 41.2% | 1.582 ms |
| sync minify | 20842 | 41.2% | 1.65 ms |
| async minify | 20842 | 41.2% | 1.637 ms |
| symbols | 4562 | 87.1% | 1.029 ms |

## Notes

- conservative text strategy.

## Before Excerpt

```php
<?php

namespace Illuminate\Support;

use ArgumentCountError;
use ArrayAccess;
use Closure;
use Illuminate\Contracts\Support\Arrayable;
use Illuminate\Contracts\Support\Jsonable;
use Illuminate\Support\Traits\Macroable;
use InvalidArgumentException;
use JsonSerializable;
use Random\Randomizer;
use Traversable;
use WeakMap;

class Arr
{
    use Macroable;

    /**
     * Determine whether the given value is array accessible.
     *
     * @param  mixed  $value
     * @return bool
     */
    public static function accessible($value)
    {
        return is_array($value) || $value instanceof ArrayAccess;
    }

    /**
     * Determine whether the given value is arrayable.
     *
     * @param  mixed  $value
     * @return ($value is array
     *     ? true
     *     : ($value is \Illuminate\Contracts\Support\Arrayable
     *         ? true
     *         : ($value is \Traversable
     *             ? true
     *             : ($value is \Illuminate\Contracts\Support\Jsonable
     *                 ? true
     *                 : ($value is \JsonSerializable ? true : false)
     *             )
     *         )
     *     )
     * )
     */
    public static function arrayable($value)
    {
        return

... [truncated 33669 chars] ...

     return static::where($array, fn ($value) => ! is_null($value));
    }

    /**
     * If the given value is not an array and not null, wrap it in one.
     *
     * @template TKey of array-key = array-key
     * @template TValue
     *
     * @param  array<TKey, TValue>|TValue|null  $value
     * @return ($value is null ? array{} : ($value is array ? array<TKey, TValue> : array{TValue}))
     */
    public static function wrap($value)
    {
        if (is_null($value)) {
            return [];
        }

        return is_array($value) ? $value : [$value];
    }
}

```

## Content-View Excerpt

```php
<?php

namespace Illuminate\Support;

use ArgumentCountError;
use ArrayAccess;
use Closure;
use Illuminate\Contracts\Support\Arrayable;
use Illuminate\Contracts\Support\Jsonable;
use Illuminate\Support\Traits\Macroable;
use InvalidArgumentException;
use JsonSerializable;
use Random\Randomizer;
use Traversable;
use WeakMap;

class Arr
{
    use Macroable;

    public static function accessible($value)
    {
        return is_array($value) || $value instanceof ArrayAccess;
    }

    public static function arrayable($value)
    {
        return is_array($value)
            || $value instanceof Arrayable
            || $value instanceof Traversable
            || $value instanceof Jsonable
            || $value instanceof JsonSerializable;
    }

    public static function add($array, $key, $value)
    {
        if (is_null(static::get($array, $key))) {
            static::set($array, $key, $value);
        }

        return $array;
    }

    public static function array(ArrayAccess|array $array, string|int|null $key, ?array $default = null): array
    {
        $value = Arr::get($array, $key, $default);

        if (! is_array($value)) {
            throw new InvalidArgumentException(
                sprin

... [truncated 18978 chars] ...

     $failed = [];

        foreach ($array as $key => $item) {
            if ($callback($item, $key)) {
                $passed[$key] = $item;
            } else {
                $failed[$key] = $item;
            }
        }

        return [$passed, $failed];
    }

    public static function whereNotNull($array)
    {
        return static::where($array, fn ($value) => ! is_null($value));
    }

    public static function wrap($value)
    {
        if (is_null($value)) {
            return [];
        }

        return is_array($value) ? $value : [$value];
    }
}
```

## Apply Minification Excerpt

```php
<?php

namespace Illuminate\Support;

use ArgumentCountError;
use ArrayAccess;
use Closure;
use Illuminate\Contracts\Support\Arrayable;
use Illuminate\Contracts\Support\Jsonable;
use Illuminate\Support\Traits\Macroable;
use InvalidArgumentException;
use JsonSerializable;
use Random\Randomizer;
use Traversable;
use WeakMap;

class Arr
{
    use Macroable;


    public static function accessible($value)
    {
        return is_array($value) || $value instanceof ArrayAccess;
    }


    public static function arrayable($value)
    {
        return is_array($value)
            || $value instanceof Arrayable
            || $value instanceof Traversable
            || $value instanceof Jsonable
            || $value instanceof JsonSerializable;
    }


    public static function add($array, $key, $value)
    {
        if (is_null(static::get($array, $key))) {
            static::set($array, $key, $value);
        }

        return $array;
    }


    public static function array(ArrayAccess|array $array, string|int|null $key, ?array $default = null): array
    {
        $value = Arr::get($array, $key, $default);

        if (! is_array($value)) {
            throw new InvalidArgumentException(
                s

... [truncated 19042 chars] ...

   $failed = [];

        foreach ($array as $key => $item) {
            if ($callback($item, $key)) {
                $passed[$key] = $item;
            } else {
                $failed[$key] = $item;
            }
        }

        return [$passed, $failed];
    }


    public static function whereNotNull($array)
    {
        return static::where($array, fn ($value) => ! is_null($value));
    }


    public static function wrap($value)
    {
        if (is_null($value)) {
            return [];
        }

        return is_array($value) ? $value : [$value];
    }
}
```

## Sync Minify Excerpt

```php
<?php

namespace Illuminate\Support;

use ArgumentCountError;
use ArrayAccess;
use Closure;
use Illuminate\Contracts\Support\Arrayable;
use Illuminate\Contracts\Support\Jsonable;
use Illuminate\Support\Traits\Macroable;
use InvalidArgumentException;
use JsonSerializable;
use Random\Randomizer;
use Traversable;
use WeakMap;

class Arr
{
    use Macroable;


    public static function accessible($value)
    {
        return is_array($value) || $value instanceof ArrayAccess;
    }


    public static function arrayable($value)
    {
        return is_array($value)
            || $value instanceof Arrayable
            || $value instanceof Traversable
            || $value instanceof Jsonable
            || $value instanceof JsonSerializable;
    }


    public static function add($array, $key, $value)
    {
        if (is_null(static::get($array, $key))) {
            static::set($array, $key, $value);
        }

        return $array;
    }


    public static function array(ArrayAccess|array $array, string|int|null $key, ?array $default = null): array
    {
        $value = Arr::get($array, $key, $default);

        if (! is_array($value)) {
            throw new InvalidArgumentException(
                s

... [truncated 19042 chars] ...

   $failed = [];

        foreach ($array as $key => $item) {
            if ($callback($item, $key)) {
                $passed[$key] = $item;
            } else {
                $failed[$key] = $item;
            }
        }

        return [$passed, $failed];
    }


    public static function whereNotNull($array)
    {
        return static::where($array, fn ($value) => ! is_null($value));
    }


    public static function wrap($value)
    {
        if (is_null($value)) {
            return [];
        }

        return is_array($value) ? $value : [$value];
    }
}
```

## Async Minify Excerpt

```php
<?php

namespace Illuminate\Support;

use ArgumentCountError;
use ArrayAccess;
use Closure;
use Illuminate\Contracts\Support\Arrayable;
use Illuminate\Contracts\Support\Jsonable;
use Illuminate\Support\Traits\Macroable;
use InvalidArgumentException;
use JsonSerializable;
use Random\Randomizer;
use Traversable;
use WeakMap;

class Arr
{
    use Macroable;


    public static function accessible($value)
    {
        return is_array($value) || $value instanceof ArrayAccess;
    }


    public static function arrayable($value)
    {
        return is_array($value)
            || $value instanceof Arrayable
            || $value instanceof Traversable
            || $value instanceof Jsonable
            || $value instanceof JsonSerializable;
    }


    public static function add($array, $key, $value)
    {
        if (is_null(static::get($array, $key))) {
            static::set($array, $key, $value);
        }

        return $array;
    }


    public static function array(ArrayAccess|array $array, string|int|null $key, ?array $default = null): array
    {
        $value = Arr::get($array, $key, $default);

        if (! is_array($value)) {
            throw new InvalidArgumentException(
                s

... [truncated 19042 chars] ...

   $failed = [];

        foreach ($array as $key => $item) {
            if ($callback($item, $key)) {
                $passed[$key] = $item;
            } else {
                $failed[$key] = $item;
            }
        }

        return [$passed, $failed];
    }


    public static function whereNotNull($array)
    {
        return static::where($array, fn ($value) => ! is_null($value));
    }


    public static function wrap($value)
    {
        if (is_null($value)) {
            return [];
        }

        return is_array($value) ? $value : [$value];
    }
}
```

## Symbols

```txt
   3| namespace Illuminate\Support;
   5| use ArgumentCountError;
   6| use ArrayAccess;
   7| use Closure;
   8| use Illuminate\Contracts\Support\Arrayable;
   9| use Illuminate\Contracts\Support\Jsonable;
  10| use Illuminate\Support\Traits\Macroable;
  11| use InvalidArgumentException;
  12| use JsonSerializable;
  13| use Random\Randomizer;
  14| use Traversable;
  15| use WeakMap;
  17| class Arr
  19|     use Macroable;
  27|     public static function accessible($value)
  50|     public static function arrayable($value)
  67|     public static function add($array, $key, $value)
  81|     public static function array(ArrayAccess|array $array, string|int|null $key, ?array $default = null): array
  99|     public static function boolean(ArrayAccess|array $array, string|int|null $key, ?bool $default = null): bool
 118|     public static function collapse($array)
 141|     public static function crossJoin(...$arrays)
 171|     public static function divide($array)
 184|     public static function dot($array, $prepend = '', $depth = INF)
 214|     public static function undot($array)
 232|     public static function except($array, $keys)
 247|     public static function exceptValues($array, $values, $strict = false)
 263|     public static function exists($array, $key)
 292|     public static function first($array, ?callable $callback = null, $default = null)
 329|     public static function last($array, ?callable $callback = null, $default = null)
 345|     public static function take($array, $limit)
 361|     public static function flatten($array, $depth = INF)
 389|     public static function float(ArrayAccess|array $array, string|int|null $key, ?float $default = null): float
 409|     public static function forget(&$array, $keys)
 45

... [truncated 1962 chars] ...

tatic function sort($array, $callback = null)
1112|     public static function sortDesc($array, $callback = null)
1128|     public static function sortRecursive($array, $options = SORT_REGULAR, $descending = false)
1159|     public static function sortRecursiveDesc($array, $options = SORT_REGULAR)
1169|     public static function string(ArrayAccess|array $array, string|int|null $key, ?string $default = null): string
1188|     public static function toCssClasses($array)
1211|     public static function toCssStyles($array)
1238|     public static function where($array, callable $callback)
1253|     public static function reject($array, callable $callback)
1268|     public static function partition($array, callable $callback)
1290|     public static function whereNotNull($array)
1304|     public static function wrap($value)
```
