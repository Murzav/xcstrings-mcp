# Apple Xcode 27 fixture corpus

Captured 2026-09-19 with **Xcode 27.0, build 27A266a** on macOS. This corpus targets the Apple String Catalog profile of XLIFF 1.2, not arbitrary future catalog schemas or all vendor XLIFF dialects.

Catalog inputs are deliberately constructed examples verified by the real Apple compiler. Files named `exported.xliff` and `reexported.xliff` are **byte-for-byte actual Xcode output**. Files named `import-edited.xliff` are explicitly edited copies used to probe imports, never represented as original Apple output. Expected catalog snapshots are real post-import results, including observed Apple bugs in the negative section.

## Contents and acceptance boundary

| Directory | Contract |
| --- | --- |
| `positive/catalog-matrix` | Simple and empty keys; literal delimiter keys; all seven observed devices; device.iphone → plural with simple device.other fallback; direct plurals; substitutions; dotted and Unicode substitution names; reordered multiple arguments; target-only plural/device/existing substitution; XML-special and control-character keys; new/needs_review/translated/machine_translated states; generated-comment metadata |
| `positive/multiple-catalogs` | Duplicate `shared` key in Custom.xcstrings and Localizable.xcstrings, with separate file originals and expected catalogs |
| `behavior/state-import` | Changed targets with explicit states, absent states, empty target and missing target |
| `behavior/state-qualifiers` | machine_translated ↔ translated + leveraged-mt; other qualifier behavior |
| `behavior/plural-fallback` | Compiler-valid only-other plural; target locale category synthesis; varied source/simple target |
| `behavior/missing-locale-uk` | Source en one/other, missing uk locale requiring few/many/one/other |
| `behavior/inline-import` | Actual Apple import flattening of deliberately inserted g/ph/x elements |
| `negative/id-collision` | Literal key and variation destination generate identical IDs; Xcode exporter crashes |
| `negative/delimiter-substitution-loss` | Compiler-valid delimiter-containing substitution exports, then target leaves disappear during Apple import |
| `negative/new-target-substitution-loss` | Existing target-only substitution is exported, then target is removed before import; Apple drops its leaves |
| `negative/invalid-nesting` | Compiler rejection of nested device.other, nested plural case, and invalid @ substitution references |

Positive means compiler-valid supported shape with a fully accounted-for import outcome. It does not mean every string is already translated: explicit new/needs_review states and missing target leaves are intentional test cases. The matrix's `multi` root references normalize from positional to named references without changing argNum or repeat-export text. `source_only_plural` gains new French target leaves during import. Those are the only full-JSON changes; inspect `results.json` and `expected-after-import.xcstrings`.

Negative observed output is a regression counterexample, **not desired tool behavior**. In particular, never use Xcode exit 0 alone as a losslessness assertion.

## Exact fallback observations

All strings below are literal observed fixture values.

- Missing uk target locale: `fallback|==|plural.one` exports source `one source %lld`; `few`, `many` and `other` export `other source %lld`. **All four omit `<target>` entirely.** Untouched import leaves the wholly absent uk catalog locale absent.
- Existing French localization with only `other`: `only_other|==|plural.one` exports source `EN %lld` and omits target; `other` exports source `EN %lld`, target `FR %lld`, state translated. Import adds French `one` with value `EN %lld`, state new, while preserving translated other.
- Varied source with simple French target: ID remains `varied_source_simple_target`, source is `EN %lld other`, target is `FR %lld flat`. Shape difference is legal.
- Existing target-only substitution: source for its substitution leaves is the complete unit ID; it is not the parent's unvaried source text. Exact IDs/text are in catalog-matrix expected-units.json.

These are finite observed rules. Do not generalize compiler acceptance to complete translation coverage, or require identical source/target shapes.

## States and placeholders

- Native `needs_review` exports `needs-review-l10n`; native `machine_translated` exports `translated` with `state-qualifier="leveraged-mt"`.
- Xcode ignores imports with unfinished `new`/`needs-review-l10n`/`needs-review-translation` states, leaving existing values unchanged. This policy is visible in import logs and must not be confused with an inability to represent those native states.
- Missing target is no-op. Explicit empty translated target clears the text while keeping translated state. Xcode can emit a misleading missing-translation warning even when it writes that empty value.
- A positional substitution reference uses `%N$#@NAME@` in XLIFF. Its `%arg` leaf uses `%N$<formatSpecifier>`. Reordering the target's named substitutions preserves argument numbers.
- Native metadata observed here includes `isCommentAutoGenerated`, `commentGenerationVersion`, and `generatesSymbol`; auto-generated comments export with `note from="auto-generated"`.
- TAB/LF/CR in IDs are numeric references `&#9;`, `&#10;`, `&#13;`, preserving their decoded values. Generated output uses the default XLIFF namespace. A preliminary prefixed-ns0 import was silently skipped by Xcode; final edited fixtures use the default namespace and assert resulting values.

## Provenance and reproduction

`manifest.json` records the SHA-256 of every copied source artifact, its original path relative to the research root, and its provenance class. Copying performed no minimization, namespace rewrite, unit reordering, or JSON reserialization. `expected-units.json` files are the only derived semantic extraction: Python ElementTree reads actual XML into explicit file attributes, unit IDs, source/target values, states and notes, without using production Rust code.

