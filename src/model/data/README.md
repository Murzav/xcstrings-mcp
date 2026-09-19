# Cardinal plural category data

`cldr_cardinal_categories.json` is derived from Unicode CLDR JSON release **48.2.1**,
`cldr-json/cldr-core/supplemental/plurals.json`, containing 224 locale rules.

Source: https://github.com/unicode-org/cldr-json/blob/48.2.1/cldr-json/cldr-core/supplemental/plurals.json

Source SHA-256: `6c0a48e9bcfc25856f90202f703c2c7f89c105d6868f7712a943a6ed2dcbe8f4`.

The transformation keeps each locale and strips `pluralRule-count-` from its
cardinal rule keys, preserving category order. It omits rule expressions because
this tool needs the category set, not a number-to-category evaluator.

These are CLDR category recommendations for complete translations, distinct from
Xcode compiler acceptance: a catalog containing only `other` may compile while
still lacking translations for other locale categories. Apple-specific differences
must be documented and tested explicitly, never inferred from an export exit code.

The source license is reproduced verbatim in `UNICODE-LICENSE.txt`. To update,
fetch a pinned stable CLDR release, apply the same key-only transformation, record
its version/hash here, and review changed category sets with regression examples.
