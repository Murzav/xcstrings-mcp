"""Apple XML oracle fixtures and atomic import scenarios over MCP."""

import xml.etree.ElementTree as ET

from .apple_native import PREFIX, leaves, ordered

NS = "{urn:oasis:names:tc:xliff:document:1.2}"


def xml_units(path):
    return ET.parse(path).findall(".//" + NS + "trans-unit")


def unit_value(unit):
    target = unit.find(NS + "target")
    return {"source": "".join(unit.find(NS + "source").itertext()),
            "target": None if target is None else "".join(target.itertext()),
            "target_attributes": None if target is None else target.attrib,
            "notes": [{"attributes": note.attrib, "text": "".join(note.itertext())}
                      for note in unit.findall(NS + "note") if "".join(note.itertext())]}


def filtered_xml(h, relative, name, excluded):
    tree = ET.parse(h.fixture(PREFIX + relative))
    for body in tree.findall(".//" + NS + "body"):
        for unit in list(body):
            if unit.attrib.get("id") in excluded:
                body.remove(unit)
    path = h.temporary / name
    tree.write(path, encoding="utf-8", xml_declaration=True)
    return path


def import_success(h, catalog, xml, expected, count, case, **arguments):
    before = catalog.read_bytes()
    dry = h.on("import_xliff", catalog, case + " dry", xliff_path=str(xml), dry_run=True, **arguments)
    h.equal((dry["accepted"], dry["rejected"], dry["dry_run"]), (count, [], True), case + " dry report")
    h.equal(catalog.read_bytes(), before, case + " dry conservation")
    applied = h.on("import_xliff", catalog, case, xliff_path=str(xml), **arguments)
    h.equal((applied["accepted"], applied["rejected"], applied["dry_run"]), (count, [], False), case + " report")
    h.equal(len(applied["accepted_destinations"]), count, case + " destination count")
    h.equal(h.read(catalog), expected, case + " entire catalog conservation")
    return applied


def import_rejected(h, catalog, xml, codes, case):
    before = catalog.read_bytes()
    result = h.on("import_xliff", catalog, case, xliff_path=str(xml))
    h.equal(result["accepted"], 0, case + " atomic accepted count")
    h.equal({item["code"] for item in result["rejected"]}, set(codes), case + " exact rejection codes")
    h.equal(result["accepted_destinations"], [], case + " no accepted destinations")
    h.equal(catalog.read_bytes(), before, case + " byte conservation")
    return result


def apple_exports(h):
    for case, locale in (("positive/catalog-matrix", "fr"), ("behavior/plural-fallback", "fr"),
                         ("behavior/missing-locale-uk", "uk"), ("behavior/state-qualifiers", "fr")):
        source_case = "positive/catalog-matrix-safe" if case == "positive/catalog-matrix" else case
        catalog = h.copy(PREFIX + source_case + "/source.xcstrings", "apple-export/" + case + "/Localizable.xcstrings")
        if case == "positive/catalog-matrix":
            original_catalog = h.read(h.fixture(PREFIX + case + "/source.xcstrings"))
            del original_catalog["strings"]["ambiguous|==|plural.one"]
            h.equal(h.read(catalog), original_catalog, "safe26key derivative removes exactly one Apple-unsafe literal")
        original = catalog.read_bytes()
        output = catalog.with_suffix(".xliff")
        report = h.on("export_xliff", catalog, case + " export", locale=locale,
                      output_path=str(output), untranslated_only=False)
        expected = h.read(h.fixture(PREFIX + case + "/expected-units.json"))[0]
        xml = ET.parse(output)
        file = xml.find(NS + "file")
        h.equal({k: file.attrib[k] for k in expected["file"]}, expected["file"], case + " file scope")
        actual = {unit.attrib["id"]: unit_value(unit) for unit in xml_units(output)}
        h.equal(len(actual), len(xml_units(output)), case + " no duplicate export IDs")
        oracle = {}
        for unit in expected["units"]:
            if case == "positive/catalog-matrix" and unit["id"] == "ambiguous|==|plural.one":
                continue
            oracle[unit["id"]] = {key: value for key, value in unit.items() if key != "id"}
            oracle[unit["id"]]["notes"] = [note for note in unit["notes"] if note["text"]]
            h.equal(actual.get(unit["id"]), oracle[unit["id"]], case + " exact Apple unit " + unit["id"])
        extras = set(actual) - set(oracle)
        # Pinned CLDR includes French many; Xcode's minimal categories omit it.
        expected_extra = {key[:-5] + "many" for key in oracle if locale == "fr" and key.endswith("plural.other")}
        h.equal(extras, expected_extra, case + " complete required category set")
        for key in extras:
            other = oracle[key[:-4] + "other"]
            h.equal(actual[key], {**other, "target": None, "target_attributes": None},
                    case + " required many source fallback")
        h.equal(report["exported_count"], len(actual), case + " exported leaf count")
        h.equal(catalog.read_bytes(), original, case + " export does not mutate native file")


