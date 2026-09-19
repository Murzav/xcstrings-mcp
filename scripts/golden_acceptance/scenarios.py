"""Original catalog inspection and mutation conservation through real MCP."""

import copy


CATALOGS = {
    "golden.xcstrings": 638, "should_not_translate.xcstrings": 2,
    "simple.xcstrings": 2, "with_device_variants.xcstrings": 2,
    "with_interpolation.xcstrings": 3, "with_multiline.xcstrings": 2,
    "with_plurals.xcstrings": 5, "with_stale.xcstrings": 4,
    "with_substitutions.xcstrings": 3, "xcode26.xcstrings": 2,
}


def catalog_scenarios(h):
    h.equal(h.call("list_files", {}, "empty cache"), [])
    for fixture, count in CATALOGS.items():
        path = h.copy(fixture)
        original = h.read(path)
        h.equal(len(original["strings"]), count, fixture + " pinned key count")
        result = h.on("parse_xcstrings", path, fixture + " parse")
        h.equal(result["total_keys"], count, fixture + " parsed count")
        h.equal(result["source_language"], original["sourceLanguage"], fixture + " source")
        expected_locales = sorted({locale for entry in original["strings"].values()
                                   for locale in entry.get("localizations", {})})
        h.equal(sorted(result["locales"]), expected_locales, fixture + " locales")
        h.equal(result["translatable_keys"], sum(entry.get("shouldTranslate", True)
                                                for entry in original["strings"].values()),
                fixture + " translatable count")
        # Every original golden key is inspected, including empty/whitespace/Unicode keys.
        for key, entry in original["strings"].items():
            actual = h.on("get_key", path, fixture + " key", key=key)
            h.equal(actual["key"], key, "key identity")
            h.equal(actual["comment"], entry.get("comment"), "comment preservation")
            h.equal(actual["should_translate"], entry.get("shouldTranslate", True), "translate flag")
            localizations = entry.get("localizations", {})
            source = localizations.get(original["sourceLanguage"], {}).get("stringUnit", {})
            h.equal(actual["source_text"], source.get("value", key), "source text")
            h.equal({item["locale"] for item in actual["translations"]}, set(localizations), "locale identities")
            for item in actual["translations"]:
                unit = localizations[item["locale"]].get("stringUnit", {})
                h.equal(item["value"], unit.get("value"), "exact target text")
                h.equal(item["state"], unit.get("state"), "exact target state")
        # A real write forces the full typed parse/serialize path for every fixture.
        first = next(iter(original["strings"]))
        expected = copy.deepcopy(original)
        expected["strings"][first]["comment"] = "Golden acceptance: café / 一"
        h.equal(h.on("update_comments", path, fixture + " conservation", comments=[{
            "key": first, "comment": "Golden acceptance: café / 一"}]), {"updated": 1}, "comment update report")
        h.equal(h.read(path), expected, fixture + " full catalog conservation")

    files = h.call("list_files", {}, "all original catalogs cached")
    h.equal({item["path"] for item in files}, {str(h.temporary / name) for name in CATALOGS}, "cached paths")
    h.equal(sum(item["is_active"] for item in files), 1, "one active file")
    h.on("get_key", h.temporary / "simple.xcstrings", "missing key", "not found", key="absent-key")
    h.on("parse_xcstrings", h.temporary / "wrong.txt", "invalid extension", "xcstrings")

    path = h.copy("simple.xcstrings", "inspect/simple.xcstrings")
    coverage = h.on("get_coverage", path, "simple exact coverage")
    h.equal(coverage, {"source_language": "en", "total_keys": 2, "translatable_keys": 2,
        "locales": [
            {"locale": "en", "total_keys": 2, "translatable_keys": 2, "translated": 2, "percentage": 100.0},
            {"locale": "uk", "total_keys": 2, "translatable_keys": 2, "translated": 1, "percentage": 50.0},
        ]}, "coverage report")
    h.equal(h.on("list_locales", path, "exact locale coverage"), [
        {"locale": "en", "translated": 2, "total": 2, "percentage": 100.0},
        {"locale": "uk", "translated": 1, "total": 2, "percentage": 50.0},
    ], "locale report")
    untranslated = h.on("get_untranslated", path, "missing Ukrainian translation", locale="uk")
    h.equal([u["key"] for u in untranslated["units"]], ["welcome_message"], "untranslated keys")
    h.equal(untranslated["total"], 1, "untranslated total")
    h.equal(untranslated["units"][0]["source_text"], "Welcome to the app", "untranslated source")
    end = h.on("get_untranslated", path, "pagination end", locale="uk", offset=1)
    h.equal((end["units"], end["total"], end["has_more"]), ([], 1, False), "end page")
    for tool in ("get_untranslated", "get_stale", "get_plurals"):
        h.on(tool, path, "invalid page size", "batch_size", locale="uk", batch_size=0)
    search = h.on("search_keys", path, "case insensitive source search", locale="uk", pattern="HELLO")
    h.equal([u["key"] for u in search["units"]], ["greeting"], "search results")
    h.equal(search["total"], 1, "search total")
    context = h.on("get_context", path, "neighbor context", key="greeting", locale="uk", count=1)
    h.equal([{k: v for k, v in item.items() if k not in ("leaves", "diagnostics")} for item in context],
            [{"key": "welcome_message", "source_text": "Welcome to the app"}], "legacy context text")
    h.equal(context[0]["diagnostics"], [], "simple context diagnostics")
    h.equal(context[0]["leaves"], [{"path": [], "source_text": "Welcome to the app", "required": True,
                                   "complete": False, "substitutions": []}], "context missing root leaf")
    h.equal(h.on("get_context", path, "unknown context key", key="absent", locale="uk"), [], "unknown context")
    stale = h.on("get_stale", h.temporary / "with_stale.xcstrings", "stale excludes nontranslatable", locale="uk")
    h.equal([u["key"] for u in stale["units"]], ["removed_feature", "renamed_key"], "stale keys")
    for fixture, names in (
        ("with_plurals.xcstrings", ["days_remaining", "items_count", "photos_count"]),
        ("with_device_variants.xcstrings", ["home_button", "tap_action"]),
        ("with_substitutions.xcstrings", ["bird_sighting", "file_count", "simple_with_sub"]),
    ):
        result = h.on("get_plurals", h.temporary / fixture, fixture + " varied inspection", locale="fr")
        h.equal([unit["key"] for unit in result["units"]], names, fixture + " varied keys")
        h.equal(result["total"], len(names), fixture + " varied total")
    h.equal(h.on("validate_translations", path, "clean existing translation", locale="uk"),
            [{"locale": "uk", "errors": [], "warnings": []}], "clean validation")