Original research root: `/tmp/xcstrings-pr26-xcode-contract-9m3wxp0f`. Original snapshots remain unchanged. `provenance/` retains the actual project descriptions and experiment scripts as an audit trail. Those scripts refer to the original research layout and are not advertised as standalone repository automation. Each action's log records its full exact argument vector.

For an isolated replay, place a chosen `source.xcstrings` as `Localizable.xcstrings` alongside an `Oracle.xcodeproj/project.pbxproj` copied from the single-catalog project description. For multiple catalogs, use its separate project description and also copy source-custom.xcstrings as Custom.xcstrings. Ensure the requested export locale appears in the project's knownRegions (add uk for the uk fallback case). The original project has developmentRegion en.

```sh
xcodebuild -version
xcrun xcstringstool compile ROOT/Localizable.xcstrings --output-directory ROOT/compiled
xcodebuild -exportLocalizations -project ROOT/Oracle.xcodeproj -localizationPath ROOT/export -exportLanguage fr
xcodebuild -importLocalizations -project ROOT/Oracle.xcodeproj -localizationPath ROOT/export/fr.xcloc/Localized\ Contents/fr.xliff
xcrun xcstringstool compile ROOT/Localizable.xcstrings --output-directory ROOT/compiled-after
xcodebuild -exportLocalizations -project ROOT/Oracle.xcodeproj -localizationPath ROOT/reexport -exportLanguage fr
```

For an edited-import scenario, substitute `import-edited.xliff` only after obtaining the baseline export. For new-target-substitution-loss, replace the working catalog with `destination-before-import.xcstrings` before import. Compare parsed catalog output against the scenario's expected snapshot and the generated file/unit semantics against expected-units.json; compiler/import success alone is insufficient. Remove only replay-owned compiled/export/project directories afterward.

## Additional bounded probes, 2026-09-20

`behavior/partial-device` proves that an existing target device shape governs export:
source has iphone, ipad, and other, while target has only iphone; Xcode exports exactly
one iphone unit. Missing source device branches are not synthesized into that existing
target shape. This probe asserts export only, without claiming an import roundtrip.

`negative/substitution-without-parent` proves that a localization containing only
substitutions, without its parent stringUnit, fails compilation with a missing required
stringUnit error. A partial leaf import into an absent target must retain or initialize
the source parent macro as unfinished, or reject when no unambiguous parent exists.
The library initializes a source-derived parent as New and translates only submitted
leaves; its user-selected draft policy intentionally differs from Xcode import.

### Changed-target probes refine the interoperability boundary

Untouched roundtrips alone did not expose Apple's silent skips. The added probes
change target text and compare complete native catalogs after real Xcode import:

- `negative/literal-variation-suffix`: literal keys ending in recognized device,
  plural, or substitution paths silently ignore updates even with Xcode's own
  XML and no competing catalog key. Ordinary delimiter text (`plain|==|word`,
  `plain|==|unknown.x`) updates correctly. Catalog-aware native/tool import can
  safely address exact keys, but interoperable export rejects these suffixes.
- `positive/catalog-matrix` remains the original unchanged evidence, including the
  newly discovered unsafe literal. `positive/catalog-matrix-safe/source.xcstrings`
  is an explicitly derived positive input removing only that exact key; its origin
  SHA and extraction method are recorded. It is not labelled an Xcode-generated
  catalog or XML document.
- `negative/new-target-case-source-identity`: a new target-only substitution case
  whose source is its new unit ID is skipped. In
  `behavior/new-target-case-existing-context`, using the existing other case's
  source identity applies the new many translation and preserves catalog metadata.
- `negative/overlapping-shapes`: stringUnit plus variations, or plural plus device
  axes, compile but Xcode exports only part of the shape and deletes native source
  and target data on import. Preserve raw data but reject semantic mutation/export
  of these overlapping shapes.
- Literal CR must remain a numeric reference when editing XML: ElementTree writes
  raw CR in element text, which XML readers normalize to LF. Probe scripts restore
  CR references after writing and then Xcode preserves the CR keys and values.
- The original unedited catalog-matrix import already upgrades catalog version
  1.0 to 1.1; this is recorded in its source and expected-after-import snapshots.
  A native tool preserving the input version is intentionally not the same as
  Xcode's serialization normalization; do not ignore unrelated metadata changes.

Added edited XML is explicitly classified as `edited_xcode_export`, not raw Xcode
output. Logs, before/after catalogs, and reexported Xcode XML remain separate.

### Device leaves with root-scoped substitutions

`positive/device-root-substitution` is an actual changed-target German export/import/compile/reexport. Its localization owns `variations.device` and sibling root `substitutions`. Device text references that root metadata. Unit IDs are `k|==|device.iphone`, `k|==|device.other`, and root `k|==|substitutions.COUNT.plural.{one,other}`. There is no nested device-to-substitution path. The separately probed nested metadata placement fails compilation with undefined substitution COUNT. Parent macros are therefore resolved across ordinary/device leaves against root substitution metadata.

Xcode reexports some newly added plural cases without source elements. `behavior/new-target-case-existing-context/reexported.xliff` records this deviation from the XLIFF 1.2 schema. The parser profile permits a present target on a recognized plural ID without source only provisionally; catalog-aware import requires an existing resolved target plural leaf and validates against catalog source/metadata. Simple strings and new unresolvable plural leaves retain strict rejection.
