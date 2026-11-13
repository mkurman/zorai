use serde_json::json;

/// Return the embedded JSON Schema v1 definition for plugin.json manifests.
/// Per D-01/D-02/D-03: only name, version, schema_version required;
/// additionalProperties: true for forward compatibility.
pub fn plugin_schema_v1() -> serde_json::Value {
    let python_config = json!({
        "type": "object",
        "properties": {
            "run_path": { "type": "string" },
            "source": { "type": "string" },
            "env": {
                "oneOf": [
                    { "type": "string" },
                    { "type": "boolean" }
                ]
            },
            "dependencies": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    });
    let local_run = json!({
        "type": "object",
        "required": ["command"],
        "properties": {
            "command": { "type": "string", "minLength": 1 },
            "run_path": { "type": "string" },
            "source": { "type": "string" },
            "env": {
                "oneOf": [
                    { "type": "string" },
                    { "type": "boolean" }
                ]
            },
            "dependencies": {
                "type": "array",
                "items": { "type": "string" }
            },
            "timeout_secs": { "type": "integer", "minimum": 1, "maximum": 3600 }
        }
    });
    let python_command = json!({
        "type": "object",
        "required": ["command"],
        "properties": {
            "command": { "type": "string", "minLength": 1 },
            "run_path": { "type": "string" },
            "source": { "type": "string" },
            "env": {
                "oneOf": [
                    { "type": "string" },
                    { "type": "boolean" }
                ]
            },
            "dependencies": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    });
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "required": ["name", "version", "schema_version"],
        "properties": {
            "name": {
                "type": "string",
                "pattern": "^[a-z0-9]([a-z0-9._-]*[a-z0-9])?$",
                "minLength": 1,
                "maxLength": 128
            },
            "version": {
                "type": "string",
                "pattern": "^\\d+\\.\\d+\\.\\d+"
            },
            "schema_version": {
                "type": "integer",
                "const": 1
            },
            "description": { "type": "string", "maxLength": 500 },
            "author": { "type": "string", "maxLength": 128 },
            "license": { "type": "string", "maxLength": 64 },
            "zorai_version": { "type": "string" },
            "python": python_config,
            "settings": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "required": ["type", "label"],
                    "properties": {
                        "type": { "type": "string", "enum": ["string", "number", "boolean", "select"] },
                        "label": { "type": "string" },
                        "required": { "type": "boolean", "default": false },
                        "secret": { "type": "boolean", "default": false },
                        "default": {},
                        "options": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "description": { "type": "string" }
                    }
                }
            },
            "api": {
                "type": "object",
                "properties": {
                    "base_url": { "type": "string", "format": "uri" },
                    "endpoints": {
                        "type": "object",
                        "additionalProperties": {
                            "type": "object",
                            "anyOf": [
                                { "required": ["method", "path"] },
                                { "required": ["run"] }
                            ],
                            "properties": {
                                "method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"] },
                                "path": { "type": "string" },
                                "run": local_run,
                                "params": { "type": "object" },
                                "headers": { "type": "object" },
                                "body": { "type": ["object", "string"] },
                                "response_template": { "type": "string" }
                            }
                        }
                    },
                    "rate_limit": {
                        "type": "object",
                        "properties": {
                            "requests_per_minute": { "type": "integer", "minimum": 1 }
                        }
                    }
                }
            },
            "commands": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "required": ["description"],
                    "properties": {
                        "description": { "type": "string" },
                        "action": { "type": "string" },
                        "python": python_command
                    }
                }
            },
            "skills": {
                "type": "array",
                "items": { "type": "string" }
            },
            "auth": {
                "type": "object",
                "properties": {
                    "type": { "type": "string", "enum": ["oauth2", "api_key", "bearer"] },
                    "authorization_url": { "type": "string", "format": "uri" },
                    "token_url": { "type": "string", "format": "uri" },
                    "scopes": { "type": "array", "items": { "type": "string" } },
                    "pkce": { "type": "boolean", "default": false }
                }
            },
            "connector": {
                "type": "object",
                "required": ["kind"],
                "properties": {
                    "kind": { "type": "string", "minLength": 1 },
                    "category": { "type": "string" },
                    "setup_hint": { "type": "string" },
                    "docs_path": { "type": "string" },
                    "readiness_endpoint": { "type": "string" },
                    "workflow_primitives": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "read_actions": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "write_actions": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                }
            }
        },
        "additionalProperties": true
    })
}

/// Compile the embedded schema v1 into a reusable validator.
pub fn compile_schema_v1() -> jsonschema::Validator {
    jsonschema::validator_for(&plugin_schema_v1()).expect("built-in schema must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schema_v1_accepts_minimal_manifest() {
        let validator = compile_schema_v1();
        let manifest = json!({
            "name": "test-plugin",
            "version": "1.0.0",
            "schema_version": 1
        });
        let result = validator.validate(&manifest);
        assert!(result.is_ok(), "minimal manifest should be valid");
    }

    #[test]
    fn schema_v1_rejects_missing_name() {
        let validator = compile_schema_v1();
        let manifest = json!({
            "version": "1.0.0",
            "schema_version": 1
        });
        let result = validator.validate(&manifest);
        assert!(result.is_err(), "manifest without name should be rejected");
    }

    #[test]
    fn schema_v1_rejects_non_integer_schema_version() {
        let validator = compile_schema_v1();
        let manifest = json!({
            "name": "test-plugin",
            "version": "1.0.0",
            "schema_version": "one"
        });
        let result = validator.validate(&manifest);
        assert!(
            result.is_err(),
            "manifest with string schema_version should be rejected"
        );
    }

    #[test]
    fn schema_v1_rejects_invalid_name_pattern() {
        let validator = compile_schema_v1();
        let manifest = json!({
            "name": "../escape",
            "version": "1.0.0",
            "schema_version": 1
        });
        let result = validator.validate(&manifest);
        assert!(
            result.is_err(),
            "manifest with path traversal name should be rejected"
        );
    }

    #[test]
    fn schema_v1_accepts_unknown_fields() {
        let validator = compile_schema_v1();
        let manifest = json!({
            "name": "test-plugin",
            "version": "1.0.0",
            "schema_version": 1,
            "custom_field": "hello",
            "future_feature": { "nested": true }
        });
        let result = validator.validate(&manifest);
        assert!(
            result.is_ok(),
            "manifest with unknown fields should be accepted per D-01"
        );
    }

    #[test]
    fn schema_v1_accepts_python_defaults_and_commands() {
        let validator = compile_schema_v1();
        let manifest = json!({
            "name": "python-plugin",
            "version": "1.0.0",
            "schema_version": 1,
            "python": {
                "run_path": "workspace",
                "source": "https://example.com/tool.py",
                "env": true,
                "dependencies": ["requests>=2.32"]
            },
            "commands": {
                "sync": {
                    "description": "Run sync",
                    "python": {
                        "command": "python sync.py"
                    }
                }
            }
        });
        let result = validator.validate(&manifest);
        assert!(result.is_ok(), "python-backed manifest should be valid");
    }
}
