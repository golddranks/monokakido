# monokakido.rs

A Rust library for parsing and interpreting the [Monokakido](https://www.monokakido.jp/en/dictionaries/app/) dictionary format. Aiming for full test coverage and efficient implementation with minimal dependencies.

## TODO:
- Refactor code for generic "rsc" and "nrsc" support
- Audio using "rsc" (CCCAD, WISDOM3)
- Audio using "nrsc" (DAIJISEN2, NHKACCENT2, OALD10, OLDAE, OLEX, OLT, RHEJ, SMK8)
- Multiple contents (WISDOM3, OLEX)
- Document the rsc, nrsc and keystore formats
- Split main.rs into "dict exploder" and "dict cli"

## Planned to support:
- WISDOM3
- SMK8
- RHEJ
- OLT
- OLEX
- OLDAE
- OCD
- OALD10
- NHKACCENT2
- DAIJISEN2
- CCCAD
