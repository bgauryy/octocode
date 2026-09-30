# bench baseline (2026-09-29T17:38:18.698Z)

## pr suite

| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |
|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|
| ts-51387 | gh-typical | 6854k | 1714k | 3 | 6.2k | 1 | 0.005 | 414 | ✓ |
| ts-51387 | gh-lean | 3.8k | 942 | 2 | 5.4k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode | 86k | 22k | 11 | 14k | 1 | 1 | 2 | ✓ |
| ts-51387 | octocode+clasify | 172k | 43k | 17 | 100k | 1 | 0.143 | 14 | ✓ |
| rust-157558 | gh-typical | 1417k | 354k | 3 | 5.7k | 1 | 0.007 | 425 | ✓ |
| rust-157558 | gh-lean | 1.2k | 311 | 2 | 2.9k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode | 41k | 10k | 7 | 8.0k | 1 | 1 | 3 | ✓ |
| rust-157558 | octocode+clasify | 43k | 11k | 8 | 21k | 1 | 1 | 3 | ✓ |
| ts-61986 | gh-typical | 7269k | 1817k | 2 | 4.2k | 1 | 0.333 | 12 | ✓ |
| ts-61986 | gh-lean | 40k | 9.9k | 3 | 2.4k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode | 101k | 25k | 9 | 12k | 1 | 0.667 | 6 | ✓ |
| ts-61986 | octocode+clasify | 49k | 12k | 9 | 26k | 1 | 0.571 | 7 | ✓ |
| tokio-8156 | gh-typical | 49k | 12k | 2 | 2.8k | 1 | 0.081 | 37 | ✓ |
| tokio-8156 | gh-lean | 725 | 181 | 2 | 1.1k | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode | 19k | 4.8k | 3 | 2.9k | 1 | 1 | 3 | ✓ |
| tokio-8156 | octocode+clasify | 40k | 9.9k | 5 | 21k | 1 | 0.273 | 11 | ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| gh-typical | 4/4 | 15590k | 3898k | 10 | 19k | 1 | 888 |
| gh-lean | 4/4 | 45k | 11k | 9 | 12k | 1 | 14 |
| octocode | 4/4 | 247k | 62k | 30 | 37k | 1 | 14 |
| octocode+clasify | 4/4 | 305k | 76k | 39 | 167k | 1 | 35 |

## local suite