def apple_states(h):
    cases = [
        ("behavior/state-import", {"qualifier", "unknown"}, {
            "empty": ("", "translated"), "final": ("changed final", "translated"),
            "needs-review-l10n": ("changed needs-review-l10n", "needs_review"),
            "needs-review-translation": ("changed needs-review-translation", "needs_review"),
            "new": ("changed new", "new"), "no-state": ("changed no-state", "translated"),
            "signed-off": ("changed signed-off", "translated"), "translated": ("changed translated", "translated"),
        }),
        ("behavior/state-qualifiers", {"x-apple-machine-translated", "x-machine-translated"}, {
            "exact-match": ("changed exact-match", "translated"), "fuzzy-match": ("changed fuzzy-match", "translated"),
            "leveraged-mt": ("changed leveraged-mt", "machine_translated"),
            "machine_translated": ("Machine fr", "machine_translated"),
        }),
    ]
    for case, unsupported, updates in cases:
        catalog = h.copy(PREFIX + case + "/source.xcstrings", "apple-states/" + case + ".xcstrings")
        h.prepare(catalog)
        xml = h.fixture(PREFIX + case + "/import-edited.xliff")
        rejected = import_rejected(h, catalog, xml, {"unsupported_state"}, case + " unknown states reject whole batch")
        h.equal({item["unit_id"] for item in rejected["rejected"]}, unsupported, case + " exact unsupported units")
        expected = h.read(catalog)
        for key, (value, state) in updates.items():
            expected["strings"][key]["localizations"]["fr"]["stringUnit"] = {"state": state, "value": value}
        filtered = filtered_xml(h, case + "/import-edited.xliff", case.replace("/", "-") + ".xliff", unsupported)
        result = import_success(h, catalog, filtered, expected, len(updates), case + " draft/blank/qualifier states")
        h.equal(result["missing_targets"], 1 if case.endswith("state-import") else 0, case + " missing target distinction")


def apple_scopes(h):
    case = "positive/multiple-catalogs"
    xml = h.fixture(PREFIX + case + "/exported.xliff")
    for source, original, other in (("source.xcstrings", "Localizable.xcstrings", "Custom.xcstrings"),
                                    ("source-custom.xcstrings", "Custom.xcstrings", "Localizable.xcstrings")):
        catalog = h.copy(PREFIX + case + "/" + source, "apple-scopes/" + source)
        h.prepare(catalog)
        before = catalog.read_bytes()
        h.on("import_xliff", catalog, "multiple originals require selection", "multiple file originals", xliff_path=str(xml))
        h.equal(catalog.read_bytes(), before, "ambiguous scope preserves bytes")
        expected = h.read(catalog)
        expected["strings"]["shared"]["localizations"]["fr"] = {
            "stringUnit": {"state": "translated", "value": "CUSTOM cible" if original == "Custom.xcstrings" else "LOCAL cible"}}
        result = import_success(h, catalog, xml, expected, 1, "exact original " + original, original=original)
        h.equal(result["skipped_scopes"], [other], "reported skipped scope")
        h.equal([(d["original"], d["key"], d["locale"], d["path"]) for d in result["accepted_destinations"]],
                [(original, "shared", "fr", [])], "exact scoped destination identity")
        before = catalog.read_bytes()
        h.on("import_xliff", catalog, "scope is exact, never basename guessed", "was not found",
             xliff_path=str(xml), original="nested/" + original)
        h.equal(catalog.read_bytes(), before, "wrong scope preserves bytes")


