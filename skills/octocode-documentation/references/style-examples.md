# Example values

Load when a doc needs a sample domain, address, person, phone number, or project name.

Never use real or personally identifiable data in an example.

- Keep people generic: singular "they" unless gender is the point, and no example that ties a job, a skill, or a behavior to an ethnicity, a gender, or an age.
- `Alice` and `Bob` belong to cryptographic and protocol specifications; everywhere else, take a name from the following list.

| Kind | Use |
|---|---|
| Domains | `example.com`, `example.org`, `example.net`; documentation domains `altostrat.com`, `examplepetstore.com`, `example-pet-store.com`, `cymbalgroup.com`, `myownpersonaldomain.com` |
| Email | an example domain plus a first name: `dana@example.com`; generic `support@example.net` is fine |
| Person names | Alex, Amal, Ariel, Bola, Charlie, Cruz, Dana, Dani, Hao, Ira, Izumi, Jie, Kai, Kalani, Kim, Kiran, Lee, Lucian, Luka, Mahan, Noam, Nur, Quinn, Raha, Rosario, Sasha, Tal, Taylor, Tristan, Yuri — add an initial for a surname (`Quinn N.`) |
| Companies | "Example Organization", "Enterprise Example Organization" |
| Phone | `800-555-0100` through `800-555-0199` (`references/style-numbers.md` for format) |
| IPv4 | `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24` |
| IPv6 | `2001:db8::/32` |
| Street address | `1800 Amphibious Blvd., Mountain View, CA 94045`; `8 Rue du Nom Fictif 341, Paris`; `Avenida da Pastelaria 1903, Lisbon` |
| Service account ID | `123456789012345678901` |
| Project names | descriptive, with numbering when needed: `staging`, `frontend-development`, `production-1` |
| Internationalized domains | one of the IDN test TLDs |

- Don't put person names, product names, or invented names inside email addresses.
- Don't use the Alice and Bob characters unless you're documenting a specification that uses them; if you do, stay within that cast.
- Vary names, genders, ages, and locations across examples, avoid US-centric defaults, and check that a chosen name doesn't carry a gender connotation that conflicts with the example. Don't assign roles along gender or ethnic lines.
- Avoid `foo`, `bar`, and `baz`; a meaningful placeholder name teaches something. <!-- style-lint: ignore-line metasyntactic-name -->

Upstream: [Example domains and names](https://developers.google.com/style/examples). Verify a disputed or missing rule against the live page → `references/style-sources.md`.

Next: placeholder syntax → `references/style-cli.md`; inclusive examples → `references/style-global.md`.
