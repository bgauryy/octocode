**Short answer:** with a callback, `parser::parse` builds the value through the SAX handler `json_sax_dom_callback_parser`. That handler builds the DOM as usual, but it asks the callback about every event. Rejected values are never added, or are removed again, so the result contains only what the callback kept. All of this is in `include/nlohmann/detail/input/`.

**Entry point** (`parser.hpp:98-101`): if a callback is set, `parse` creates a `json_sax_dom_callback_parser` writing into `result` and runs `sax_parse_internal` with it. Without a callback it uses the plain `json_sax_dom_parser` (`parser.hpp:137`).

**Per-event mechanics** (`json_sax.hpp`):
- **Bookkeeping.** The constructor pushes `true` onto `keep_stack` (`:527`). `start_object` and `start_array` call the callback with `object_start` or `array_start` and push the result onto `keep_stack` (`:586-587`, `:707-708`). `ref_stack` holds a pointer to each open container, or `nullptr` if that container was rejected (`:595`, `:714`).
- **Scalars** (`handle_value`, `:1006`):
  - If the enclosing container was rejected (`keep_stack.back()` is false), the value is dropped and `{false, nullptr}` is returned (`:1012-1015`).
  - Otherwise it builds the value and calls the callback with the `value` event (`:1025`).
  - A rejected value is not added. If it was meant to be an object member, the placeholder that `key()` stored is removed (`:1028-1045`).
  - An accepted value becomes the root, is appended to the array, or is assigned to the object slot (`:1048-1097`).
- **Object keys** (`key`, `:620-648`): the callback is called with the `key` event and the result is pushed onto `key_keep_stack`. If the key is kept, a `discarded` placeholder is inserted at that key (`:644`). If the key is rejected, nothing is inserted, and the value that follows is dropped because `store_element` is false (`:1076-1083`).
- **Closing containers** (`end_object` `:650`, `end_array` `:745`): the callback is called with `object_end` or `array_end` on the finished container (`:654`, `:752`).
  - If it returns true, `set_parents()` is called on the container (`:680`, `:764`).
  - If it returns false, the container is overwritten with `discarded` (`:661`, `:777`).
  - The container is then popped, and `remove_discarded_value` (`:964-988`) removes the discarded entry from the parent. For an array it pops the last element if that element is discarded. For an object it erases the entry at that key if it is discarded.
- **Duplicate keys** (this checkout only): `key()` saves the previous value of a repeated key in `duplicate_key_stash` (`:636-642`). If the new value is rejected, `resolve_duplicate_key_stash` restores the old value instead of erasing the slot (`:908-927`).
- **Top-level result** (`parser.hpp:121-133`):
  - On a parse error, the result is set to `discarded`.
  - If the root itself was rejected and is left as `discarded`, it is converted to `null`.

**Uncertainty:** `end_array` has a `stored` flag (`:748`) and its tail logic at `:795-809` is slightly asymmetric from `end_object`. I read it but did not trace edge cases such as a rejected array nested in a rejected object. I also did not read `sax_parse_internal`, which drives these calls, or the remaining lines `:1101-1211` of `json_sax.hpp`.