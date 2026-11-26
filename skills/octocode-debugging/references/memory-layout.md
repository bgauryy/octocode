Load when a data structure costs more memory or cache misses than its element count explains: node-based or boxed collections, retained slices, struct padding.

# Collection and object layout

## Lists: linked nodes vs contiguous storage

| Structure | Memory cost | Profile signature | Watch |
|---|---|---|---|
| Linked list, tree or graph of separately allocated nodes | Allocator header plus pointers per node | Cache misses and pointer-chasing loads in CPU samples; GC scan time | Prefer arrays, or nodes in an arena addressed by index |
| Array, vector, slice | Dense; spare capacity up to the growth factor | Large `memcpy`/realloc frames during growth | Growth moves elements: pointers, iterators and references into it become invalid |
| Go slice | Header shares a backing array | Small slices retaining a huge array | `append` to a subslice can overwrite the parent's elements; copy out (`slices.Clone`) to release the large array |
| Boxed element lists (`ArrayList<Integer>`, Python `list` of ints) | One object per element plus a pointer | Object count far above the element count | Primitive arrays, `array`, NumPy, typed arrays |
| JS arrays | Packed elements while types stay uniform | Sudden slowdown after holes or mixed types | `TypedArray` for numbers; avoid `delete arr[i]` |
| Substrings and slices of large strings or buffers | Can retain the parent (V8 sliced strings, Go substrings, Node `buf.subarray` and its deprecated alias `buf.slice`) | Retainer path from a small string to a large one | Copy the slice when you keep it longer than the parent |

## Structs and object layout

Fields are aligned to their natural alignment and the struct size is rounded up to its largest alignment, so field order changes size:

```c
struct Bad  { char a; double b; char c; };  /* 24 bytes on x86-64: 7 + 7 bytes padding */
struct Good { double b; char a; char c; };  /* 16 bytes */
```

Check with `sizeof`/`offsetof` or `pahole -C Bad ./binary` (needs debug info) before reordering.

| Language | Layout rule | Inspect | Memory lever |
|---|---|---|---|
| C, C++ | Declaration order with padding | `sizeof`, `offsetof`, `pahole` | Order fields large to small; bitfields for flags; packing (`__attribute__((packed))`, `#pragma pack`) makes misaligned access slow or faulting on some CPUs |
| Rust | Default `repr` may reorder fields; `#[repr(C)]` keeps declaration order for FFI | `std::mem::size_of::<T>()`, `-Zprint-type-sizes` (nightly) | `Box` large enum variants; niches keep `Option<Box<T>>` pointer-sized; references to packed fields are rejected |
| Go | Declaration order with padding | `unsafe.Sizeof`, the `fieldalignment` analyzer | Order fields; pointer-free types are not scanned by the GC; `go build -gcflags=-m` shows what escapes to the heap |
| Java | Object header (about 12–16 bytes) plus aligned fields; compact headers shrink it (`-XX:+UseCompactObjectHeaders`, a product option in JDK 25 and the default from JDK 27) | JOL (Java Object Layout) | Primitives over boxes; fewer small objects |
| C# | `struct` is a value, `class` a heap object; boxing allocates | `Unsafe.SizeOf<T>()`, `StructLayout` | Value types in hot arrays; avoid boxing in generic paths |
| Python | Each instance carries a `__dict__` unless `__slots__` is declared | `sys.getsizeof` (shallow only), `tracemalloc` | `__slots__` for many small objects; arrays for numbers |
| JavaScript | Objects share hidden classes (shapes) when built the same way | DevTools heap snapshot shallow sizes | Same constructor and property order; avoid `delete` on hot objects |

Large structs passed or returned by value show up as `memcpy` in CPU samples; pass a reference or pointer. Split hot fields from cold ones and use struct-of-arrays when a loop reads one field across many items. Pad per-thread counters to the cache line to avoid false sharing.
