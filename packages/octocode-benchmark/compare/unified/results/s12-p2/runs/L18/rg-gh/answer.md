**Short answer:** The parser builds the result with `json_sax_dom_callback_parser` (`include/nlohmann/detail/input/json_sax.hpp:509`). It calls the callback for each event as it goes, and a rejected value is never kept, or is added and then removed. If the top-level value is rejected, the result is `null`. If there is a parse error, the result is `discarded`. I read the code but did not run it.

**Mechanism**

- **Entry point.** `parser::parse` creates the callback SAX handler when a callback is set (`parser.hpp:97-99`). It then runs `sax_parse_internal`, which writes into `result`.
- **Bookkeeping.**
  - `ref_stack` holds a pointer to each container under construction. The pointer is `nullptr` if that container was rejected (`json_sax.hpp:595`).
  - `keep_stack` records the callback's verdict per container (`:586-587`).
  - `key_keep_stack` and `key_stack` record the verdict per object key (`:625-627`).
- **Scalars and the start of containers (`handle_value`, `:1006`).**
  - If the enclosing container was rejected (`keep_stack.back()` is false), the value is dropped without calling the callback (`:1012-1015`).
  - Otherwise the callback is called with `parse_event_t::value`. `start_object` and `start_array` pass `skip_callback=true` so the callback doesn't see an empty container (`:1025`, `:594`).
  - If the callback returns false, nothing is stored and `{false, nullptr}` is returned (`:1028-1046`).
  - If it returns true, the value becomes the root when `ref_stack` is empty (`:1048-1052`). Otherwise it is appended to the parent array (`:1065-1068`) or assigned to the object slot (`:1072-1082`, `:1096`).
- **Object keys (`key()`, `:622-647`).**
  - The `key` event is sent to the callback.
  - If it is kept, a `discarded` placeholder is inserted at that key.
  - If the key already exists, its old value is stashed in `duplicate_key_stash` so it can be restored if the new value is rejected.
- **Rejected object members.** `handle_value` calls `remove_discarded_value` to erase the placeholder (`:1032-1044`). That function (`:964-985`) pops the last array element if it is discarded, or erases the key from an object. It restores the stashed duplicate value instead of erasing, if there is one.
- **Containers (`end_object` `:650-700`, `end_array` `:745-810`).**
  - The `object_end` or `array_end` event is sent to the callback with the finished container.
  - If it is rejected, the container is overwritten with `discarded` (`:661`, `:777`), unless a duplicate-key stash entry is restored instead.
  - After popping the stacks, the discarded entry is removed from the parent. For an array this is `pop_back` (`:798-800`). For an object it is `remove_discarded_value` (`:696-699`, `:802-807`).
  - Children of a rejected container are never stored, because `keep_stack.back()` is false.
- **Final result (`parser.hpp:121-133`).**
  - On a parse error, `result = value_t::discarded`.
  - If the root itself was rejected, so `result` is still `discarded`, it is set to `nullptr` (the comment there says "set top-level value to null").

**Uncertainty:** I did not read the `end_array` body (`:745-810`) in full, only its grep hits and the lines quoted above, so its details come from those lines. I did not look at `basic_json::parse` or the tests. The `git rev-parse` check failed because `/dev/null` was not writable in the sandbox, so I did not confirm that the checkout is at f422b753cc. I assumed the checkout is at the pinned commit as you stated.