def mutation_scenarios(h):
    path = h.copy("golden.xcstrings", "mutations/golden.xcstrings")
    expected = h.read(path)
    original_keys = set(expected["strings"])
    entries = [
        {"key": "__acceptance.title", "source_text": "A title", "comment": "Title context"},
        {"key": "__acceptance.count", "source_text": "%lld objects"},
    ]
    h.equal(h.on("add_keys", path, "golden add two asymmetric keys", keys=entries),
            {"added": 2, "skipped": []}, "add keys report")
    current = h.read(path)
    h.equal({k: v for k, v in current["strings"].items() if k in original_keys}, expected["strings"], "golden original keys conserved")
    h.equal(set(current["strings"]) - original_keys, {"__acceptance.title", "__acceptance.count"}, "only requested keys added")
    h.equal(h.on("add_keys", path, "duplicate key", keys=[entries[0]]),
            {"added": 0, "skipped": ["__acceptance.title"]}, "duplicate report")
    h.equal(h.read(path), current, "duplicate does not change catalog")
    requests = [{"key": "__acceptance.title", "locale": "ca", "value": "Un títol"},
                {"key": "__acceptance.count", "locale": "ca", "value": "%lld objectes"}]
    before = path.read_bytes()
    dry = h.on("submit_translations", path, "golden dry run", translations=requests, dry_run=True)
    h.equal((dry["accepted"], dry["rejected"], dry["dry_run"]), (2, [], True), "submit dry report")
    h.equal(path.read_bytes(), before, "dry run byte conservation")
    applied = h.on("submit_translations", path, "golden apply", translations=requests)
    h.equal((applied["accepted"], applied["rejected"]), (2, []), "submit apply report")
    expected = current
    for request in requests:
        expected["strings"][request["key"]]["localizations"]["ca"] = {
            "stringUnit": {"state": "translated", "value": request["value"]}}
    h.equal(h.read(path), expected, "golden exact translated catalog")
    before = path.read_bytes()
    rejected = h.on("submit_translations", path, "atomic mixed-validity submission", continue_on_error=False,
                    translations=[{**requests[0], "value": "Must not persist"}, {**requests[1], "value": "wrong"}])
    h.equal(rejected["accepted"], 0, "invalid batch accepts zero")
    h.equal([item["key"] for item in rejected["rejected"]], ["__acceptance.title", "__acceptance.count"], "all rejected batch keys")
    h.equal(rejected["rejected"][0]["reason"], "batch rejected due to other failures", "valid entry rejected atomically")
    h.require("format" in rejected["rejected"][1]["reason"], "specific format rejection")
    h.equal(path.read_bytes(), before, "invalid submit byte conservation")
    h.equal(h.on("rename_key", path, "golden rename", old_key="__acceptance.title", new_key="__acceptance.heading"),
            {"old_key": "__acceptance.title", "new_key": "__acceptance.heading"}, "rename report")
    expected["strings"]["__acceptance.heading"] = expected["strings"].pop("__acceptance.title")
    h.equal(h.read(path), expected, "rename conserves all translations and metadata")
    before = path.read_bytes()
    h.on("rename_key", path, "rename collision", "already exists", old_key="__acceptance.heading", new_key="__acceptance.count")
    h.equal(path.read_bytes(), before, "rename collision no write")
    h.on("delete_translations", path, "protect source locale", "source", keys=["__acceptance.heading"], locale="en")
    h.equal(path.read_bytes(), before, "source translation protection")
    result = h.on("delete_translations", path, "delete one target", keys=["__acceptance.heading"], locale="ca")
    h.equal(result["reset"], ["__acceptance.heading"], "deleted target keys")
    del expected["strings"]["__acceptance.heading"]["localizations"]["ca"]
    h.equal(h.read(path), expected, "delete target conservation")
    h.equal(h.on("delete_keys", path, "delete keys plus missing key", keys=["__acceptance.heading", "__acceptance.count", "missing"]),
            {"deleted": ["__acceptance.heading", "__acceptance.count"], "not_found": ["missing"]}, "delete report")
    h.equal(h.read(path), h.read(h.fixture("golden.xcstrings")), "golden restored semantically")

    simple = h.copy("simple.xcstrings", "mutations/simple.xcstrings")
    before = h.read(simple)
    h.equal(h.on("add_locale", simple, "add missing locale", locale="ca"), {"added": 2, "locale": "ca"}, "add locale report")
    expected = copy.deepcopy(before)
    for entry in expected["strings"].values():
        entry["localizations"]["ca"] = {"stringUnit": {"state": "new", "value": ""}}
    h.equal(h.read(simple), expected, "locale initialization")
    raw = simple.read_bytes()
    h.on("add_locale", simple, "duplicate locale", "already exists", locale="ca")
    h.on("remove_locale", simple, "protect source", "source", locale="en")
    h.equal(simple.read_bytes(), raw, "locale errors no write")
    h.equal(h.on("remove_locale", simple, "remove locale", locale="ca"), {"removed": 2, "locale": "ca"}, "remove locale report")
    h.equal(h.read(simple), before, "locale cycle conservation")
    h.on("parse_xcstrings", simple, "prime diff cache")
    changed = copy.deepcopy(before)
    changed["strings"]["greeting"]["localizations"]["en"]["stringUnit"]["value"] = "Changed source"
    h.write(simple, changed)
    h.equal(h.on("get_diff", simple, "external source change"), {
        "added": [], "removed": [], "modified": [{"key": "greeting", "old_value": "Hello", "new_value": "Changed source"}]}, "source diff")

    created = h.temporary / "new/catalog.xcstrings"
    h.equal(h.on("create_xcstrings", created, "create catalog", source_language="en"),
            {"path": str(created), "source_language": "en"}, "create report")
    h.equal(h.read(created), {"sourceLanguage": "en", "strings": {}, "version": "1.0"}, "new empty catalog")
    raw = created.read_bytes()
    h.on("create_xcstrings", created, "protect existing catalog", "already exists", source_language="en")
    h.equal(created.read_bytes(), raw, "create collision no write")
    empty = h.call("get_glossary", {"source_locale": "en", "target_locale": "ca"}, "empty glossary")
    h.equal(empty, {"source_locale": "en", "target_locale": "ca", "entries": {}, "count": 0}, "empty glossary")
    h.equal(h.call("update_glossary", {"source_locale": "en", "target_locale": "ca", "entries": {"Settings": "Configuració", "Save": "Desa"}}, "persist glossary"),
            {"updated": 2, "source_locale": "en", "target_locale": "ca"}, "glossary update")
    h.equal(h.call("get_glossary", {"source_locale": "en", "target_locale": "ca", "filter": "CONFIG"}, "translated-value filter"),
            {"source_locale": "en", "target_locale": "ca", "entries": {"Settings": "Configuració"}, "count": 1}, "filtered glossary")
    h.require((h.temporary / "glossary.json").is_file(), "glossary persisted")
