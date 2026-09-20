"""Draft, review, source/context guards and advisory guidance through real MCP."""

import copy


def approval(item):
    return {**item["destination"], "expected_source_version": item["source_version"],
            "expected_target_version": item["target_version"]}


def queue_item(h, path):
    page = h.on("get_review_queue", path, "inspect exact existing draft", locale="fr")
    h.equal(page["total"], 1, "one physical draft to review")
    h.equal(page["items"][0]["destination"], {"key": "account", "locale": "fr", "path": []})
    return page["items"][0]


def workflow_scenarios(h):
    path = h.temporary / "workflow.xcstrings"
    document = {"sourceLanguage": "en", "version": "1.0", "strings": {
        "account": {"comment": "Billing count, not a login name", "localizations": {
            "en": {"stringUnit": {"state": "translated", "value": "Account %lld Acme"}},
            "fr": {"stringUnit": {"state": "translated", "value": "Compte %lld Acme"}}}},
        "unrelated": {"localizations": {
            "en": {"stringUnit": {"state": "translated", "value": "Help"}},
            "fr": {"stringUnit": {"state": "translated", "value": "Aide"}}}},
    }}
    h.write(path, document)
    first = h.on("get_context", path, "untracked current context", key="account", locale="fr", count=0)
    h.equal((first["tracking"], first["current"]["comment"], first["neighbors"]),
            ("uninitialized", "Billing count, not a login name", []))
    h.equal(first["leaf_contexts"][0]["variables"][0]["meaning"], None, "never invent variable meaning")
    authored = {"context": {"screen": "Billing", "role": "label", "purpose": "Count linked accounts",
        "variables": [{"reference": {"argument": 1}, "meaning": "Number of linked accounts"}],
        "neighbors": ["unrelated"], "screenshots": [{"uri": "screens/billing.png", "caption": "Billing label"}],
        "future_context": {"keep": [3, 1]}}, "leaves": [], "future_record": "preserve"}
    edit = [{"action": "set", "key": "account", "context": authored}]
    before = path.read_bytes()
    preview = h.on("update_context", path, "context preview", edits=edit, dry_run=True)
    h.equal((preview["written"], preview["changed"], preview["rejected"]), (False, True, []))
    h.equal(path.read_bytes(), before, "context preview catalog conservation")
    applied = h.on("update_context", path, "authored context apply", edits=edit, expected=preview["input_revisions"])
    h.equal((applied["written"], applied["rejected"]), (True, []))
    h.equal(path.read_bytes(), before, "authored metadata never changes Xcode fields")
    context = h.on("get_context", path, "resolved authored package", key="account", locale="fr", count=5)
    h.equal(context["authored_contexts"], {"account": authored}, "raw records preserve unknown fields")
    h.equal([(n["unit"]["key"], n["relation"]) for n in context["neighbors"]], [("unrelated", "explicit")])
    h.equal(context["leaf_contexts"][0]["variables"], [{"reference": {"argument": 1}, "position": 1,
        "role": "value", "format": "%lld", "meaning": "Number of linked accounts", "meaning_origin": "key"}])
    h.equal([d["code"] for d in context["diagnostics"]], ["screenshot_availability_unverified"])
    h.prepare(path)
    captured = dict(h.sources[str(path)])

    glossary = h.call("get_glossary", {"source_locale": "en", "target_locale": "fr"}, "capture guidance revision")
    rules = [{"id": "account-label", "source_locale": "en", "target_locale": "fr", "source": "Account",
              "preferred": ["Compte"], "forbidden": ["Profil"], "scope": {"roles": ["label"]},
              "accepted_variants": ["Comptes"], "future_rule": {"keep": True}},
             {"id": "brand", "source_locale": "en", "target_locale": "fr", "source": "Acme", "do_not_translate": True}]
    policy_path = h.temporary / "glossary.json"
    policy_before = policy_path.read_bytes() if policy_path.exists() else None
    dry = h.call("update_glossary", {"upsert": rules, "dry_run": True}, "rich glossary preview")
    h.equal((dry["written"], dry["updated"], dry["rejected"]), (False, 2, []))
    h.equal(policy_path.read_bytes() if policy_path.exists() else None, policy_before, "glossary preview conserves bytes")
    policy = h.call("update_glossary", {"upsert": rules, "expected_revision": glossary["revision"]}, "rich glossary guarded apply")
    h.equal((policy["written"], policy["updated"], policy["rejected"]), (True, 2, []))
    h.call("update_glossary", {"remove_ids": ["brand"], "expected_revision": glossary["revision"]},
           "stale policy revision", "stale_glossary_revision")
    current = h.on("get_context", path, "relevant terminology context", key="account", locale="fr", count=0)
    h.equal([item["term"]["id"] for item in current["leaf_contexts"][0]["terminology"]["terms"]],
            ["account-label", "brand"], "only relevant terms supplied")
    h.equal(current["current"]["source_version"], captured["account"], "glossary changes never invalidate source")

    request = {"key": "account", "locale": "fr", "path": [], "value": "Profil %lld Marque"}
    before = path.read_bytes()
    dry = h.on("submit_translations", path, "advisory QA preview", translations=[request], dry_run=True)
    issues = [(i["code"], i["term_id"], i["key"], i["locale"], i["path"]) for i in dry["guidance"]["issues"]]
    h.equal(issues, [("glossary_preferred_missing", "account-label", "account", "fr", []),
                     ("glossary_forbidden_used", "account-label", "account", "fr", []),
                     ("glossary_untranslatable_changed", "brand", "account", "fr", [])])
    h.equal((dry["accepted"], dry["rejected"]), (1, []), "terminology is advisory")
    h.equal(path.read_bytes(), before, "draft preview no write")
    saved = h.on("submit_translations", path, "advisory violations still save draft", translations=[request])
    h.equal(saved["guidance"], dry["guidance"], "same policy and candidate yield identical QA")
    expected = copy.deepcopy(document)
    expected["strings"]["account"]["localizations"]["fr"]["stringUnit"] = {"state": "needs_review", "value": request["value"]}
    h.equal(h.read(path), expected, "native submission changes only requested draft")
    old_review = queue_item(h, path)
    h.equal(old_review["reasons"], ["draft"])
    replacement = {**request, "value": "Comptes %lld Acme"}
    saved = h.on("submit_translations", path, "explicit inflection variant", translations=[replacement])
    h.equal(saved["guidance"]["issues"], [], "authored accepted variant passes")
    before = path.read_bytes()
    rejected = h.on("approve_translations", path, "stale target approval", approvals=[approval(old_review)])
    h.equal((rejected["written"], rejected["report"]["accepted"]), (False, 0))
    h.equal([r["code"] for r in rejected["report"]["rejected"]], ["target_version_mismatch"])
    h.equal(path.read_bytes(), before, "stale approval no write")
    item = queue_item(h, path)
    review = h.on("approve_translations", path, "review preview", approvals=[approval(item)], dry_run=True)
    h.equal((review["written"], review["report"]["accepted"]), (False, 1))
    h.equal(path.read_bytes(), before, "review preview preserves draft")
    approved = h.on("approve_translations", path, "explicit approval", approvals=[approval(item)])
    h.equal((approved["written"], approved["report"]["accepted"], approved["report"]["rejected"]), (True, 1, []))
    expected["strings"]["account"]["localizations"]["fr"]["stringUnit"] = {"state": "translated", "value": replacement["value"]}
    h.equal(h.read(path), expected, "approval changes state only")
    h.equal(h.on("get_review_queue", path, "approved queue empty", locale="fr")["items"], [])

    # Export captures source versions before a later external source edit.
    output = h.temporary / "workflow.xliff"
    exported = h.on("export_xliff", path, "source-bound XML export", locale="fr", output_path=str(output), untranslated_only=False)
    held_xml = dict(exported["source_versions"])
    expected["strings"]["account"]["localizations"]["en"]["stringUnit"]["value"] = "Account %lld Acme updated"
    h.write(path, expected)
    before = path.read_bytes()
    stale = h.on("submit_translations", path, "old source translation rejected", translations=[replacement])
    h.equal((stale["accepted"], [r["code"] for r in stale["rejected"]]), (0, ["source_version_mismatch"]))
    xml = h.on("import_xliff", path, "old XML manifest rejected", xliff_path=str(output), expected_source_versions=held_xml)
    h.equal(xml["accepted"], 0)
    h.equal({r["code"] for r in xml["rejected"]}, {"source_text_mismatch"}, "stale XML source text rejected before token validation")
    h.equal(path.read_bytes(), before, "stale native and XML writes preserve catalog")
    view = h.on("get_key", path, "effective stale read", key="account")
    h.equal(view["source_freshness"], "source_changed")
    target = next(t for t in view["translations"] if t["locale"] == "fr")
    h.equal((target["state"], target["leaves"][0]["complete"]), ("needs_review", False))
    h.equal(path.read_bytes(), before, "effective stale view never writes")
    sync = h.checkpoint(path, "review")
    h.equal(sync["report"]["changed_keys"], ["account"])
    h.equal(sync["report"]["invalidated_destinations"], [{"key": "account", "locale": "fr", "path": []}])
    expected["strings"]["account"]["localizations"]["fr"]["stringUnit"]["state"] = "needs_review"
    h.equal(h.read(path), expected, "source synchronization persists review state only")
    item = queue_item(h, path)
    h.require("source_changed" in item["reasons"], "checkpoint retains bounded source change evidence")
    h.capture_sources(path)
    current_source = h.sources[str(path)]["account"]
    context_xml = h.temporary / "workflow-context.xliff"
    context_export = h.on("export_xliff", path, "capture XML before authored context changes", locale="fr",
                          output_path=str(context_xml), untranslated_only=False)
    raw = h.on("get_context", path, "capture context before authored edit", key="account", locale="fr", count=0)
    changed = copy.deepcopy(raw["authored_contexts"]["account"])
    changed["context"]["purpose"] = "Count linked business accounts"
    result = h.on("update_context", path, "authored meaning changes source version",
                  edits=[{"action": "set", "key": "account", "context": changed}], expected=raw["input_revisions"])
    h.equal(result["written"], True)
    current = h.on("get_context", path, "changed authored meaning", key="account", locale="fr", count=0)
    h.require(current["current"]["source_version"] != current_source, "authored context invalidates source token")
    h.equal(current["authored_contexts"]["account"]["future_record"], "preserve")
    before = path.read_bytes()
    stale = h.on("approve_translations", path, "old context approval rejected", approvals=[approval(item)])
    h.equal([r["code"] for r in stale["report"]["rejected"]], ["source_version_mismatch"])
    h.equal(path.read_bytes(), before, "context-stale approval conserves catalog")
    stale_xml = h.on("import_xliff", path, "context-stale XML manifest rejected", xliff_path=str(context_xml),
                     expected_source_versions=context_export["source_versions"])
    h.equal((stale_xml["accepted"], {r["code"] for r in stale_xml["rejected"]}),
            (0, {"source_version_mismatch"}), "unchanged text cannot bypass stale authored-context token")
    h.equal(path.read_bytes(), before, "context-stale XML conserves catalog")
    h.report["workflow_acceptance"] = {"tools": ["get_review_queue", "approve_translations", "sync_source_changes", "update_context"],
        "drafts": True, "stale_source_and_target_guards": True, "context_versioning": True,
        "advisory_glossary": True, "inert_screenshot_references": True}
