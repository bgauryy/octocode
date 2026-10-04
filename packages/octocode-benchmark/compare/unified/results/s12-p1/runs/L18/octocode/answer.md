**Short answer:** When a callback is given, `parser::parse` uses a different SAX handler, `json_sax_dom_callback_parser`. It builds the DOM incrementally and asks the callback about each event. Rejected elements are never stored, or are removed again right away. Whatever remains is the result. A rejected top-level value becomes `null`.

All paths below are under `include/nlohmann/detail/input/`. I read this at the pinned checkout and did not run it.

**1. Choosing the handler**
- `parser.hpp:98-101`: if `callback` is set, `parse` creates `json_sax_dom_callback_parser sdp(result, callback, …)` and calls `sax_parse_internal(&sdp)`. The no-callback path uses the plain `json_sax_dom_parser` (`parser.hpp:137`).

**2. Scalars (`handle_value`, `json_sax.hpp:1006-1098`)**
- If the enclosing container was already rejected (`!keep_stack.back()`), the value is skipped without calling the callback (`:1012-1015`).
- Otherwise the value is built and passed to `callback(depth, parse_event_t::value, value)` (`:1025`).
- If the callback returns false, nothing is added (`:1028-1046`). For an object member, the placeholder that `key()` stored is removed (`:1032-1044`).
- If it returns true, the value becomes the root (`:1048-1052`), is appended to the array (`:1065-1069`), or is assigned to the object slot (`:1086`).

**3. Object keys (`json_sax.hpp:625-644`)**
- `key()` calls the callback with `parse_event_t::key` and records the result in `key_keep_stack`.
- If the key is kept, it inserts a `discarded` placeholder at that key (`:644`).
- `handle_value` then either overwrites the placeholder or removes it (`:1042`, `:1076-1086`).
- If the key is rejected, the member is simply not stored (`:1080-1083`).

**4. Containers (`start_object`/`end_object`, `start_array`/`end_array`)**
- `start_*` calls the callback (`:586`, `:707`) and pushes the result on `keep_stack` (`:587`, `:708`).
- It also pushes onto `ref_stack` a pointer to the stored container, or `nullptr` if the container was rejected (`:595`, `:714`). Children of a `nullptr` container are skipped.
- `end_object` calls the callback with `object_end` (`:654`). If that returns false, the stored object is overwritten with `discarded` (`:661`). The same happens for arrays at `:777`.
- The stacks are then popped (`:691-692`, `:790-791`).
- The discarded child is removed from its parent in two ways:
  - Objects use `remove_discarded_value(*ref_stack.back(), object_key)` (`:699`, `:807`).
  - Arrays use `array->pop_back()` (`:800`).
- `remove_discarded_value` (`:964-988`) only inspects the last array element, or the object entry for the key. It erases that entry only if it `is_discarded()`.

**5. Duplicate keys**
- If a rejected value sits in a duplicate key's slot, `resolve_duplicate_key_stash(..., true)` restores the previous value instead of erasing it (`:659`, `:775`, `:982`). I did not read the stash implementation.

**6. Final fix-up (`parser.hpp:121-133`)**
- If the parse errored, `result` becomes `discarded`.
- If the root value itself was rejected and is still `discarded`, `result = nullptr`. A fully rejected document therefore comes back as `null`.

**Uncertainty:** I did not read lines 536-939 of `json_sax.hpp` in full (the `null`/`boolean`/`number`/`string` handlers, `parse_error`, and the stash implementation). They presumably all go through `handle_value`.