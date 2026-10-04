**Short answer:** The parser builds the value through `json_sax_dom_callback_parser`, which is the SAX handler used whenever a callback is supplied. A rejected element is never added to its parent, or it is added as a `discarded` placeholder and erased as soon as its parent knows the outcome. If the root itself is rejected, `parse()` returns `null`. This checkout has been modified compared with stock nlohmann/json: it keeps extra stacks and a duplicate-key stash. All paths below are under `include/nlohmann/detail/input/`.

**Mechanism (`json_sax.hpp`)**
- **State.** Each parse keeps `ref_stack`, a stack of pointers to the containers being built (`json_sax.hpp:1103`). `keep_stack` records whether each open container was kept (`:1105`). `key_keep_stack` records whether each object key was kept (`:1107`). The handler also holds the user callback (`:1125`) and a `discarded` value (`:1128`).
- **Scalars (`handle_value`, `:1006`).**
  - If the enclosing container was already rejected (`!keep_stack.back()`), it returns `{false, nullptr}` without calling the callback (`:1012-1015`).
  - Otherwise it calls `callback(depth, parse_event_t::value, value)` (`:1025`).
  - If the callback rejects the value, nothing is stored. For an object member, the placeholder that `key()` already wrote is removed via `remove_discarded_value` (`:1030-1044`).
  - If the callback accepts it and `ref_stack` is empty, the value becomes `root` (`:1048-1052`).
  - Otherwise it is appended to the parent array (`:1065-1068`) or assigned into the object slot (`:1072-1097`).
- **Object keys (`key`, `:622-647`).**
  - The callback is called with `parse_event_t::key` and its result is pushed onto `key_keep_stack`.
  - If the key is kept, a `discarded` placeholder is inserted at that key (`:644`) and its address is remembered in `object_element`.
  - If the key is rejected, nothing is inserted. The later value is dropped because `key_keep_stack.back()` is false (`:1076-1082`).
- **Containers (`start_object` at `:583`, `start_array` at `:705`).**
  - The callback gets `object_start` or `array_start` and the result goes onto `keep_stack` (`:586-587`, `:707-708`).
  - `handle_value(..., skip_callback=true)` creates the empty container. `ref_stack` gets a pointer to it, or `nullptr` if it isn't stored (`:595`, `:714`).
  - Children of a rejected container see a `nullptr` top of `ref_stack` and are skipped (`:1056-1059`).
- **End of container (`end_object` at `:650`, `end_array` at `:745`).**
  - The callback gets `object_end` or `array_end`.
  - If it returns false, the container is overwritten with `discarded` (`:661`, `:777`), unless a stashed duplicate-key value is restored instead.
  - After popping the stacks, the discarded child is removed from its parent. For arrays this is `pop_back()` (`:798-801`). For objects it is `remove_discarded_value`, which erases the key (`:696-700`, `:802-808`).
  - `remove_discarded_value` (`:947-985`) only checks the last array element or the one object key, so it is O(1) for arrays and O(log n) for objects. If it finds no discarded value there, none was stored and it does nothing.
- **Duplicate keys.** If a key repeats, `key()` stashes the old value in `duplicate_key_stash` (`:634-643`). If the new value is rejected, `resolve_duplicate_key_stash` (`:908-927`) restores the old value instead of erasing it.

**Top-level result (`parser.hpp`)**
- After the SAX parse, if the handler reports an error, `result` is set to `value_t::discarded` (`parser.hpp:121-125`).
- If there was no error but the root was discarded by the callback, `result` is set to `nullptr` (`parser.hpp:128-133`). The caller therefore gets `null`, not a discarded value.

**Uncertainty:** I read the code but did not run any tests. I did not read the `parser.hpp` code that chooses between the callback handler and the plain DOM handler. I did not read the `sdp` declaration above `parser.hpp:108` either. I only saw the plain-DOM branch at `parser.hpp:136-138`.