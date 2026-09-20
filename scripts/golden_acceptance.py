#!/usr/bin/env python3
"""Exercise the supplied final binary over MCP using disposable fixture copies.

Usage: python3 scripts/golden_acceptance.py --binary target/release/xcstrings-mcp
No build, network, third-party Python dependencies, or writes to original fixtures.
The JSON report separates final-binary fixture acceptance from actual Xcode evidence.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True

from golden_acceptance.workflow import workflow_scenarios
from golden_acceptance.scenarios import catalog_scenarios, mutation_scenarios
from golden_acceptance.formats import legacy_scenarios, merge_scenarios, xliff_scenarios
from golden_acceptance.apple_shared_substitution import apple_shared_substitution
from golden_acceptance.apple_native import apple_inventory, apple_native_reads, apple_native_paths, apple_delimiter_native, apple_legacy_orphan_rejected
from golden_acceptance.apple_xliff import (apple_exports, apple_states, apple_scopes,
                                         apple_import_shapes, apple_matrix_import, apple_unsafe_exports,
                                         apple_atomic_errors, apple_partial_substitution)


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def equal(actual, expected, label="equality"):
    require(actual == expected, f"{label}: expected {expected!r}, got {actual!r}")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8-sig"))


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def validate_schema(value, schema, root, location="arguments"):
    """Validate the JSON Schema vocabulary emitted by this server, fail on unknown constraints."""
    supported = {"$schema", "$id", "$ref", "$defs", "title", "description", "default", "examples",
                 "deprecated", "type", "properties", "required", "additionalProperties", "items",
                 "anyOf", "oneOf", "allOf", "enum", "const", "minimum", "maximum", "format",
                 "minItems", "maxItems", "minLength", "maxLength"}
    require(set(schema) <= supported, f"unsupported live schema constraints: {set(schema) - supported}")
    if "$ref" in schema:
        require(schema["$ref"].startswith("#/$defs/"), "only local schema references supported")
        validate_schema(value, root["$defs"][schema["$ref"].split("/")[-1]], root, location)
    for keyword in ("anyOf", "oneOf", "allOf"):
        if keyword in schema:
            matches = 0
            for alternative in schema[keyword]:
                try:
                    validate_schema(value, alternative, root, location)
                    matches += 1
                except AssertionError:
                    pass
            expected = len(schema[keyword]) if keyword == "allOf" else 1
            require(matches >= expected if keyword == "anyOf" else matches == expected, f"{location}: {keyword} mismatch")
    kinds = schema.get("type", [])
    kinds = [kinds] if isinstance(kinds, str) else kinds
    types = {"null": value is None, "boolean": type(value) is bool, "integer": type(value) is int,
             "number": type(value) in (int, float), "string": isinstance(value, str),
             "array": isinstance(value, list), "object": isinstance(value, dict)}
    require(not kinds or any(types[kind] for kind in kinds), f"{location}: expected {kinds}")
    if "enum" in schema:
        require(value in schema["enum"], f"{location}: enum mismatch")
    if "const" in schema:
        equal(value, schema["const"], f"{location}: const")
    for keyword, passes in (("minimum", lambda bound: value >= bound), ("maximum", lambda bound: value <= bound)):
        if keyword in schema:
            require(passes(schema[keyword]), f"{location}: {keyword}")
    for keyword, passes in (("minItems", lambda bound: len(value) >= bound), ("maxItems", lambda bound: len(value) <= bound),
                            ("minLength", lambda bound: len(value) >= bound), ("maxLength", lambda bound: len(value) <= bound)):
        if keyword in schema:
            require(passes(schema[keyword]), f"{location}: {keyword}")
    if isinstance(value, dict):
        require(set(schema.get("required", [])) <= value.keys(), f"{location}: required property missing")
        for key, child in value.items():
            child_schema = schema.get("properties", {}).get(key, schema.get("additionalProperties", True))
            require(child_schema is not False, f"{location}: unrecognized property {key}")
            if isinstance(child_schema, dict):
                validate_schema(child, child_schema, root, f"{location}.{key}")
    if isinstance(value, list) and "items" in schema:
        for index, child in enumerate(value):
            validate_schema(child, schema["items"], root, f"{location}[{index}]")


class Mcp:
    def __init__(self, binary, temporary, report):
        self.sequence = 0
        self.report = report
        self.exercised = set()
        self.stderr = tempfile.TemporaryFile()
        self.process = subprocess.Popen(
            [str(binary), "--glossary-path", str(temporary / "glossary.json")],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr,
            cwd=temporary, env={**os.environ, "RUST_LOG": "off"},
        )
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        initialized = self.request("initialize", {
            "protocolVersion": "2026-07-28", "capabilities": {},
            "clientInfo": {"name": "golden-acceptance", "version": "1"},
        })
        require("tools" in initialized["capabilities"], "initialize must advertise tools")
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        tools = self.request("tools/list", {})["tools"]
        self.schemas = {tool["name"]: tool["inputSchema"] for tool in tools}
        equal(len(self.schemas), len(tools), "unique advertised tool names")
        self.report["advertised_tools"] = sorted(self.schemas)
        self.report["server_info"] = initialized["serverInfo"]

    def send(self, frame):
        self.process.stdin.write((json.dumps(frame, ensure_ascii=False) + "\n").encode())
        self.process.stdin.flush()

    def request(self, method, params):
        self.sequence += 1
        self.send({"jsonrpc": "2.0", "id": self.sequence, "method": method, "params": params})
        while True:
            require(self.selector.select(30), f"timeout waiting for {method}")
            line = self.process.stdout.readline()
            require(line, f"server closed stdout while waiting for {method}")
            response = json.loads(line)  # Every stdout line must be a JSON-RPC frame.
            equal(response.get("jsonrpc"), "2.0", "stdout JSON-RPC version")
            if "id" not in response:
                require("method" in response, "malformed server notification")
                continue
            equal(response["id"], self.sequence, "response request ID")
            require("error" not in response, f"JSON-RPC error: {response.get('error')}")
            return response["result"]

    def call(self, name, arguments, case, error=None):
        require(name in self.schemas, f"unadvertised tool: {name}")
        validate_schema(arguments, self.schemas[name], self.schemas[name])
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        self.exercised.add(name)
        self.report["calls"].append({"tool": name, "case": case, "error_expected": error is not None})
        if error is not None:
            require(result.get("isError") is True, f"{name} must reject {case}: {result}")
            text = "\n".join(item.get("text", "") for item in result.get("content", []))
            require(error in text, f"{name}: expected error containing {error!r}, got {text!r}")
            return text
        require(not result.get("isError", False), f"{name}/{case}: {result}")
        if "structuredContent" in result:
            return result["structuredContent"]
        content = result["content"]
        equal(len(content), 1, f"{name}: one JSON result block")
        equal(content[0]["type"], "text", f"{name}: text result block")
        return json.loads(content[0]["text"])

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
            raise AssertionError("MCP server did not exit after stdin closed")
        remaining = self.process.stdout.read()
        self.selector.close()
        self.process.stdout.close()
        self.stderr.seek(0)
        stderr = self.stderr.read()
        self.stderr.close()
        equal(self.process.returncode, 0, "MCP exit code")
        equal(remaining, b"", "no unsolicited stdout after final response")
        equal(stderr, b"", "MCP stderr with tracing disabled")


class Harness:
    equal = staticmethod(equal)
    require = staticmethod(require)
    read = staticmethod(read_json)
    write = staticmethod(write_json)

    def __init__(self, client, fixtures, temporary, report):
        self.client = client
        self.fixtures = fixtures
        self.temporary = temporary
        self.report = report
        self.used = set()
        self.sources = {}

    def fixture(self, relative):
        path = self.fixtures / relative
        require(path.is_file(), f"missing fixture {relative}")
        self.used.add(relative)
        return path

    def copy(self, relative, name=None):
        source = self.fixture(relative)
        destination = self.temporary / (name or relative)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
        return destination

    def call(self, name, arguments, case, error=None):
        return self.client.call(name, arguments, case, error)

    def checkpoint(self, path, mode="adopt_existing"):
        """Explicit setup/review decision; never infer adoption during a write."""
        before = path.read_bytes()
        preview = self.on("sync_source_changes", path, "source checkpoint preview", mode=mode, dry_run=True)
        equal(path.read_bytes(), before, "checkpoint preview preserves catalog bytes")
        result = self.on("sync_source_changes", path, "explicit source checkpoint", mode=mode,
                         dry_run=False, expected=preview["input_revisions"])
        equal((result["retry_required"], result["phase_error"]), (False, None), "checkpoint completed")
        if mode == "adopt_existing":
            equal(path.read_bytes(), before, "explicit adoption preserves native states")
        return result

    def capture_sources(self, path, keys=None):
        """Capture before authoring targets; callers retain this exact manifest."""
        keys = self.read(path)["strings"] if keys is None else keys
        captured = {key: self.on("get_key", path, "capture source before translation", key=key)["source_version"]
                    for key in keys}
        self.sources[str(path)] = captured
        return dict(captured)

    def prepare(self, path, keys=None):
        self.checkpoint(path)
        return self.capture_sources(path, keys)

    def on(self, name, path, case, error=None, **arguments):
        held = self.sources.get(str(path))
        if name == "submit_translations":
            requests = []
            for request in arguments["translations"]:
                if "expected_source_version" not in request:
                    require(held is not None and request["key"] in held,
                            case + ": capture source before authoring this translation")
                    request = {**request, "expected_source_version": held[request["key"]]}
                requests.append(request)
            arguments["translations"] = requests
        elif name == "import_xliff" and "expected_source_versions" not in arguments:
            require(held is not None, case + ": capture the source manifest before translating XML")
            arguments["expected_source_versions"] = dict(held)
        result = self.call(name, {"file_path": str(path), **arguments}, case, error)
        if name == "export_xliff" and error is None:
            self.sources[str(path)] = dict(result["source_versions"])
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--fixtures", type=Path, default=Path(__file__).resolve().parents[1] / "tests/fixtures")
    parser.add_argument("--report", type=Path, default=Path("/tmp/xcstrings-golden-acceptance.json"))
    parser.add_argument("--require-complete", action="store_true", help="Fail while any feature acceptance gate is pending")
    parser.add_argument("--xcode-report", type=Path, help="Actual final-binary Xcode oracle report; hash must match --binary")
    args = parser.parse_args()
    binary, fixtures = args.binary.resolve(), args.fixtures.resolve()
    require(binary.is_file(), f"binary does not exist: {binary}")
    originals = {str(p.relative_to(fixtures)): digest(p) for p in fixtures.rglob("*") if p.is_file()}
    report = {"binary": str(binary), "binary_sha256": digest(binary), "calls": [],
              "fixture_sha256": originals, "status": "failed", "scope": "original-and-Apple-fixtures",
              "pending": ["Actual final-binary Xcode compile/import/semantic comparison/reexport"]}
    try:
        if args.xcode_report:
            oracle = read_json(args.xcode_report)
            equal(oracle["binary_sha256"], report["binary_sha256"], "Xcode evidence belongs to this exact binary")
            equal(oracle["status"], "passed", "external Xcode result")
            require(oracle.get("originals_unchanged") is True, "external Xcode run must preserve original fixtures")
            for check in ("compile", "import", "semantic_compare", "reexport"):
                require(oracle["checks"].get(check) is True, "missing actual Xcode check: " + check)
            require(isinstance(oracle["scenarios"], list) and len(oracle["scenarios"]) > 0, "Xcode scenarios required")
            require(isinstance(oracle["xcode_version"], str) and oracle["xcode_version"].strip(), "Xcode version required")
            report["xcode_oracle"] = {"report": str(args.xcode_report.resolve()), "sha256": digest(args.xcode_report), "result": oracle}
            report["pending"] = []
        with tempfile.TemporaryDirectory(prefix="xcstrings-golden-acceptance-") as directory:
            temporary = Path(directory)
            client = Mcp(binary, temporary, report)
            h = Harness(client, fixtures, temporary, report)
            try:
                failures = []
                for scenario in (catalog_scenarios, mutation_scenarios, legacy_scenarios, xliff_scenarios, merge_scenarios,
                                 apple_inventory, apple_native_reads, apple_native_paths, apple_delimiter_native, apple_legacy_orphan_rejected, apple_exports,
                                 apple_states, apple_scopes, apple_import_shapes, apple_matrix_import,
                                 apple_unsafe_exports, apple_atomic_errors, apple_partial_substitution, apple_shared_substitution, workflow_scenarios):
                    try:
                        scenario(h)
                    except Exception as error:
                        failures.append({"scenario": scenario.__name__, "failure": str(error)})
                report["scenario_failures"] = failures
                report["exercised_tools"] = sorted(client.exercised)
                report["used_fixtures"] = sorted(h.used)
                equal(client.exercised, set(client.schemas), "ALL advertised MCP tools exercised")
                baseline = {name for name in originals if "/" not in name or name.startswith(("en.lproj/", "es.lproj/"))}
                equal(h.used - {name for name in h.used if name.startswith("apple_xcode27/")}, baseline,
                      "all original fixtures assigned meaningful scenarios")
                for relative, role in report.get("apple_artifacts", {}).items():
                    if "apple_xcode27/" + relative in h.used and role["verification"] == "provenance hash":
                        role["verification"] = "MCP scenario input or independent expected-output oracle"
                require(not failures, f"scenario failures: {failures}")
                if args.require_complete:
                    require(not report["pending"], f"incomplete feature acceptance: {report['pending']}")
                report["status"] = "passed"
            finally:
                client.close()
    except Exception as error:
        report["status"] = "failed"
        report["failure"] = str(error)
        raise
    finally:
        after = {str(p.relative_to(fixtures)): digest(p) for p in fixtures.rglob("*") if p.is_file()}
        report["originals_unchanged"] = originals == after
        if originals != after:
            report["status"] = "failed"
            report["failure"] = "original fixture bytes or inventory changed"
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
        equal(after, originals, "original fixture bytes and inventory unchanged")
    print(json.dumps({"status": report["status"], "tools": len(client.exercised),
                      "calls": len(report["calls"]), "report": str(args.report), "pending": report["pending"]}))


if __name__ == "__main__":
    main()