def apple_import_shapes(h):
    case = "behavior/missing-locale-uk"
    catalog = h.copy(PREFIX + case + "/source.xcstrings", "apple-import/missing.xcstrings")
    h.prepare(catalog)
    before = catalog.read_bytes()
    result = import_success(h, catalog, h.fixture(PREFIX + case + "/exported.xliff"), h.read(catalog), 0, "all missing targets")
    h.equal(result["missing_targets"], 4, "all four absent Ukrainian targets")
    h.equal(catalog.read_bytes(), before, "missing target import does not even reformat")
    case = "negative/new-target-substitution-loss"
    catalog = h.copy(PREFIX + case + "/destination-before-import.xcstrings", "apple-import/new-substitution.xcstrings")
    h.prepare(catalog)
    expected = h.read(h.fixture(PREFIX + case + "/source.xcstrings"))
    import_success(h, catalog, h.fixture(PREFIX + case + "/exported.xliff"), expected, 3, "construct target-only substitution without Apple data loss")
    case = "behavior/plural-fallback"
    catalog = h.copy(PREFIX + case + "/source.xcstrings", "apple-import/fallback.xcstrings")
    h.prepare(catalog)
    result = import_success(h, catalog, h.fixture(PREFIX + case + "/exported.xliff"), h.read(catalog), 2,
                            "only-other preserved and varied source simple target accepted")
    h.equal(result["missing_targets"], 1, "missing synthesized French one remains absent")
    case = "behavior/inline-import"
    catalog = h.copy(PREFIX + case + "/source.xcstrings", "apple-import/inline.xcstrings")
    h.prepare(catalog)
    xml = h.fixture(PREFIX + case + "/import-edited.xliff")
    before = catalog.read_bytes()
    h.on("import_xliff", catalog, "opaque inline x cannot silently lose its meaning", "inline", xliff_path=str(xml))
    h.equal(catalog.read_bytes(), before, "opaque inline parse failure byte conservation")
    xml = filtered_xml(h, case + "/import-edited.xliff", "safe-inline.xliff", {"x"})
    rejected = import_rejected(h, catalog, xml, {"format_mismatch"}, "inline ph cannot introduce an absent argument")
    h.equal([item["unit_id"] for item in rejected["rejected"]], ["ph"], "inline exact invalid argument unit")
    expected = h.read(catalog)
    expected["strings"]["ph"]["localizations"]["en"]["stringUnit"]["value"] = "source %@ ph"
    h.write(catalog, expected)
    h.checkpoint(catalog, "review")
    h.capture_sources(catalog)
    # A new translation round must use the updated catalog source as well as its token.
    refreshed = ET.parse(xml)
    for unit in refreshed.findall(".//" + NS + "trans-unit"):
        if unit.attrib["id"] == "ph":
            unit.find(NS + "source").text = "source %@ ph"
    refreshed.write(xml, encoding="utf-8", xml_declaration=True)
    expected = h.read(catalog)
    for key, value in {"g": "before middle after", "ph": "before %@ after"}.items():
        expected["strings"][key]["localizations"]["fr"]["stringUnit"].update(value=value, state="translated")
    import_success(h, catalog, xml, expected, 2, "standard inline content preserves exact text and arguments")


def apple_matrix_import(h):
    case = "positive/catalog-matrix"
    catalog = h.copy(PREFIX + case + "/source.xcstrings", "apple-matrix-import/Localizable.xcstrings")
    h.prepare(catalog)
    expected = h.read(catalog)
    expected["strings"]["multi"]["localizations"]["fr"]["stringUnit"]["value"] = "Yards %#@YARDS@ oiseaux %#@BIRDS@"
    xml = h.fixture(PREFIX + case + "/exported.xliff")
    result = import_success(h, catalog, xml, expected, 52, "entire Apple matrix import")
    h.equal(result["missing_targets"], 2, "source-only plural has two absent targets")
    expected_paths = {(key, repr(path)) for key, entry in expected["strings"].items()
                      for path, _ in leaves(entry.get("localizations", {}).get("fr", {}))}
    h.equal({(d["key"], repr(d["path"])) for d in result["accepted_destinations"]}, expected_paths,
            "all matrix identities including dotted substitution names and delimiter keys")
    h.equal(ordered(h.read(catalog)), ordered(expected), "matrix known/unknown metadata and member order retained")
    before = catalog.read_bytes()
    import_success(h, catalog, xml, expected, 52, "repeated matrix import")
    h.equal(catalog.read_bytes(), before, "matrix import byte idempotency")


def apple_unsafe_exports(h):
    for case in ("positive/catalog-matrix", "negative/id-collision", "negative/delimiter-substitution-loss",
                 "negative/literal-variation-suffix"):
        catalog = h.copy(PREFIX + case + "/source.xcstrings", "apple-unsafe/" + case + ".xcstrings")
        before = catalog.read_bytes()
        output = catalog.with_suffix(".xliff")
        output.write_bytes(b"previous output must survive\n")
        h.on("export_xliff", catalog, case + " safe export rejection", "XLIFF", locale="fr",
             output_path=str(output), untranslated_only=False)
        h.equal(output.read_bytes(), b"previous output must survive\n", case + " previous export conservation")
        h.equal(catalog.read_bytes(), before, case + " native source conservation")
    catalog = h.copy(PREFIX + "negative/delimiter-substitution-loss/source.xcstrings", "apple-unsafe/delimiter-import.xcstrings")
    h.capture_sources(catalog)
    import_rejected(h, catalog, h.fixture(PREFIX + "negative/delimiter-substitution-loss/exported.xliff"),
                    {"unsupported_destination"}, "unsafe Apple delimiter import is atomic")


