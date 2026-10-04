When a parser callback rejects an element, the result is built by the SAX-style DOM builder `json_sax_dom_callback_parser` in `include/nlohmann/detail/input/json_sax.hpp`. It first inserts rejected values, or placeholders for them, and then removes them again. If the top-level value itself is rejected, the result is `null`. I read the code but did not run it.

**Setup.** `parser::parse` uses this builder when a callback is supplied. The builder keeps parallel stacks:
- `ref_stack` holds a pointer to each open container, or `nullptr` if that container was rejected (`:595`, `:714`).
- `keep_stack` holds the callback's keep/discard decision for each open container (`:527`, `:587`).
- `key_keep_stack` and `key_stack` hold the keep decision and name of each pending object key (`:626-629`).

**Values** (`handle_value`, `:1006-1098`):
- If the enclosing container was already rejected (`!keep_stack.back()`), the value is dropped without calling the callback (`:1012-1015`).
- Otherwise the callback is called with `parse_event_t::value` (`:1025`).
- If the callback rejects the value, nothing is added. For an object member, the placeholder that `key()` wrote is erased with `remove_discarded_value` (`:1028-1045`).
- If the value is kept, it becomes the root (`:1048-1052`), is appended to the array (`:1065-1069`), or is assigned over the key's placeholder in the object (`:1086`).

**Object keys** (`key()`, `:620-648`):
- The callback is called with `parse_event_t::key`.
- If the key is kept, `key()` inserts `obj[val] = discarded` as a placeholder.
- A later accepted value overwrites the placeholder. A rejected value causes it to be erased.
- For a duplicate key, the previous value is stashed (`:642`) and restored if the new value is rejected.

**Containers** (`start_object`/`end_object`, `start_array`/`end_array`):
- Containers are inserted at the start (`:586-595`), and the callback is called again at `object_end`/`array_end`.
- If the end callback returns false, the container is overwritten with `discarded` (`:661`, `:777`). The exception is a pending duplicate-key restore.
- After popping the stacks, the discarded slot is removed from the parent:
  - `remove_discarded_value` (`:964-988`) pops a trailing discarded array element or erases the discarded object key.
  - `end_array` also pops directly when the parent is an array (`:798-801`).
- Kept containers get `set_parents()` (`:680`, `:764`).

**Top level** (`parser.hpp:121-133`):
- If parsing errors, the result is `value_t::discarded`.
- If the callback rejected the root value, the result is still discarded, so it is set to `nullptr`.

The result is therefore the original document with every rejected element removed. An object with all its members rejected becomes `{}`, and a rejected root becomes `null`.

**Not read:** I did not read `start_array`'s body or the `:536-579` and `:813-994` ranges, except the `remove_discarded_value` portion.