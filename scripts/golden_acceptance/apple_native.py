"""Native Apple catalog acceptance over the final binary's public MCP boundary."""

import copy
import hashlib

PREFIX = "apple_xcode27/"
MATRIX = "positive/catalog-matrix/source.xcstrings"


def ordered(value):
    """Object member order is part of the catalog conservation contract."""
    if isinstance(value, dict):
        return [(key, ordered(child)) for key, child in value.items()]
    if isinstance(value, list):
        return [ordered(child) for child in value]
    return value


def leaves(node, path=()):
    """Enumerate fixture JSON directly; never call or duplicate the Rust path codec."""
    if "stringUnit" in node:
        yield list(path), node["stringUnit"]
    for axis, variants in node.get("variations", {}).items():
        for category, child in variants.items():
            yield from leaves(child, (*path, {axis: category}))
    for name, substitution in node.get("substitutions", {}).items():
        yield from leaves(substitution, (*path, {"substitution": name}))


def unit_at(node, path):
    for step in path:
        axis, name = next(iter(step.items()))
        node = node["substitutions"][name] if axis == "substitution" else node["variations"][axis][name]
    return node["stringUnit"]


def apple_inventory(h):
    manifest = h.read(h.fixture(PREFIX + "manifest.json"))
    h.fixture(PREFIX + "README.md")
    h.equal(len(manifest["files"]), 165, "reviewed Xcode corpus artifact count")
    roles = {}
    for item in manifest["files"]:
        path = h.fixtures / PREFIX / item["path"]
        h.equal(hashlib.sha256(path.read_bytes()).hexdigest(), item["sha256"], item["path"] + " oracle hash")
        # Logs, projects and observed Apple-loss snapshots are provenance, not a
        # claim that this run executed Xcode or that Apple's losses are desired.
        roles[item["path"]] = {"kind": item["kind"], "verification": "provenance hash"}
    h.report["apple_artifacts"] = roles
    h.report["apple_toolchain_provenance"] = manifest["toolchain"]
    for item in manifest["files"]:
        if item["kind"] not in ("catalog_input", "derived_catalog_input"):
            continue
        path = h.copy(PREFIX + item["path"], "apple-conservation/" + item["path"])
        before = h.read(path)
        result = h.on("parse_xcstrings", path, "Apple catalog " + item["path"])
        h.equal((result["total_keys"], result["source_language"]),
                (len(before["strings"]), before["sourceLanguage"]), "Apple catalog summary")
        key = next(iter(before["strings"]))
        expected = copy.deepcopy(before)
        expected["strings"][key]["comment"] = "Golden acceptance conservation probe"
        h.on("update_comments", path, "preserve complete Apple shape " + item["path"],
             comments=[{"key": key, "comment": "Golden acceptance conservation probe"}])
        h.equal(h.read(path), expected, "all Apple fields survive unrelated mutation")
        roles[item["path"]]["verification"] = "MCP parse and complete catalog conservation"


def native_submit(h, path, translations, case):
    original = h.read(path)
    before = path.read_bytes()
    arguments = {"translations": translations, "continue_on_error": False}
    dry = h.on("submit_translations", path, case + " dry", dry_run=True, **arguments)
    h.equal((dry["accepted"], dry["rejected"], dry["dry_run"]), (len(translations), [], True), case + " dry result")
    h.equal(path.read_bytes(), before, case + " dry bytes")
    applied = h.on("submit_translations", path, case, **arguments)
    h.equal((applied["accepted"], applied["rejected"], applied["dry_run"]), (len(translations), [], False), case + " result")
    expected_destinations = [(t["key"], t["locale"], t["path"]) for t in translations]
    actual_destinations = [(d["key"], d["locale"], d["path"]) for d in applied["accepted_destinations"]]
    h.equal(actual_destinations, expected_destinations, case + " exact leaf destinations")
    expected = copy.deepcopy(original)
    for translation in translations:
        node = expected["strings"][translation["key"]]["localizations"][translation["locale"]]
        unit = unit_at(node, translation["path"])
        unit["state"], unit["value"] = "needs_review", translation["value"]
    h.equal(ordered(h.read(path)), ordered(expected), case + " complete ordered conservation")


