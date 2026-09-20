"""Legacy formats, XLIFF and fingerprint-protected semantic merge scenarios."""

import copy
import hashlib
from pathlib import Path
import xml.etree.ElementTree as ET


def legacy_scenarios(h):
    legacy = [h.copy(f"{locale}.lproj/Localizable.{extension}", f"legacy/{locale}.lproj/Localizable.{extension}")
              for locale in ("en", "es") for extension in ("strings", "stringsdict")]
    discovery = h.call("discover_files", {"directory": str(h.temporary / "legacy")}, "discover both legacy formats")
    h.equal(discovery["files"], [], "no modern files in legacy-only directory")
    h.equal(discovery["count"], 0, "zero modern count")
    h.equal(discovery["legacy_count"], 4, "four legacy fixtures")
    h.equal({item["path"] for item in discovery["legacy_files"]}, set(map(str, legacy)), "all discovered legacy paths")
    h.call("discover_files", {"directory": str(h.temporary / "absent")}, "invalid discovery directory", "not a directory")
    output = h.temporary / "migrated.xcstrings"
    params = {"directory": str(h.temporary / "legacy"), "source_language": "en", "output_path": str(output)}
    dry = h.call("import_strings", {**params, "dry_run": True}, "legacy dry run")
    h.equal((dry["total_keys"], dry["plural_keys"], dry["dry_run"]), (25, 3, True), "legacy dry summary")
    h.require(not output.exists(), "legacy dry run must not create output")
    applied = h.call("import_strings", params, "legacy migration")
    h.equal((applied["total_keys"], applied["plural_keys"], applied["warnings"]), (25, 3, []), "legacy apply summary")
    result = h.read(output)["strings"]
    h.equal(len(result), 25, "exact migrated key count")
    h.equal(result["test.escapes"]["localizations"]["es"]["stringUnit"]["value"],
            'Línea 1\nLínea 2\t"citado"\\barra invertida', "Spanish escaped text")
    h.equal(result["testUnquotedKey"]["localizations"]["en"]["stringUnit"]["value"], "Unquoted key value", "unquoted key")
    h.equal(result["items_count"]["localizations"]["es"]["variations"]["plural"], {
        "one": {"stringUnit": {"state": "translated", "value": "%lld elemento"}},
        "other": {"stringUnit": {"state": "translated", "value": "%lld elementos"}},
    }, "Spanish plural forms")
    before = output.read_bytes()
    h.call("import_strings", {**params, "file_paths": [str(legacy[0])]}, "mutually exclusive sources", "exactly one")
    h.equal(output.read_bytes(), before, "legacy parameter error preserves output")
    utf16 = h.copy("utf16le.strings", "utf16/en.lproj/Localizable.strings")
    utf_output = h.temporary / "utf16.xcstrings"
    utf = h.call("import_strings", {"file_paths": [str(utf16)], "source_language": "en", "output_path": str(utf_output)}, "UTF16 migration")
    h.equal((utf["total_keys"], utf["plural_keys"]), (2, 0), "UTF16 counts")
    h.equal({key: value["localizations"]["en"]["stringUnit"]["value"]
             for key, value in h.read(utf_output)["strings"].items()},
            {"utf16.key1": "Hello UTF-16", "utf16.key2": "Value with accent: café"}, "UTF16 exact values")
    # Position is defined by the root format, not alphabetical map iteration.
    for locale, photos, albums in (("en", "%arg photos", "%arg albums"), ("es", "%arg fotos", "%arg álbumes")):
        subs = result["photos_in_albums"]["localizations"][locale]["substitutions"]
        h.equal((subs["photos"]["argNum"], subs["albums"]["argNum"]), (1, 2), "root positional substitution identities")
        h.equal(subs["photos"]["variations"]["plural"]["other"]["stringUnit"]["value"], photos, "photo argument text")
        h.equal(subs["albums"]["variations"]["plural"]["other"]["stringUnit"]["value"], albums, "album argument text")


