"""Workflow setup and native draft acceptance using the same MCP client as goldens."""
import importlib.util
import json
from contextlib import contextmanager
from pathlib import Path
import shutil


def require(condition, message):
    if not condition:
        raise AssertionError(message)


@contextmanager
def client(binary, directory, report):
    spec = importlib.util.spec_from_file_location("golden_protocol", Path(__file__).with_name("golden_acceptance.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    protocol_report = {"calls": []}
    instance = module.Mcp(binary, directory, protocol_report)
    try:
        yield instance
    finally:
        instance.close()
        report.setdefault("workflow_calls", []).extend(protocol_report["calls"])


def checkpoint(mcp, catalog, mode="adopt_existing"):
    arguments = {"file_path": str(catalog), "mode": mode, "dry_run": True}
    preview = mcp.call("sync_source_changes", arguments, "Xcode checkpoint preview")
    result = mcp.call("sync_source_changes", {**arguments, "dry_run": False, "expected": preview["input_revisions"]}, "Xcode checkpoint apply")
    require(not result["retry_required"] and result["phase_error"] is None, "checkpoint failed")
    return result


def adopt(binary, catalog, directory, report):
    with client(binary, directory, report) as mcp:
        checkpoint(mcp, catalog)


def native_review(binary, corpus, directory, logs, report, run, units):
    """Changed native draft -> explicit approval -> real Xcode -> source re-review."""
    directory = directory / "native-review"
    directory.mkdir()
    catalog = directory / "Localizable.xcstrings"
    initial = {"sourceLanguage": "en", "strings": {"button.open": {
        "extractionState": "manual", "comment": "Opens the current document",
        "localizations": {"en": {"stringUnit": {"state": "translated", "value": "Open document"}},
                          "de": {"stringUnit": {"state": "translated", "value": "Alte Übersetzung"}}}
    }}, "version": "1.0"}
    catalog.write_text(json.dumps(initial, ensure_ascii=False))
    project = directory / "Oracle.xcodeproj"
    project.mkdir()
    shutil.copyfile(corpus / "provenance/single-catalog-project.pbxproj", project / "project.pbxproj")
    with client(binary, directory, report) as mcp:
        context = mcp.call("update_context", {"file_path": str(catalog), "edits": [{"action": "set", "key": "button.open", "context": {"context": {"screen": "Document", "role": "button", "purpose": "Open the current document"}}}], "dry_run": True}, "authored context preview")
        mcp.call("update_context", {"file_path": str(catalog), "edits": [{"action": "set", "key": "button.open", "context": {"context": {"screen": "Document", "role": "button", "purpose": "Open the current document"}}}], "expected": context["input_revisions"]}, "authored context apply")
        checkpoint(mcp, catalog, "review")
        captured = mcp.call("get_key", {"file_path": str(catalog), "key": "button.open"}, "capture native source")
        draft = mcp.call("submit_translations", {"file_path": str(catalog), "translations": [{"key": "button.open", "locale": "de", "path": [], "expected_source_version": captured["source_version"], "value": "Dokument öffnen · geprüft"}]}, "write changed native draft")
        require(draft["accepted"] == 1 and draft["rejected"] == [], "native draft failed")
        native = json.loads(catalog.read_text())
        require(native["strings"]["button.open"]["localizations"]["de"]["stringUnit"] == {"state": "needs_review", "value": "Dokument öffnen · geprüft"}, "native draft state/text differs")
        queue = mcp.call("get_review_queue", {"file_path": str(catalog), "locale": "de"}, "review native draft")
        require(queue["total"] == 1, "expected one native review item")
        item = queue["items"][0]
        approved = mcp.call("approve_translations", {"file_path": str(catalog), "approvals": [{**item["destination"], "expected_source_version": item["source_version"], "expected_target_version": item["target_version"]}]}, "approve inspected native draft")
        require(approved["written"] and approved["report"]["accepted"] == 1, "approval failed")
        sidecar = Path(str(catalog) + ".xcstrings-mcp.json")
        metadata = sidecar.read_bytes()
        xml = directory / "approved.xliff"
        exported = mcp.call("export_xliff", {"file_path": str(catalog), "locale": "de", "original": "Localizable.xcstrings", "output_path": str(xml), "untranslated_only": False}, "export approved native value")
        expected = units(xml)
        require(expected[("Localizable.xcstrings", "button.open")]["target"] == "Dokument öffnen · geprüft", "export lost reviewed value")
        # Reset only the Xcode target to the old value so a skipped import fails comparison.
        reset = json.loads(catalog.read_text())
        reset["strings"]["button.open"]["localizations"]["de"]["stringUnit"]["value"] = "Alte Übersetzung"
        catalog.write_text(json.dumps(reset, ensure_ascii=False))
        compiled = directory / "compiled"
        compiled.mkdir()
        run(["xcodebuild", "-importLocalizations", "-project", project, "-localizationPath", xml], "native-xcode-import", logs, report, directory)
        actual = json.loads(catalog.read_text())
        require(actual["strings"]["button.open"]["localizations"]["de"]["stringUnit"] == {"state": "translated", "value": "Dokument öffnen · geprüft"}, "Xcode skipped changed approved text")
        require(sidecar.read_bytes() == metadata, "Xcode changed workflow sidecar")
        run(["xcrun", "xcstringstool", "compile", catalog, "--output-directory", compiled], "native-compile", logs, report, directory)
        reexport = directory / "reexport"
        run(["xcodebuild", "-exportLocalizations", "-project", project, "-localizationPath", reexport, "-exportLanguage", "de"], "native-xcode-reexport", logs, report, directory)
        require(units(reexport / "de.xcloc/Localized Contents/de.xliff") == expected, "Xcode changed reviewed translation/source on reexport")
        # External source edit: stale captured work must reject, while old text survives sync.
        actual["strings"]["button.open"]["localizations"]["en"]["stringUnit"]["value"] = "Open selected document"
        catalog.write_text(json.dumps(actual, ensure_ascii=False))
        stale = mcp.call("submit_translations", {"file_path": str(catalog), "translations": [{"key": "button.open", "locale": "de", "path": [], "expected_source_version": captured["source_version"], "value": "Veraltete Antwort"}]}, "reject source changed after Xcode")
        require(stale["accepted"] == 0 and stale["rejected"][0]["code"] == "source_version_mismatch", "stale source was accepted")
        checkpoint(mcp, catalog, "review")
        after = json.loads(catalog.read_text())
        require(after["strings"]["button.open"]["localizations"]["de"]["stringUnit"] == {"state": "needs_review", "value": "Dokument öffnen · geprüft"}, "source sync lost translation text")
        final_queue = mcp.call("get_review_queue", {"file_path": str(catalog), "locale": "de"}, "source-change review survives Xcode")
        require(final_queue["total"] == 1 and final_queue["items"][0]["old_source"] is not None, "source-change evidence missing")
        package = mcp.call("get_context", {"file_path": str(catalog), "key": "button.open", "locale": "de", "count": 0}, "context sidecar survives Xcode")
        require(package["authored"]["fields"]["screen"] == "Document", "authored context was lost")
    return {"name": "native-draft-approval-source-change", "status": "passed", "changed_units": 1, "sidecar_preserved": True, "stale_source_rejected": True}
