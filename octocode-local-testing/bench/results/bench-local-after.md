# bench local-after (2026-09-29T20:38:12.881Z)

## local suite

| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |
|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|
| ts-symbol | rg/sed | 1.5k | 365 | 2 | 80 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| ts-symbol | octocode | 2.1k | 519 | 1 | 187 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| ts-symbol | octocode+clasify | 12k | 2.9k | 3 | 2.2k | 0 (usages R 1 / P 1) | 0 | 1 | ✗ |
| ts-how | rg/sed | 6.2k | 1.6k | 2 | 40 | 1 | 0.667 | 3 | ✓ |
| ts-how | octocode | 8.0k | 2.0k | 2 | 123 | 1 | 0.667 | 3 | ✓ |
| ts-how | octocode+clasify | 8.5k | 2.1k | 2 | 797 | 0.5 | 0.5 | 2 | ✗ |
| ts-how | octocode+clasify(files) | 7.5k | 1.9k | 3 | 693 | 1 | 0.5 | 2 | ✓ |
| ts-unknown | rg/sed | 5.9k | 1.5k | 2 | 43 | 1 | 0.333 | 3 | ✓ |
| ts-unknown | octocode | 7.8k | 2.0k | 2 | 218 | 1 | 0.333 | 3 | ✓ |
| ts-unknown | octocode+clasify | 3.5k | 887 | 2 | 552 | 1 | 1 | 1 | ✓ |
| ts-unknown | octocode+clasify(files) | 6.4k | 1.6k | 3 | 1.0k | 1 | 1 | 1 | ✓ |
| rust-symbol | rg/sed | 1.7k | 414 | 2 | 44 | 1 (usages R 1 / P 1) | 0.167 | 6 | ✓ |
| rust-symbol | octocode | 2.2k | 542 | 1 | 96 | 1 (usages R 1 / P 1) | 0.167 | 6 | ✓ |
| rust-symbol | octocode+clasify | 8.7k | 2.2k | 3 | 1.4k | 1 (usages R 1 / P 1) | 0.5 | 2 | ✓ |
| rust-how | rg/sed | 8.4k | 2.1k | 2 | 24 | 1 | 0.667 | 3 | ✓ |
| rust-how | octocode | 12k | 3.1k | 2 | 150 | 1 | 0.667 | 3 | ✓ |
| rust-how | octocode+clasify | 10k | 2.5k | 2 | 881 | 1 | 1 | 2 | ✓ |
| rust-how | octocode+clasify(files) | 13k | 3.4k | 3 | 782 | 1 | 1 | 2 | ✓ |
| rust-unknown | rg/sed | 9.6k | 2.4k | 2 | 30 | 1 | 0.333 | 3 | ✓ |
| rust-unknown | octocode | 11k | 2.8k | 2 | 98 | 1 | 0.333 | 3 | ✓ |
| rust-unknown | octocode+clasify | 7.2k | 1.8k | 2 | 778 | 1 | 1 | 1 | ✓ |
| rust-unknown | octocode+clasify(files) | 6.5k | 1.6k | 3 | 645 | 1 | 1 | 1 | ✓ |
| go-symbol | rg/sed | 208 | 52 | 2 | 93 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-symbol | octocode | 785 | 196 | 1 | 214 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-symbol | octocode+clasify | 5.7k | 1.4k | 3 | 971 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-how | rg/sed | 10k | 2.6k | 2 | 27 | 1 | 0.667 | 3 | ✓ |
| go-how | octocode | 11k | 2.9k | 2 | 153 | 1 | 0.667 | 3 | ✓ |
| go-how | octocode+clasify | 9.4k | 2.3k | 2 | 796 | 1 | 1 | 2 | ✓ |
| go-how | octocode+clasify(files) | 11k | 2.8k | 3 | 937 | 1 | 1 | 2 | ✓ |
| go-unknown | rg/sed | 11k | 2.8k | 2 | 27 | 1 | 0.333 | 3 | ✓ |
| go-unknown | octocode | 11k | 2.7k | 2 | 159 | 1 | 0.333 | 3 | ✓ |
| go-unknown | octocode+clasify | 8.4k | 2.1k | 2 | 1.2k | 1 | 1 | 1 | ✓ |
| go-unknown | octocode+clasify(files) | 6.3k | 1.6k | 3 | 912 | 1 | 1 | 1 | ✓ |
| py-symbol | rg/sed | 521 | 130 | 2 | 427 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-symbol | octocode | 1.2k | 296 | 1 | 442 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-symbol | octocode+clasify | 5.8k | 1.5k | 3 | 937 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-how | rg/sed | 12k | 2.9k | 2 | 28 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode | 15k | 3.8k | 2 | 327 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode+clasify | 11k | 2.8k | 2 | 896 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode+clasify(files) | 20k | 5.1k | 4 | 2.2k | 1 | 0.667 | 3 | ✓ |
| py-unknown | rg/sed | 9.4k | 2.4k | 2 | 133 | 1 | 1 | 3 | ✓ |
| py-unknown | octocode | 11k | 2.8k | 2 | 201 | 1 | 1 | 3 | ✓ |
| py-unknown | octocode+clasify | 7.9k | 2.0k | 2 | 584 | 0.67 | 1 | 2 | ✗ |
| py-unknown | octocode+clasify(files) | 8.7k | 2.2k | 3 | 961 | 1 | 1 | 2 | ✓ |
| java-symbol | rg/sed | 1.1k | 269 | 2 | 54 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| java-symbol | octocode | 1.3k | 326 | 1 | 99 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| java-symbol | octocode+clasify | 6.8k | 1.7k | 3 | 816 | 0 (usages R 1 / P 1) | 0 | 1 | ✗ |
| java-how | rg/sed | 14k | 3.6k | 2 | 23 | 0.5 | 0.667 | 3 | ✗ |
| java-how | octocode | 13k | 3.2k | 2 | 155 | 0.5 | 0.667 | 3 | ✗ |
| java-how | octocode+clasify | 12k | 2.9k | 2 | 886 | 0.5 | 0.667 | 3 | ✗ |
| java-how | octocode+clasify(files) | 13k | 3.3k | 3 | 1.1k | 1 | 1 | 2 | ✓ |
| java-unknown | rg/sed | 13k | 3.2k | 2 | 25 | 1 | 0.333 | 3 | ✓ |
| java-unknown | octocode | 13k | 3.2k | 2 | 156 | 1 | 0.333 | 3 | ✓ |
| java-unknown | octocode+clasify | 5.9k | 1.5k | 2 | 756 | 1 | 1 | 1 | ✓ |
| java-unknown | octocode+clasify(files) | 9.0k | 2.2k | 3 | 779 | 1 | 1 | 1 | ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| rg/sed | 14/15 | 105k | 26k | 30 | 1.1k | 0.97 | 40 |
| octocode | 14/15 | 121k | 30k | 25 | 2.8k | 0.97 | 40 |
| octocode+clasify | 10/15 | 123k | 31k | 35 | 14k | 0.78 | 24 |
| octocode+clasify(files) | 10/10 | 102k | 26k | 31 | 10k | 1 | 17 |