| task | arm | chars | ≈tokens | calls | ms | recall | read P | files read | correct |
|---|---|--:|--:|--:|--:|--:|--:|--:|:-:|
| ts-symbol | rg/sed | 1.5k | 365 | 2 | 86 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| ts-symbol | octocode | 5.0k | 1.3k | 1 | 317 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| ts-symbol | octocode+clasify | 11k | 2.7k | 3 | 866 | 0 (usages R 1 / P 1) | 0 | 1 | ✗ |
| ts-how | rg/sed | 6.2k | 1.6k | 2 | 52 | 1 | 0.667 | 3 | ✓ |
| ts-how | octocode | 8.3k | 2.1k | 2 | 128 | 1 | 0.667 | 3 | ✓ |
| ts-how | octocode+clasify | 7.1k | 1.8k | 2 | 547 | 0.5 | 0.667 | 3 | ✗ |
| ts-how | octocode+clasify(files) | 7.6k | 1.9k | 3 | 720 | 1 | 0.5 | 2 | ✓ |
| ts-unknown | rg/sed | 5.9k | 1.5k | 2 | 47 | 1 | 0.333 | 3 | ✓ |
| ts-unknown | octocode | 8.1k | 2.0k | 2 | 228 | 1 | 0.333 | 3 | ✓ |
| ts-unknown | octocode+clasify | 3.3k | 820 | 2 | 546 | 1 | 1 | 1 | ✓ |
| ts-unknown | octocode+clasify(files) | 6.5k | 1.6k | 3 | 1.4k | 1 | 1 | 1 | ✓ |
| rust-symbol | rg/sed | 1.7k | 414 | 2 | 53 | 1 (usages R 1 / P 1) | 0.167 | 6 | ✓ |
| rust-symbol | octocode | 5.3k | 1.3k | 1 | 275 | 1 (usages R 1 / P 1) | 0.167 | 6 | ✓ |
| rust-symbol | octocode+clasify | 10k | 2.6k | 3 | 855 | 0 (usages R 1 / P 1) | 1 | 1 | ✗ |
| rust-how | rg/sed | 8.4k | 2.1k | 2 | 32 | 1 | 0.667 | 3 | ✓ |
| rust-how | octocode | 13k | 3.3k | 2 | 153 | 1 | 0.667 | 3 | ✓ |
| rust-how | octocode+clasify | 5.5k | 1.4k | 2 | 516 | 0.5 | 1 | 1 | ✗ |
| rust-how | octocode+clasify(files) | 14k | 3.5k | 3 | 815 | 1 | 1 | 2 | ✓ |
| rust-unknown | rg/sed | 9.6k | 2.4k | 2 | 32 | 1 | 0.333 | 3 | ✓ |
| rust-unknown | octocode | 12k | 2.9k | 2 | 104 | 1 | 0.333 | 3 | ✓ |
| rust-unknown | octocode+clasify | 6.1k | 1.5k | 2 | 482 | 0 | 1 | 1 | ✗ |
| rust-unknown | octocode+clasify(files) | 6.9k | 1.7k | 3 | 662 | 1 | 1 | 1 | ✓ |
| go-symbol | rg/sed | 208 | 52 | 2 | 106 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-symbol | octocode | 1.0k | 260 | 1 | 204 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-symbol | octocode+clasify | 6.6k | 1.6k | 3 | 789 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| go-how | rg/sed | 10k | 2.6k | 2 | 29 | 1 | 0.667 | 3 | ✓ |
| go-how | octocode | 11k | 2.8k | 2 | 159 | 0.5 | 0.667 | 3 | ✗ |
| go-how | octocode+clasify | 4.8k | 1.2k | 2 | 501 | 0.5 | 1 | 1 | ✗ |
| go-how | octocode+clasify(files) | 12k | 2.9k | 3 | 871 | 1 | 1 | 2 | ✓ |
| go-unknown | rg/sed | 11k | 2.8k | 2 | 40 | 1 | 0.333 | 3 | ✓ |
| go-unknown | octocode | 11k | 2.6k | 2 | 159 | 1 | 0.333 | 3 | ✓ |
| go-unknown | octocode+clasify | 5.4k | 1.4k | 2 | 486 | 0 | 0 | 1 | ✗ |
| go-unknown | octocode+clasify(files) | 6.5k | 1.6k | 3 | 848 | 1 | 1 | 1 | ✓ |
| py-symbol | rg/sed | 521 | 130 | 2 | 401 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-symbol | octocode | 4.2k | 1.0k | 1 | 634 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-symbol | octocode+clasify | 9.8k | 2.4k | 3 | 1.1k | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| py-how | rg/sed | 12k | 2.9k | 2 | 25 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode | 17k | 4.3k | 2 | 325 | 1 | 0.667 | 3 | ✓ |
| py-how | octocode+clasify | 9.3k | 2.3k | 2 | 580 | 0.5 | 0.5 | 2 | ✗ |
| py-how | octocode+clasify(files) | 28k | 6.9k | 4 | 2.4k | 1 | 0.667 | 3 | ✓ |
| py-unknown | rg/sed | 9.4k | 2.4k | 2 | 127 | 1 | 1 | 3 | ✓ |
| py-unknown | octocode | 12k | 2.9k | 2 | 212 | 1 | 1 | 3 | ✓ |
| py-unknown | octocode+clasify | 8.5k | 2.1k | 2 | 597 | 0.67 | 1 | 2 | ✗ |
| py-unknown | octocode+clasify(files) | 8.7k | 2.2k | 3 | 822 | 1 | 1 | 2 | ✓ |
| java-symbol | rg/sed | 1.1k | 269 | 2 | 55 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| java-symbol | octocode | 4.2k | 1.1k | 1 | 275 | 1 (usages R 1 / P 1) | 1 | 1 | ✓ |
| java-symbol | octocode+clasify | 10k | 2.6k | 3 | 731 | 0 (usages R 1 / P 1) | 0 | 1 | ✗ |
| java-how | rg/sed | 14k | 3.6k | 2 | 26 | 0.5 | 0.667 | 3 | ✗ |
| java-how | octocode | 14k | 3.5k | 2 | 157 | 0 | 0.667 | 3 | ✗ |
| java-how | octocode+clasify | 6.8k | 1.7k | 2 | 497 | 0 | 0.5 | 2 | ✗ |
| java-how | octocode+clasify(files) | 13k | 3.3k | 3 | 1.1k | 1 | 1 | 2 | ✓ |
| java-unknown | rg/sed | 13k | 3.2k | 2 | 25 | 1 | 0.333 | 3 | ✓ |
| java-unknown | octocode | 14k | 3.4k | 2 | 158 | 1 | 0.333 | 3 | ✓ |
| java-unknown | octocode+clasify | 3.9k | 974 | 2 | 448 | 0 | 1 | 1 | ✗ |
| java-unknown | octocode+clasify(files) | 9.5k | 2.4k | 3 | 736 | 1 | 1 | 1 | ✓ |

| arm | correct | chars | ≈tokens | calls | ms | mean recall | files read |
|---|--:|--:|--:|--:|--:|--:|--:|
| rg/sed | 14/15 | 105k | 26k | 30 | 1.1k | 0.97 | 40 |
| octocode | 13/15 | 139k | 35k | 25 | 3.5k | 0.9 | 40 |
| octocode+clasify | 3/15 | 108k | 27k | 35 | 9.5k | 0.38 | 20 |
| octocode+clasify(files) | 10/10 | 112k | 28k | 31 | 10k | 1 | 17 |
