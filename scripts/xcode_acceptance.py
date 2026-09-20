#!/usr/bin/env python3
"""Verify the supplied binary against the real Xcode compiler/import/export.

Runs only on disposable project/catalog copies. The proof is bound to the binary
SHA-256 and is consumed by golden_acceptance.py --xcode-report.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET

from xcode_workflow import adopt, native_review

NS = "urn:oasis:names:tc:xliff:document:1.2"
ET.register_namespace("", NS)
SUFFIX = " · oracle ✓"
SOURCELESS_NEW_CASES = {
    "chained|==|device.iphone.plural.many",
    "multi|==|substitutions.BIRDS.plural.many",
    "multi|==|substitutions.YARDS.plural.many",
    "name_A..B|==|substitutions.@A..B@.plural.many",
    "name_é|==|substitutions.é.plural.many",
    "plural|==|plural.many",
    "source_only_plural|==|plural.many",
    "sub_DOT.NAME|==|substitutions.@DOT.NAME@.plural.many",
    "substitution|==|substitutions.COUNT.plural.many",
    "target_only_plural|==|plural.many",
    "target_only_substitution|==|substitutions.COUNT.plural.many",
    "varied|==|key|==|plural.many",
}


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def units(path):
    result = {}
    for section in ET.parse(path).getroot().findall(f"{{{NS}}}file"):
        for unit in section.findall(f".//{{{NS}}}trans-unit"):
            target = unit.find(f"{{{NS}}}target")
            source = unit.find(f"{{{NS}}}source")
            identity = (section.attrib.get("original"), unit.attrib["id"])
            require(identity not in result, f"duplicate oracle identity {identity}")
            result[identity] = {
                "source": None if source is None else "".join(source.itertext()),
                "target": None if target is None else "".join(target.itertext()),
                "attributes": {} if target is None else dict(target.attrib),
            }
    return result


def normalize_references(value):
    """The corpus proves Xcode removes redundant positions from named references.

    Normalize only when the position equals this very substitution's argNum;
    malformed positions remain different and cannot be hidden by this comparison.
    """
    result = copy.deepcopy(value)

    def visit(node, substitutions):
        unit = node.get("stringUnit")
        if unit is not None:
            for name, sub in substitutions.items():
                position = sub.get("argNum")
                if position is not None:
                    unit["value"] = unit["value"].replace(f"%{position}$#@{name}@", f"%#@{name}@")
        for axis in ("device", "plural"):
            for child in node.get("variations", {}).get(axis, {}).values():
                visit(child, substitutions)

    for entry in result["strings"].values():
        for localization in entry.get("localizations", {}).values():
            substitutions = localization.get("substitutions", {})
            visit(localization, substitutions)
            for substitution in substitutions.values():
                visit(substitution, {})
    return result


def run(command, label, logs, report, cwd):
    completed = subprocess.run(list(map(str, command)), cwd=cwd, capture_output=True, text=True, timeout=120)
    log = logs / f"{label}.log"
    log.write_text("COMMAND: " + repr(list(map(str, command))) + "\n" + completed.stdout + completed.stderr)
    report["commands"].append({"label": label, "exit": completed.returncode, "log_sha256": sha(log)})
    require(completed.returncode == 0, f"{label} failed: {log}")
    return completed.stdout


def matrix(binary, corpus, directory, logs, report):
    fixture = corpus / "positive/catalog-matrix-safe/source.xcstrings"
    initial = read(fixture)
    unsafe_key = "ambiguous|==|plural.one"
    original_matrix = read(corpus / "positive/catalog-matrix/source.xcstrings")
    del original_matrix["strings"][unsafe_key]
    require(initial == original_matrix, "safe matrix must exclude exactly the proven unsafe literal key")
    catalog = directory / "Localizable.xcstrings"
    tool_catalog = directory / "tool-result.xcstrings"
    shutil.copyfile(fixture, catalog)
    shutil.copyfile(fixture, tool_catalog)
    project = directory / "Oracle.xcodeproj"
    project.mkdir()
    shutil.copyfile(corpus / "provenance/single-catalog-project.pbxproj", project / "project.pbxproj")
    compiled = directory / "compiled"
    compiled.mkdir()
    run(["xcrun", "xcstringstool", "compile", catalog, "--output-directory", compiled], "compile-before", logs, report, directory)

    exported = directory / "tool.xliff"
    adopt(binary, tool_catalog, directory, report)
    response = json.loads(run([binary, "export", tool_catalog, "--locale", "fr", "--all", "--original", "Localizable.xcstrings", "--output", exported, "--json"], "tool-export", logs, report, directory))
    tool_units = units(exported)
    require(response["exported_count"] == len(tool_units), "export count differs from XML unit count")
    recorded = units(corpus / "positive/catalog-matrix/exported.xliff")
    del recorded[("Localizable.xcstrings", unsafe_key)]
    for identity, expected in recorded.items():
        require(identity in tool_units, f"missing real Xcode unit {identity}")
        require(tool_units[identity] == expected, f"tool export disagrees with recorded Apple unit {identity}: {tool_units[identity]} != {expected}")

    # Preserve every placeholder, but change every existing target so an Xcode
    # import that silently skips the file cannot produce a passing result.
    tree = ET.parse(exported)
    edited = {}
    for section in tree.getroot().findall(f"{{{NS}}}file"):
        for unit in section.findall(f".//{{{NS}}}trans-unit"):
            source = unit.find(f"{{{NS}}}source")
            target = unit.find(f"{{{NS}}}target")
            if target is None:
                target = ET.Element(f"{{{NS}}}target", {"state": "translated"})
                unit.insert(list(unit).index(source) + 1, target)
                target.text = "".join(source.itertext())
                # Apple's source for a target-only substitution is its ID,
                # which is context, not a valid translated format string.
                if unit.attrib["id"] == "target_only_substitution|==|substitutions.COUNT.plural.many":
                    target.text = "%1$lld éléments"
            target.text = (target.text or "") + SUFFIX
            edited[(section.attrib["original"], unit.attrib["id"])] = {
                "source": "".join(source.itertext()), "target": target.text, "attributes": dict(target.attrib)
            }
    incoming = directory / "edited.xliff"
    # ElementTree leaves CR literal in text, which XML readers normalize to LF.
    # Numeric references preserve the exact source and edited target payloads.
    incoming.write_bytes(ET.tostring(tree.getroot(), encoding="utf-8", xml_declaration=True).replace(b"\r", b"&#13;"))
    require(units(incoming) == edited, "oracle XML serialization changed unit identities or text")
    shutil.copyfile(incoming, logs / "edited.xliff")
    shutil.copyfile(exported, logs / "tool-exported.xliff")
    imported = json.loads(run([binary, "import", tool_catalog, "--xliff", incoming, "--source-versions", str(exported) + ".source-versions.json", "--original", "Localizable.xcstrings", "--json"], "tool-import", logs, report, directory))
    require(imported["rejected"] == [] and imported["accepted"] == len(edited) and imported["written"], "tool import did not accept every edited leaf")
    run(["xcrun", "xcstringstool", "compile", tool_catalog, "--output-directory", compiled], "compile-tool-result", logs, report, directory)
    expected_catalog = read(tool_catalog)
    # The original, unedited Apple-only roundtrip already upgraded this fixture
    # from 1.0 to 1.1. This finite format-version normalization is not a blanket
    # exemption for metadata changes.
    expected_catalog["version"] = read(corpus / "positive/catalog-matrix/expected-after-import.xcstrings")["version"]
    for key, state in (("state_new", "new"), ("state_needs_review", "needs_review")):
        target = expected_catalog["strings"][key]["localizations"]["fr"]["stringUnit"]
        require(target == {"state": state, "value": initial["strings"][key]["localizations"]["fr"]["stringUnit"]["value"] + SUFFIX}, f"tool lost draft state/text for {key}")
        # This is the documented intentional difference: Xcode skips drafts.
        expected_catalog["strings"][key]["localizations"]["fr"] = copy.deepcopy(initial["strings"][key]["localizations"]["fr"])

    run(["xcodebuild", "-importLocalizations", "-project", project, "-localizationPath", incoming], "xcode-import", logs, report, directory)
    actual_catalog = read(catalog)
    if normalize_references(actual_catalog) != normalize_references(expected_catalog):
        (logs / "expected-catalog.json").write_text(json.dumps(expected_catalog, ensure_ascii=False, indent=2))
        (logs / "actual-catalog.json").write_text(json.dumps(actual_catalog, ensure_ascii=False, indent=2))
        raise AssertionError(f"whole-catalog Xcode/tool semantic mismatch; see {logs}")
    run(["xcrun", "xcstringstool", "compile", catalog, "--output-directory", compiled], "compile-after", logs, report, directory)
    reexport = directory / "reexport"
    run(["xcodebuild", "-exportLocalizations", "-project", project, "-localizationPath", reexport, "-exportLanguage", "fr"], "xcode-reexport", logs, report, directory)
    reexported_file = reexport / "fr.xcloc/Localized Contents/fr.xliff"
    shutil.copyfile(reexported_file, logs / "reexported.xliff")
    actual_units = units(reexported_file)
    require({identity[1] for identity, unit in actual_units.items() if unit["source"] is None} == SOURCELESS_NEW_CASES, "unexpected source-less Xcode units")
    verified = []
    for identity, expected in edited.items():
        require(identity in actual_units, f"Xcode reexport lost leaf {identity}")
        if expected["attributes"].get("state") in ("new", "needs-review-l10n"):
            expected = recorded[identity]
        if identity[1] in SOURCELESS_NEW_CASES:
            require(identity not in recorded and tool_units[identity]["target"] is None, "source omission is only expected for the twelve newly added cases")
            expected = {**expected, "source": None}
        require(actual_units[identity] == expected, f"Xcode reexport mismatch {identity}: {actual_units[identity]} != {expected}")
        verified.append(identity[1])
    reimport_catalog = directory / "reimport.xcstrings"
    shutil.copyfile(catalog, reimport_catalog)
    adopt(binary, reimport_catalog, directory, report)
    reimport_map_xml = directory / "reimport-source.xliff"
    run([binary, "export", reimport_catalog, "--locale", "fr", "--all", "--original", "Localizable.xcstrings", "--output", reimport_map_xml, "--json"], "capture-reimport-source", logs, report, directory)
    reimported = json.loads(run([binary, "import", reimport_catalog, "--xliff", reexported_file, "--source-versions", str(reimport_map_xml) + ".source-versions.json", "--original", "Localizable.xcstrings", "--json"], "tool-reimport-xcode", logs, report, directory))
    require(reimported["rejected"] == [] and reimported["accepted"] == len(actual_units) and reimported["written"], "tool cannot reimport Xcode's actual reexport")
    require(normalize_references(read(reimport_catalog)) == normalize_references(actual_catalog), "reimporting Xcode output changed semantic catalog data")
    return {"name": "catalog-matrix", "status": "passed", "keys": len(initial["strings"]), "changed_units": len(edited), "verified_unit_ids": verified, "draft_policy_difference": ["state_new", "state_needs_review"]}


def unsafe_literal(binary, corpus, directory, logs, report):
    catalog = directory / "unsafe-literal.xcstrings"
    shutil.copyfile(corpus / "positive/catalog-matrix/source.xcstrings", catalog)
    before = catalog.read_bytes()
    destination = directory / "unsafe.xliff"
    destination.write_bytes(b"existing output must survive")
    command = [str(binary), "export", str(catalog), "--locale", "fr", "--all", "--output", str(destination), "--json"]
    completed = subprocess.run(command, capture_output=True, text=True, timeout=120)
    log = logs / "unsafe-literal.log"
    log.write_text("COMMAND: " + repr(command) + "\n" + completed.stdout + completed.stderr)
    report["commands"].append({"label": "unsafe-literal", "exit": completed.returncode, "log_sha256": sha(log)})
    require(completed.returncode == 1 and not completed.stdout, "unsafe literal export must fail")
    require("literal key 'ambiguous|==|plural.one' ends in an Apple variation path" in completed.stderr, "unsafe literal diagnostic missing")
    require(catalog.read_bytes() == before and destination.read_bytes() == b"existing output must survive", "rejected export changed input or output")
    return {"name": "unsafe-literal-no-write", "status": "passed"}


def device_root_substitution(binary, corpus, directory, logs, report):
    fixture = corpus / "positive/device-root-substitution"
    directory = directory / "device-root"
    directory.mkdir()
    catalog = directory / "Localizable.xcstrings"
    tool_catalog = directory / "tool.xcstrings"
    shutil.copyfile(fixture / "source.xcstrings", catalog)
    shutil.copyfile(catalog, tool_catalog)
    project = directory / "Oracle.xcodeproj"
    project.mkdir()
    shutil.copyfile(corpus / "provenance/single-catalog-project.pbxproj", project / "project.pbxproj")
    compiled = directory / "compiled"
    compiled.mkdir()
    run(["xcrun", "xcstringstool", "compile", catalog, "--output-directory", compiled], "device-compile-before", logs, report, directory)
    exported = directory / "tool.xliff"
    adopt(binary, tool_catalog, directory, report)
    response = json.loads(run([binary, "export", tool_catalog, "--locale", "de", "--all", "--original", "Localizable.xcstrings", "--output", exported, "--json"], "device-tool-export", logs, report, directory))
    require(response["exported_count"] == 4 and units(exported) == units(fixture / "exported.xliff"), "device/root-substitution export differs from Apple")
    tree = ET.parse(exported)
    for target in tree.getroot().iter(f"{{{NS}}}target"):
        target.text = (target.text or "") + SUFFIX
    incoming = directory / "edited.xliff"
    incoming.write_bytes(ET.tostring(tree.getroot(), encoding="utf-8", xml_declaration=True).replace(b"\r", b"&#13;"))
    imported = json.loads(run([binary, "import", tool_catalog, "--xliff", incoming, "--source-versions", str(exported) + ".source-versions.json", "--original", "Localizable.xcstrings", "--json"], "device-tool-import", logs, report, directory))
    require(imported["accepted"] == 4 and imported["rejected"] == [] and imported["written"], "device/root-substitution import rejected leaves")
    run(["xcrun", "xcstringstool", "compile", tool_catalog, "--output-directory", compiled], "device-compile-tool", logs, report, directory)
    run(["xcodebuild", "-importLocalizations", "-project", project, "-localizationPath", incoming], "device-xcode-import", logs, report, directory)
    expected = read(tool_catalog)
    expected["version"] = read(fixture / "expected-after-import.xcstrings")["version"]
    require(normalize_references(read(catalog)) == normalize_references(expected), "device/root-substitution whole-catalog mismatch")
    run(["xcrun", "xcstringstool", "compile", catalog, "--output-directory", compiled], "device-compile-after", logs, report, directory)
    reexport = directory / "reexport"
    run(["xcodebuild", "-exportLocalizations", "-project", project, "-localizationPath", reexport, "-exportLanguage", "de"], "device-xcode-reexport", logs, report, directory)
    reexported = reexport / "de.xcloc/Localized Contents/de.xliff"
    require(units(reexported) == units(incoming), "device/root-substitution changed targets did not survive Xcode")
    shutil.copyfile(exported, logs / "device-tool-exported.xliff")
    shutil.copyfile(incoming, logs / "device-edited.xliff")
    shutil.copyfile(reexported, logs / "device-reexported.xliff")
    return {"name": "device-root-substitution", "status": "passed", "keys": 1, "changed_units": 4}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--report", type=Path, default=Path("/tmp/xcstrings-xcode-acceptance.json"))
    args = parser.parse_args()
    binary = args.binary.resolve()
    corpus = Path(__file__).resolve().parents[1] / "tests/fixtures/apple_xcode27"
    report_path = args.report.resolve()
    logs = report_path.with_suffix(".logs")
    logs.mkdir(parents=True, exist_ok=True)
    before = {str(path.relative_to(corpus)): sha(path) for path in corpus.rglob("*") if path.is_file()}
    report = {"status": "failed", "binary_sha256": sha(binary), "checks": {"compile": False, "import": False, "semantic_compare": False, "reexport": False}, "commands": [], "scenarios": []}
    try:
        with tempfile.TemporaryDirectory(prefix="xcstrings-final-xcode-") as temporary:
            directory = Path(temporary)
            report["xcode_version"] = run(["xcodebuild", "-version"], "xcode-version", logs, report, directory).strip()
            report["scenarios"].append(unsafe_literal(binary, corpus, directory, logs, report))
            report["scenarios"].append(matrix(binary, corpus, directory, logs, report))
            report["scenarios"].append(device_root_substitution(binary, corpus, directory, logs, report))
            report["scenarios"].append(native_review(binary, corpus, directory, logs, report, run, units))
        require(sha(binary) == report["binary_sha256"], "binary changed during acceptance")
        report["checks"] = dict.fromkeys(report["checks"], True)
        report["status"] = "passed"
    except Exception as error:
        report["failure"] = str(error)
        raise
    finally:
        after = {str(path.relative_to(corpus)): sha(path) for path in corpus.rglob("*") if path.is_file()}
        report["originals_unchanged"] = before == after
        if before != after:
            report["status"] = "failed"
            report["checks"] = dict.fromkeys(report["checks"], False)
            report["failure"] = "original Apple fixtures changed"
        report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
        require(before == after, "original Apple fixtures changed")
    print(json.dumps({"status": report["status"], "report": str(report_path)}))


if __name__ == "__main__":
    main()
