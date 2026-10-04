**Yes, the bug is still present at facc6fc47e.** The checkout has none of the three fixes from PR #8546. It has no `channel_from_index` helpers and no `wraparound` tests either. The PR was merged on 2026-10-02 into `tokio-1.51.x`.

All paths below are under `tokio/src/sync/mpsc/`.

| Spot in the checkout | What the PR changes |
|---|---|
| `block.rs:181-186`, `Block::has_value`. It uses `slot_index < start_index` and `slot_index >= start_index + super::BLOCK_CAP`. The addition can overflow near `usize::MAX`. | Replaces both range checks with `if start_index(slot_index) != self.header.start_index { return false; }`. |
| `block.rs:361`, `Block::grow`. It has `Block::new(self.header.start_index + BLOCK_CAP)`. | Changes it to `Block::new(self.header.start_index.wrapping_add(BLOCK_CAP))`. |
| `list.rs:405`, `Rx::reclaim_blocks`. It has `if required_index > self.index { return; }`. | Changes it to `if required_index.wrapping_sub(self.index) as isize > 0 { return; }`. |

Some code in the checkout already wraps correctly and needs no change:
- `block.rs:298` and `block.rs:329` use `wrapping_add(BLOCK_CAP)`.
- `list.rs:300` uses `wrapping_sub`.

The PR also adds test-only code that is absent from the checkout:
- `list::channel_from_index`, which `channel` now calls with 0.
- `chan::channel_from_list`, which `channel_from_index` and `channel` both call.
- `unbounded_channel_from_index`.
- The `wraparound` and `wraparound_unbounded` tests.

**Why it matters (from the PR body):** after `tail_position` wraps, the receiver can reclaim a block that a slow sender is still reading. That is a data race or use-after-free, seen on 32-bit platforms after 2^32 messages. The PR reports the race under Miri.

I read `has_value` directly in the checkout. The other spots come from a text search of the mpsc directory, which showed the matching lines, so I did not read their full surrounding functions.