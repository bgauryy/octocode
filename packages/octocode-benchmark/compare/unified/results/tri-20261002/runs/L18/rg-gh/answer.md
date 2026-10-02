**Short answer:** the callback parser builds the result with the SAX handler `json_sax_dom_callback_parser` (`include/nlohmann/detail/input/json_sax.hpp:509`). Each value is built, passed to the callback, and attached to its parent only if the callback accepts it. Rejected elements are never added, or are added as a temporary placeholder and then removed. If the whole top-level value is rejected, the result is `null`.

**Flow**
- `parser::parse` uses this handler whenever a callback is set (`parser.hpp:98-100`).
  - If parsing errored, `result` becomes `value_t::discarded` (`parser.hpp:121-124`).
  - If the top-level value was discarded by the callback, `result` is set to `nullptr`, i.e. JSON `null` (`parser.hpp:128-132`).

**Scalars: `handle_value` (`json_sax.hpp:1006-1100`)**
- If the parent container was already rejected (`keep_stack.back()` is false), the value is dropped without calling the callback (`:1012-1015`).
- Otherwise the value is built and passed to the callback with `parse_event_t::value` (`:1025`).
- If the callback rejects it, `handle_value` returns `{false, nullptr}` (`:1030-1046`).
  - For an object member, it first removes the placeholder that `key()` stored (`:1032-1043`).
- If the callback accepts it:
  - With an empty `ref_stack`, it becomes `root` (`:1048-1052`).
  - In an array, it is `emplace_back`ed (`:1065-1068`).
  - In an object, it is assigned to the slot `object_element` that `key()` prepared (`:1072-1085`).

**Keys: `key()` (`:624-644`)**
- It calls the callback with `parse_event_t::key` and records the result in `key_keep_stack` (`:625-626`).
- If the key is kept, it inserts a `discarded` placeholder at that key and remembers the slot in `object_element` (`:632-644`).
- If the key is rejected, nothing is inserted, and the later value for that key is dropped.

**Containers: start and end**
- `start_object` and `start_array` call the callback with `object_start` or `array_start`.
  - The result goes on `keep_stack` (`:586-587`, `:707-708`).
  - The container is pushed onto `ref_stack` (`:595`, `:714`). That entry is `nullptr` if the container isn't stored (`:597`, `:716`).
- `end_object` and `end_array` call the callback with `object_end` or `array_end` and the finished container (`:654`, `:752`).
  - If it rejects the container, the stored value is overwritten with `discarded` (`:661`, `:777`).
    - Before that, `resolve_duplicate_key_stash(..., true)` tries to restore an earlier value for a duplicate key (`:659`, `:775`).
  - The container is then popped from `ref_stack` and `keep_stack` (`:691-692`, `:790-791`).
  - If the container was discarded or never stored, it is removed from its parent (`:696-699`, `:795-807`). A discarded array element is `pop_back`ed. A discarded object member is removed by `remove_discarded_value(parent, key)`, which erases it only if it is a `discarded` placeholder (`:964-978`).

**Net effect:** rejected scalars, keys, and containers are absent from the output. Their parent array or object is just smaller. A rejected root becomes `null`.

**Uncertainty:** I read the code but didn't run it. I did not read the duplicate-key stash logic (`resolve_duplicate_key_stash`) in detail. I only saw where it is called.