def write_units(path, units):
    root = ET.Element(NS + "xliff", {"version": "1.2"})
    section = ET.SubElement(root, NS + "file", {"original": "Localizable.xcstrings", "source-language": "en", "target-language": "fr"})
    body = ET.SubElement(section, NS + "body")
    for identity, source, target in units:
        unit = ET.SubElement(body, NS + "trans-unit", {"id": identity})
        ET.SubElement(unit, NS + "source").text = source
        ET.SubElement(unit, NS + "target", {"state": "translated"}).text = target
    ET.ElementTree(root).write(path, encoding="utf-8", xml_declaration=True)


def apple_atomic_errors(h):
    catalog = h.copy(PREFIX + "positive/catalog-matrix/source.xcstrings", "apple-atomic/Localizable.xcstrings")
    h.prepare(catalog)
    xml = catalog.with_suffix(".xliff")
    valid = ("simple", "EN simple", "FR modifié")
    cases = [
        ("unknown_key", [("absent-key", "absent", "absent")]),
        ("unsupported_destination", [("simple|==|future.axis", "EN simple", "cible")]),
        ("format_mismatch", [("plural|==|plural.one", "EN plural %lld one", "missing argument")]),
        ("overlapping_destinations", [("plural", "EN plural %lld many", "%lld flat"),
                                      ("plural|==|plural.other", "EN plural %lld many", "%lld varied")]),
    ]
    for code, invalid in cases:
        write_units(xml, [valid, *invalid])
        import_rejected(h, catalog, xml, {code}, "atomic selected batch " + code)
    write_units(xml, [valid, valid])
    before = catalog.read_bytes()
    h.on("import_xliff", catalog, "duplicate XML IDs within one file", "duplicate XLIFF unit id", xliff_path=str(xml))
    h.equal(catalog.read_bytes(), before, "duplicate XML ID parse failure preserves bytes")
    write_units(xml, [valid])
    document = ET.parse(xml)
    document.getroot().append(ET.fromstring(ET.tostring(document.getroot()[0])))
    document.write(xml, encoding="utf-8", xml_declaration=True)
    import_rejected(h, catalog, xml, {"duplicate_destination"}, "duplicate logical destinations across same-original sections")
    protected = h.read(catalog)
    protected["strings"]["simple"]["shouldTranslate"] = False
    h.write(catalog, protected)
    write_units(xml, [valid])
    import_rejected(h, catalog, xml, {"not_translatable"}, "protected key import")
    collision = h.copy(PREFIX + "negative/id-collision/source.xcstrings", "apple-atomic/collision.xcstrings")
    h.capture_sources(collision)
    write_units(xml, [("ambiguous|==|plural.one", "source", "%lld cible")])
    import_rejected(h, collision, xml, {"ambiguous_destination"}, "literal ID never wins over competing varied destination")


def apple_partial_substitution(h):
    for leaf_only in (True, False):
        case = "leaf-only" if leaf_only else "parent-only"
        catalog = h.copy(PREFIX + "positive/catalog-matrix/source.xcstrings", "apple-partial/" + case + ".xcstrings")
        initial = h.read(catalog)
        del initial["strings"]["substitution"]["localizations"]["fr"]
        h.write(catalog, initial)
        h.prepare(catalog)
        xml = catalog.with_suffix(".xliff")
        if leaf_only:
            write_units(xml, [("substitution|==|substitutions.COUNT.plural.one", "EN substitution %lld one", "FR %1$lld partiel")])
        else:
            write_units(xml, [("substitution", "EN substitution %1$#@COUNT@", "FR %1$#@COUNT@ partiel")])
        expected = h.read(catalog)
        expected["strings"]["substitution"]["localizations"]["fr"] = {
            "stringUnit": {"state": "new" if leaf_only else "translated",
                           "value": "EN substitution %#@COUNT@" if leaf_only else "FR %#@COUNT@ partiel"},
            "substitutions": {"COUNT": {"argNum": 1, "formatSpecifier": "lld", "variations": {"plural": {
                "one": {"stringUnit": {"state": "translated" if leaf_only else "new", "value": "FR %arg partiel" if leaf_only else ""}},
                "many": {"stringUnit": {"state": "new", "value": ""}},
                "other": {"stringUnit": {"state": "new", "value": ""}},
            }}}},
        }
        import_success(h, catalog, xml, expected, 1, "partial substitution creates compiler-valid skeleton " + case)
