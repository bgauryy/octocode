**Short answer:** `parser::parse` swaps in a SAX handler, `json_sax_dom_callback_parser`, whenever a callback is given. The handler builds the result in `root` as events arrive. It calls the callback for each event. A rejected element is never added, or is removed again straight away. If the top-level value is rejected, the result is `null`.

All lines are in `include/nlohmann/detail/input/json_sax.hpp` unless noted.

**1. Entry point.**
- `parser.hpp:98-101`: if `callback` is set, `parse()` creates `json_sax_dom_callback_parser sdp(result, callback, ...)` and runs `sax_parse_internal(&sdp)`. The SAX handler writes into `result` through its `root` reference.
- `parser.hpp:128-133`: after parsing, "set top-level value to null if it was discarded by the callback", via `if (result.is_discarded()) result = nullptr;`.

**2. Scalars (`handle_value`, lines 1006-1098).**
- If `keep_stack.back()` is false, the enclosing container was rejected, so the value is dropped at once (1012-1015).
- Otherwise the value is built and `callback(depth, parse_event_t::value, value)` is called (1025).
- If the callback returns false (1028-1046) and the parent is an object, `key()` has already stored a placeholder. The code pops `key_keep_stack` and `key_stack` and calls `remove_discarded_value` to erase that placeholder. It then returns `{false, nullptr}`, so the value is never stored.
- If it is kept, it is stored in one of three places:
  - as `root` when `ref_stack` is empty (1048-1052);
  - appended to the parent array (1065-1069);
  - assigned to `*object_element` for an object member (1085-1086).
- If the parent container was itself rejected, `ref_stack.back()` is null and the value is skipped (1056-1059).

**3. Object keys (`key`, lines 620-648).**
- The callback is called with the `key` event, and its result is pushed on `key_keep_stack`.
- If the key is kept, `obj[val] = discarded` is written as a placeholder and `object_element` points at it (644). If the key is rejected, no placeholder is written, and the later value is dropped because `store_element` is false (1076-1083).
- Duplicate keys: the previous value is saved in `duplicate_key_stash` (642). If the new value is rejected, it is restored instead of erased (`resolve_duplicate_key_stash`, 908-927; used at 982).

**4. Containers.**
- `start_object` and `start_array` (583-595, 705-714) call the callback with `object_start` or `array_start`. They push the result on `keep_stack`, then call `handle_value(..., skip_callback=true)`. The pointer from that call is pushed on `ref_stack` and is null if the container is not stored.
- `end_object` and `end_array` (650-703, 745-812) call the callback with `object_end` or `array_end` on the finished container.
  - If it is rejected, the container is overwritten with `discarded` (661, 777). A pending duplicate-key restore takes precedence over this.
  - If it is accepted, `set_parents()` is called (680, 764).
  - After popping the stacks, the discarded entry is removed from the parent. For an array this is `array->pop_back()` (800) or `remove_discarded_value` when the last element is discarded. For an object it is `remove_discarded_value(parent, key)` (699, 807).
- `remove_discarded_value` (964-988) only looks at the last array element or the one key. It erases that entry only if it `is_discarded()`, so the filtering parse stays cheap.

**Net effect:** rejected scalars, keys, and containers are absent from the result. Array elements shift down, and object members are not created. A rejected root becomes `null`.

**Uncertainty:** I read the code but did not run it. This checkout looks modified relative to upstream. The `key_stack`, `container_key_stack`, and `duplicate_key_stash` mechanisms are more elaborate than I remember upstream being, so I described what is in this tree.