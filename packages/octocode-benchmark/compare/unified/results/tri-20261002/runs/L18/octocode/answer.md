**Answer:** With a callback, `parser::parse` builds the result using a SAX handler, `json_sax_dom_callback_parser`. It adds each value to the tree as it is parsed. When the callback rejects a value, the handler removes it again, or never adds it. If the whole top-level value is rejected, the result is `null`.

**Setup.** `parser::parse` creates `json_sax_dom_callback_parser sdp(result, callback, ...)` and runs `sax_parse_internal(&sdp)` (`parser.hpp:98-101`). `root` is a reference to `result`. The handler keeps `ref_stack` (pointers to the open containers), `keep_stack` (initialised with `true`, `json_sax.hpp:527`), `key_keep_stack` and `key_stack`.

**Scalars.** Scalars go through `handle_value` (`json_sax.hpp:1006`):
- If the enclosing container was already rejected (`!keep_stack.back()`), the value is skipped without calling the callback (`:1012-1015`).
- Otherwise it calls `callback(depth, parse_event_t::value, value)` (`:1025`).
- If the callback returns false, nothing is stored. In an object, the placeholder that `key()` wrote is removed (`:1030-1045`).
- If it is kept, three cases apply:
  - With an empty `ref_stack`, it becomes `root` (`:1048-1051`).
  - In an array, it is appended with `emplace_back` (`:1065-1068`).
  - In an object, it is assigned to `*object_element` (`:1086`).

**Object keys.** `key()` (`:620`) calls the callback with `parse_event_t::key`. If that is kept, it inserts `obj[val] = discarded`, a placeholder, and stores a pointer to it in `object_element` (`:644`). The later value then overwrites the placeholder. If the callback rejects the key, `key_keep_stack` records false, and the value is not stored (`:1080-1083`).

**Containers.**
- `start_object` and `start_array` call the callback with the start event and push the result onto `keep_stack` (`:586-587`, `:707-708`).
- They add an empty container through `handle_value(..., skip_callback=true)` (`:594`, `:713`).
- If the start event is rejected, `handle_value` returns a null pointer and `ref_stack` gets a null entry. Everything inside that container is then skipped.
- `end_object` and `end_array` call the callback with `object_end` or `array_end`, passing the finished container (`:654`, `:752`).
- If that returns false, the container is overwritten with `discarded` (`:661`, `:777`).
- After popping the stacks, the parent removes the discarded child:
  - For an array, `pop_back()` (`:800`).
  - For an object, `remove_discarded_value` erases the key (`:699`, `:807`, `:964-988`).
  - `remove_discarded_value` only checks the last array element or the one key, so it is O(1) or O(log n).
- If the container is kept, `set_parents()` is called (`:680`, `:764`).

**Duplicate keys.** If a key is repeated, `key()` stashes the old value in `duplicate_key_stash` (`:642`). If the new value is then rejected, the old value is restored instead of the member being erased (`resolve_duplicate_key_stash`, `:908-927`; used at `:659`, `:775`, `:982`).

**Final result.** After parsing (`parser.hpp:121-133`):
- If a parse error occurred, `result` is set to `value_t::discarded`. With `allow_exceptions` on, the error is thrown instead (`json_sax.hpp:820-822`).
- If the top-level value was rejected, so `result` is still discarded, it is set to `nullptr`.

**Uncertainty.** I did not read `sax_parse_internal` or the `basic_json::parse` wrapper. The `JSON_DIAGNOSTIC_POSITIONS` code only sets position data and does not change which values are kept.