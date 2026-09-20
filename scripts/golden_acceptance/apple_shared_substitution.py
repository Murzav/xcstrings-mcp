"""Xcode-proven device text with substitutions scoped to the localization root."""

from .apple_native import PREFIX, native_submit, ordered, unit_at
import copy
from .apple_xliff import import_success, unit_value, xml_units


def apple_shared_substitution(h):
    case = "positive/device-root-substitution/"
    catalog = h.copy(PREFIX + case + "source.xcstrings", "apple-root-sub/native.xcstrings")
    h.prepare(catalog)
    paths = [[{"device": "iphone"}], [{"device": "other"}],
             [{"substitution": "COUNT"}, {"plural": "one"}],
             [{"substitution": "COUNT"}, {"plural": "other"}]]
    values = ["DE %#@COUNT@ phone CHANGED", "DE %lld other CHANGED",
              "DE %arg item CHANGED", "DE %arg items CHANGED"]
    result = h.on("get_key", catalog, "device/root-substitution typed paths", key="k")
    translated = next(item for item in result["translations"] if item["locale"] == "de")
    h.equal(translated["diagnostics"], [], "Xcode-valid shared substitution has no diagnostics")
    h.equal([leaf["path"] for leaf in translated["leaves"]], paths, "independent device and root substitution paths")
    h.equal([leaf["complete"] for leaf in translated["leaves"]], [True] * 4, "German shape complete")
    translations = [{"key": "k", "locale": "de", "path": path, "value": value}
                    for path, value in zip(paths, values)]
    native_submit(h, catalog, translations, "device references root-scoped substitution")
    apple_expected = h.read(h.fixture(PREFIX + case + "expected-after-import.xcstrings"))
    draft_expected = copy.deepcopy(apple_expected)
    for path in paths:
        unit_at(draft_expected["strings"]["k"]["localizations"]["de"], path)["state"] = "needs_review"
    h.equal(h.read(catalog), draft_expected, "native text matches Xcode with explicit draft states")

    catalog = h.copy(PREFIX + case + "source.xcstrings", "apple-root-sub/Localizable.xcstrings")
    h.prepare(catalog)
    before = catalog.read_bytes()
    output = catalog.with_suffix(".xliff")
    exported = h.on("export_xliff", catalog, "root-scoped device substitution export", locale="de",
                    output_path=str(output), untranslated_only=False)
    h.equal(exported["exported_count"], 4, "two device and two independent substitution units")
    oracle = h.fixture(PREFIX + case + "exported.xliff")
    h.equal({unit.attrib["id"]: unit_value(unit) for unit in xml_units(output)},
            {unit.attrib["id"]: unit_value(unit) for unit in xml_units(oracle)},
            "root metadata resolves positional macros exactly as Xcode")
    h.equal(catalog.read_bytes(), before, "root-scoped export conserves catalog bytes")

    expected_order = h.read(catalog)
    expected_order["strings"]["k"]["localizations"]["de"]["variations"]["device"]["iphone"]["stringUnit"]["value"] = values[0]
    expected_order["strings"]["k"]["localizations"]["de"]["variations"]["device"]["other"]["stringUnit"]["value"] = values[1]
    plurals = expected_order["strings"]["k"]["localizations"]["de"]["substitutions"]["COUNT"]["variations"]["plural"]
    plurals["one"]["stringUnit"]["value"], plurals["other"]["stringUnit"]["value"] = values[2:]
    xml = h.fixture(PREFIX + case + "import-edited.xliff")
    import_success(h, catalog, xml, apple_expected, 4, "root-scoped device substitution changed XML")
    h.equal(ordered(h.read(catalog)), ordered(expected_order), "XML preserves original order and independent metadata")
    reexport = h.fixture(PREFIX + case + "reexported.xliff")
    before = catalog.read_bytes()
    import_success(h, catalog, reexport, apple_expected, 4, "actual Xcode root-scoped reexport")
    h.equal(catalog.read_bytes(), before, "actual Xcode root-scoped reexport is idempotent")