def apple_native_paths(h):
    path = h.copy(PREFIX + MATRIX, "apple-native/Localizable.xcstrings")
    h.prepare(path)
    original = h.read(path)
    translations = []
    for key, entry in original["strings"].items():
        for leaf_path, unit in leaves(entry.get("localizations", {}).get("fr", {})):
            translations.append({"key": key, "locale": "fr", "path": leaf_path,
                                 "value": unit["value"] + " · accepté"})
    h.equal(len(translations), 52, "reviewed positive native leaf count")
    native_submit(h, path, translations, "all positive root/device/plural/chained/substitution leaves")
    before = path.read_bytes()
    duplicate = [translations[0], {**translations[0], "value": "duplicate"}]
    rejected = h.on("submit_translations", path, "duplicate typed destinations", translations=duplicate, continue_on_error=False)
    h.equal(rejected["accepted"], 0, "duplicate destination accepted count")
    h.require(any(r.get("code") == "duplicate_destination" for r in rejected["rejected"]), "typed duplicate diagnostic")
    h.equal(path.read_bytes(), before, "duplicate typed batch byte conservation")
    mixed = [{"key": "plural", "locale": "fr", "value": "%lld leaf", "path": [{"plural": "other"}],
              "plural_forms": {"one": "%lld one", "many": "%lld many", "other": "%lld other"}}]
    rejected = h.on("submit_translations", path, "typed and legacy selectors cannot mix", translations=mixed, continue_on_error=False)
    h.equal(rejected["accepted"], 0, "mixed selectors accepted count")
    h.require(any(r.get("code") == "conflicting_selectors" for r in rejected["rejected"]), "mixed selector diagnostic")
    h.equal(path.read_bytes(), before, "mixed selectors byte conservation")
    # Rejection identity includes the path, so one invalid leaf cannot suppress
    # its valid sibling merely because both have the same key and locale.
    before = h.read(path)
    siblings = [{"key": "plural", "locale": "fr", "path": [{"plural": "one"}], "value": "missing argument"},
                {"key": "plural", "locale": "fr", "path": [{"plural": "other"}], "value": "%lld accepté seul"}]
    result = h.on("submit_translations", path, "valid sibling survives same-key invalid leaf", translations=siblings)
    h.equal(result["accepted"], 1, "one accepted sibling request")
    h.equal([(r["key"], r["locale"], r["path"], r["code"]) for r in result["rejected"]],
            [("plural", "fr", [{"plural": "one"}], "invalid_translation")], "exact rejected sibling identity")
    h.equal(result["accepted_destinations"], [{"key": "plural", "locale": "fr", "path": [{"plural": "other"}]}],
            "exact accepted sibling identity")
    before["strings"]["plural"]["localizations"]["fr"]["variations"]["plural"]["other"]["stringUnit"]["value"] = "%lld accepté seul"
    h.equal(ordered(h.read(path)), ordered(before), "sibling rejection preserves everything except accepted leaf")


def apple_native_reads(h):
    catalog = h.copy(PREFIX + MATRIX, "apple-native/read.xcstrings")
    original = h.read(catalog)
    for key, entry in original["strings"].items():
        result = h.on("get_key", catalog, "recursive native read " + key, key=key)
        translations = {item["locale"]: item for item in result["translations"]}
        for locale, node in entry.get("localizations", {}).items():
            actual = translations[locale]
            h.equal(actual["diagnostics"], [], "supported shape has no diagnostics")
            expected_leaves = list(leaves(node))
            for path, unit in expected_leaves:
                matches = [leaf for leaf in actual["leaves"] if leaf["path"] == path]
                h.equal(len(matches), 1, "exactly one leaf per typed identity")
                leaf = matches[0]
                h.equal((leaf["value"], leaf["state"], leaf["complete"]),
                        (unit["value"], unit["state"], unit["state"] in ("translated", "machine_translated")),
                        "native leaf retains text/state/readiness")
            if locale == "fr":
                missing = [leaf for leaf in actual["leaves"] if "value" not in leaf]
                expected_missing = [path[:-1] + [{"plural": "many"}]
                                    for path, _ in expected_leaves if path and path[-1] == {"plural": "other"}]
                h.equal(sorted(repr(leaf["path"]) for leaf in missing), sorted(map(repr, expected_missing)), "French required many paths")
                h.equal([(leaf["required"], leaf["complete"]) for leaf in missing],
                        [(True, False)] * len(expected_missing), "missing categories never count as complete")


def apple_delimiter_native(h):
    relative = "negative/delimiter-substitution-loss/source.xcstrings"
    path = h.copy(PREFIX + relative, "apple-native/delimiter.xcstrings")
    h.prepare(path)
    translations = []
    for key, entry in h.read(path)["strings"].items():
        for leaf_path, unit in leaves(entry.get("localizations", {}).get("fr", {})):
            if any(step.get("substitution") == "PIPE|==|NAME" for step in leaf_path):
                translations.append({"key": key, "locale": "fr", "path": leaf_path, "value": unit["value"] + " accepté"})
    h.equal(len(translations), 2, "delimiter substitution has two editable native leaves")
    native_submit(h, path, translations, "native delimiter-bearing substitution")


def apple_legacy_orphan_rejected(h):
    catalog = h.copy(PREFIX + MATRIX, "apple-native/legacy-orphan.xcstrings")
    document = h.read(catalog)
    entry = document["strings"]["substitution"]
    entry["localizations"]["fr"] = {"stringUnit": {"state": "translated", "value": "%lld sans substitution"}}
    document["strings"] = {"substitution": entry}
    h.write(catalog, document)
    h.prepare(catalog)
    before = catalog.read_bytes()
    request = {"key": "substitution", "locale": "fr", "value": "", "substitution_name": "COUNT",
               "plural_forms": {"one": "%arg élément", "many": "%arg éléments", "other": "%arg éléments"}}
    result = h.on("submit_translations", catalog, "legacy selectors cannot create an orphan substitution",
                  translations=[request], continue_on_error=False)
    h.equal(result["accepted"], 0, "orphan legacy request rejected")
    h.equal([(r["key"], r["locale"], r["code"]) for r in result["rejected"]],
            [("substitution", "fr", "invalid_path")], "legacy request structural rejection")
    h.equal(result["accepted_destinations"], [], "no orphan destinations accepted")
    h.equal(catalog.read_bytes(), before, "rejected orphan creation leaves valid target byte-identical")