def xliff_scenarios(h):
    path = h.copy("simple.xcstrings", "xliff/simple.xcstrings")
    original = h.read(path)
    h.prepare(path)
    output = h.temporary / "translated.xliff"
    result = h.on("export_xliff", path, "simple full XLIFF export", locale="ca", output_path=str(output), untranslated_only=False)
    h.equal({k: result[k] for k in ("output_path", "locale", "exported_count")}, {"output_path": str(output), "locale": "ca", "exported_count": 2}, "export report")
    namespace = {"x": "urn:oasis:names:tc:xliff:document:1.2"}
    tree = ET.parse(output)
    units = tree.findall(".//x:trans-unit", namespace)
    h.equal([(u.attrib["id"], u.find("x:source", namespace).text) for u in units],
            [("greeting", "Hello"), ("welcome_message", "Welcome to the app")], "XLIFF IDs and sources")
    translations = {"greeting": "Hola", "welcome_message": "Benvingut a l’aplicació"}
    for unit in units:
        target = unit.find("x:target", namespace)
        if target is None:
            target = ET.Element("{urn:oasis:names:tc:xliff:document:1.2}target")
            unit.insert(list(unit).index(unit.find("x:source", namespace)) + 1, target)
        target.text = translations[unit.attrib["id"]]
        target.set("state", "translated")
    tree.write(output, encoding="utf-8", xml_declaration=True)
    before = path.read_bytes()
    dry = h.on("import_xliff", path, "XLIFF dry run", xliff_path=str(output), dry_run=True)
    h.equal((dry["accepted"], dry["rejected"], dry["dry_run"]), (2, [], True), "XLIFF dry report")
    h.equal(path.read_bytes(), before, "XLIFF dry conservation")
    applied = h.on("import_xliff", path, "XLIFF apply", xliff_path=str(output))
    h.equal((applied["accepted"], applied["rejected"]), (2, []), "XLIFF import report")
    expected = copy.deepcopy(original)
    for key, text in translations.items():
        expected["strings"][key]["localizations"]["ca"] = {"stringUnit": {"state": "translated", "value": text}}
    h.equal(h.read(path), expected, "XLIFF exact catalog conservation")
    before = path.read_bytes()
    malformed = h.temporary / "malformed.xliff"
    malformed.write_text("<xliff><file>")
    h.on("import_xliff", path, "malformed XML", "XLIFF", xliff_path=str(malformed))
    h.equal(path.read_bytes(), before, "malformed XML no write")
    h.on("export_xliff", path, "invalid export extension", "extension", locale="ca", output_path=str(h.temporary / "bad.txt"))
    h.require(not (h.temporary / "bad.txt").exists(), "invalid export creates nothing")
    fixture = h.copy("xcode_26_6_empty_id.xliff", "empty-id.xliff")
    golden = h.copy("golden.xcstrings", "xliff/golden.xcstrings")
    h.prepare(golden, [""])
    # Missing target must remain distinct from explicit empty; remove only target
    # from the real Xcode empty-ID fixture to assert the non-destructive no-op.
    tree = ET.parse(fixture)
    unit = tree.find(".//x:trans-unit", namespace)
    h.equal(unit.attrib["id"], "", "real Xcode empty-key fixture")
    unit.remove(unit.find("x:target", namespace))
    tree.write(fixture, encoding="utf-8", xml_declaration=True)
    before = golden.read_bytes()
    result = h.on("import_xliff", golden, "missing target on real empty key", xliff_path=str(fixture))
    h.equal((result["accepted"], result["rejected"]), (0, []), "missing target no-op report")
    h.equal(golden.read_bytes(), before, "missing target byte conservation")


def merge_scenarios(h):
    base = h.read(h.fixture("golden.xcstrings"))
    base["strings"]["__merge.conflict"] = {"comment": "base"}
    current, incoming = copy.deepcopy(base), copy.deepcopy(base)
    current["strings"]["__merge.conflict"]["comment"] = "current"
    incoming["strings"]["__merge.conflict"]["comment"] = "incoming"
    current["strings"]["__merge.current"] = {"comment": "current-only"}
    incoming["strings"]["__merge.incoming"] = {"comment": "incoming-only"}
    paths = {name: h.temporary / f"merge-{name}.xcstrings" for name in ("base", "current", "incoming", "output")}
    for name, content in (("base", base), ("current", current), ("incoming", incoming)):
        h.write(paths[name], content)
    params = {f"{name}_path": str(path) for name, path in paths.items()}
    dry = h.call("merge_xcstrings", params, "golden three-way conflict preview")
    h.equal((dry["dry_run"], dry["written"], dry["conflict_total"], dry["unresolved_conflict_total"]),
            (True, False, 1, 1), "merge dry status")
    conflict = dry["conflicts"][0]
    h.equal((conflict["pointer"], conflict["kind"], conflict["base"]["preview"], conflict["current"]["preview"], conflict["incoming"]["preview"]),
            ("/strings/__merge.conflict/comment", "atomic_divergence", '"base"', '"current"', '"incoming"'), "exact conflict")
    h.require(not paths["output"].exists(), "merge dry run creates nothing")
    for name in ("base", "current", "incoming"):
        h.equal(dry["expected_fingerprints"][name], "sha256:" + hashlib.sha256(paths[name].read_bytes()).hexdigest(), "exact input byte fingerprint")
    h.equal(dry["expected_fingerprints"]["output"], None, "absent output fingerprint")
    apply = {**params, "dry_run": False, "expected_fingerprints": dry["expected_fingerprints"],
             "resolutions": [{"conflict_id": conflict["id"], "choice": "incoming"}]}
    before = paths["incoming"].read_bytes()
    paths["incoming"].write_bytes(before + b"\n")
    h.call("merge_xcstrings", apply, "stale input fingerprint", "fingerprint")
    h.require(not paths["output"].exists(), "stale merge creates nothing")
    paths["incoming"].write_bytes(before)
    result = h.call("merge_xcstrings", apply, "golden resolved merge apply")
    h.equal((result["written"], result["unresolved_conflict_total"], result["resolutions_applied"]), (True, 0, 1), "merge applied status")
    expected = copy.deepcopy(incoming)
    expected["strings"]["__merge.current"] = {"comment": "current-only"}
    h.equal(h.read(paths["output"]), expected, "golden entire merged catalog conservation")
    h.equal(h.read(paths["base"]), base, "base unchanged")
    h.equal(h.read(paths["current"]), current, "current unchanged")
    h.equal(h.read(paths["incoming"]), incoming, "incoming unchanged")
