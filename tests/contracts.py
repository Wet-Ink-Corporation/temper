#!/usr/bin/env python3
"""Check docs/contracts/: every example validates against its schema, and the ok flag agrees.

Each file in docs/contracts/examples/ is named <schema stem>.<case>.json and is checked
against docs/contracts/schemas/<schema stem>.schema.json. A rust.* envelope must also have
`ok` exactly when `findings` is empty (CLI-02 and the check schemas' contract).

The validator covers the JSON Schema keywords the schemas use (type, const, enum, required,
properties, additionalProperties: false, items, allOf, pattern, minimum), so CI needs no
third-party package. `--self-test` checks that it rejects what it should.
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONTRACTS = ROOT / "docs" / "contracts"
SUPPORTED = {
    "$schema", "$id", "title", "description", "type", "const", "enum", "required",
    "properties", "additionalProperties", "items", "allOf", "pattern", "minimum",
}
TYPES = {
    "object": dict, "array": list, "string": str, "boolean": bool, "null": type(None),
}


def type_ok(value: object, name: str) -> bool:
    if name == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if name == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    return isinstance(value, TYPES[name])


def validate(value: object, schema: dict, path: str = "$") -> list[str]:
    unknown = set(schema) - SUPPORTED
    if unknown:
        return [f"{path}: schema uses unsupported keywords {sorted(unknown)}"]
    errors: list[str] = []
    if "type" in schema:
        names = schema["type"] if isinstance(schema["type"], list) else [schema["type"]]
        if not any(type_ok(value, n) for n in names):
            return [f"{path}: expected {'/'.join(names)}, got {type(value).__name__}"]
    if "const" in schema and value != schema["const"]:
        errors.append(f"{path}: expected {schema['const']!r}")
    if "enum" in schema and value not in schema["enum"]:
        errors.append(f"{path}: {value!r} is not one of {schema['enum']}")
    if "pattern" in schema and isinstance(value, str) and not re.search(schema["pattern"], value):
        errors.append(f"{path}: {value!r} does not match {schema['pattern']}")
    if "minimum" in schema and type_ok(value, "number") and value < schema["minimum"]:
        errors.append(f"{path}: {value} is below {schema['minimum']}")
    for sub in schema.get("allOf", []):
        errors += validate(value, sub, path)
    if isinstance(value, dict):
        for key in schema.get("required", []):
            if key not in value:
                errors.append(f"{path}: missing {key!r}")
        props = schema.get("properties", {})
        for key, sub in props.items():
            if key in value:
                errors += validate(value[key], sub, f"{path}.{key}")
        if schema.get("additionalProperties") is False:
            for key in value:
                if key not in props:
                    errors.append(f"{path}: unexpected key {key!r}")
    if isinstance(value, list) and "items" in schema:
        for i, item in enumerate(value):
            errors += validate(item, schema["items"], f"{path}[{i}]")
    return errors


def check_example(path: Path) -> list[str]:
    stem = path.name.split(".")
    stem = ".".join(stem[:-2])
    schema_path = CONTRACTS / "schemas" / f"{stem}.schema.json"
    if not schema_path.is_file():
        return [f"{path.name}: no schema {schema_path.name}"]
    doc = json.loads(path.read_text(encoding="utf-8"))
    errors = validate(doc, json.loads(schema_path.read_text(encoding="utf-8")))
    if stem.startswith("rust.") and isinstance(doc, dict):
        if doc.get("ok") != (doc.get("findings") == []):
            errors.append("$: ok must be true exactly when findings is empty")
    return [f"{path.name}: {e}" for e in errors]


def self_test() -> int:
    schema = {
        "type": "object", "additionalProperties": False, "required": ["a"],
        "properties": {
            "a": {"type": "integer", "minimum": 1},
            "b": {"type": ["string", "null"], "pattern": "^x"},
            "c": {"type": "array", "items": {"enum": ["p", "q"]}},
        },
    }
    cases = [
        ({"a": 1, "b": "xy", "c": ["p"]}, True),
        ({"a": 1, "b": None}, True),
        ({}, False),
        ({"a": 0}, False),
        ({"a": True}, False),
        ({"a": 1, "b": "yx"}, False),
        ({"a": 1, "c": ["r"]}, False),
        ({"a": 1, "d": 2}, False),
    ]
    bad = [doc for doc, want in cases if (not validate(doc, schema)) != want]
    if validate({}, {"oneOf": []}) == []:
        bad.append("unsupported keyword accepted")
    for b in bad:
        print(f"contracts self-test: wrong answer for {b!r}")
    if not bad:
        print("contracts self-test: ok")
    return 1 if bad else 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    examples = sorted((CONTRACTS / "examples").glob("*.json"))
    errors = [e for p in examples for e in check_example(p)]
    stems = {".".join(p.name.split(".")[:-2]) for p in examples}
    for schema in sorted((CONTRACTS / "schemas").glob("*.schema.json")):
        if schema.name.removesuffix(".schema.json") not in stems:
            errors.append(f"{schema.name}: no example")
    for e in errors:
        print(f"contracts: {e}")
    if not errors:
        print(f"contracts: {len(examples)} examples valid against their schemas")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
