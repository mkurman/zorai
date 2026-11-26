use tree_sitter::{Language, Node, Parser};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct CodeSymbol {
    pub name: String,
    pub kind: &'static str,
    pub line: usize,
    pub end_line: usize,
    pub column: usize,
    pub container: Option<String>,
    pub calls: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub(crate) struct FileGraph {
    pub language: &'static str,
    pub symbols: Vec<CodeSymbol>,
    pub imports: Vec<String>,
}

fn language_for(path: &str) -> Option<(&'static str, Language)> {
    let extension = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "rs" => ("rust", tree_sitter_rust::language()),
        "ts" | "mts" | "cts" => ("typescript", tree_sitter_typescript::language_typescript()),
        "tsx" | "js" | "jsx" | "mjs" | "cjs" => ("tsx", tree_sitter_typescript::language_tsx()),
        "py" => ("python", tree_sitter_python::language()),
        "go" => ("go", tree_sitter_go::language()),
        "c" | "h" | "cc" | "cpp" | "hpp" => ("c", tree_sitter_c::language()),
        "css" | "scss" => ("css", tree_sitter_css::language()),
        "json" => ("json", tree_sitter_json::language()),
        "html" | "htm" => ("html", tree_sitter_html::language()),
        _ => return None,
    })
}

fn symbol_kind(language: &str, kind: &str) -> Option<&'static str> {
    Some(match (language, kind) {
        ("rust", "function_item") => "function",
        ("rust", "struct_item") => "struct",
        ("rust", "enum_item") => "enum",
        ("rust", "trait_item") => "trait",
        ("rust", "impl_item") => "impl",
        ("rust", "mod_item") => "module",
        ("rust", "const_item" | "static_item") => "constant",
        ("rust", "type_item") => "type",
        ("rust", "macro_definition") => "macro",
        ("typescript" | "tsx", "function_declaration" | "generator_function_declaration") => {
            "function"
        }
        ("typescript" | "tsx", "class_declaration" | "abstract_class_declaration") => "class",
        ("typescript" | "tsx", "method_definition") => "method",
        ("typescript" | "tsx", "interface_declaration") => "interface",
        ("typescript" | "tsx", "type_alias_declaration") => "type",
        ("typescript" | "tsx", "enum_declaration") => "enum",
        ("typescript" | "tsx", "variable_declarator") => "variable",
        ("python", "function_definition") => "function",
        ("python", "class_definition") => "class",
        ("go", "function_declaration") => "function",
        ("go", "method_declaration") => "method",
        ("go", "type_spec") => "type",
        ("c", "function_definition") => "function",
        ("c", "struct_specifier") => "struct",
        ("css", "rule_set") => "rule",
        ("json", "pair") => "property",
        _ => return None,
    })
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}

fn symbol_name(language: &str, node: Node, source: &str) -> Option<String> {
    let named = |field: &str| {
        node.child_by_field_name(field)
            .map(|child| text(child, source).to_string())
    };
    match (language, node.kind()) {
        ("rust", "impl_item") => {
            let target = named("type")?;
            Some(match named("trait") {
                Some(name) => format!("impl {name} for {target}"),
                None => format!("impl {target}"),
            })
        }
        ("typescript" | "tsx", "variable_declarator") => {
            let value = node.child_by_field_name("value")?;
            matches!(
                value.kind(),
                "arrow_function" | "function_expression" | "function" | "class"
            )
            .then(|| named("name"))
            .flatten()
        }
        ("c", "function_definition") => {
            let mut declarator = node.child_by_field_name("declarator")?;
            while let Some(inner) = declarator.child_by_field_name("declarator") {
                declarator = inner;
            }
            Some(text(declarator, source).to_string())
        }
        ("css", "rule_set") => node.named_child(0).map(|selectors| {
            text(selectors, source)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        }),
        ("json", "pair") => (node
            .parent()
            .and_then(|parent| parent.parent())
            .is_some_and(|grand| grand.kind() == "document"))
        .then(|| named("key").map(|key| key.trim_matches('"').to_string()))
        .flatten(),
        _ => named("name"),
    }
}

fn is_import(language: &str, kind: &str) -> bool {
    matches!(
        (language, kind),
        ("rust", "use_declaration")
            | ("typescript" | "tsx", "import_statement")
            | ("python", "import_statement" | "import_from_statement")
            | ("go", "import_spec")
            | ("c", "preproc_include")
    )
}

fn call_name(language: &str, node: Node, source: &str) -> Option<String> {
    let callee = match (language, node.kind()) {
        ("rust" | "typescript" | "tsx" | "go" | "c", "call_expression") => {
            node.child_by_field_name("function")?
        }
        ("python", "call") => node.child_by_field_name("function")?,
        _ => return None,
    };
    let name = text(callee, source);
    let last = name.rsplit(['.', ':']).next().unwrap_or(name).trim();
    (!last.is_empty()
        && last.len() <= 80
        && last
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_'))
    .then(|| last.to_string())
}

pub(crate) fn file_graph(path: &str, source: &str) -> Option<FileGraph> {
    let (language, grammar) = language_for(path)?;
    let mut parser = Parser::new();
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(source, None)?;
    let mut graph = FileGraph {
        language,
        ..Default::default()
    };
    let mut stack: Vec<(Node, Option<usize>)> = vec![(tree.root_node(), None)];
    while let Some((node, owner)) = stack.pop() {
        let mut next_owner = owner;
        if is_import(language, node.kind()) {
            graph.imports.push(
                text(node, source)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        } else if let Some(call) = call_name(language, node, source) {
            if let Some(index) = owner {
                if !graph.symbols[index].calls.contains(&call) {
                    graph.symbols[index].calls.push(call);
                }
            }
        }
        if let Some(kind) = symbol_kind(language, node.kind()) {
            if let Some(name) = symbol_name(language, node, source).filter(|name| !name.is_empty())
            {
                let start = node.start_position();
                graph.symbols.push(CodeSymbol {
                    name: name.chars().take(200).collect(),
                    kind,
                    line: start.row + 1,
                    end_line: node.end_position().row + 1,
                    column: start.column + 1,
                    container: owner.map(|index| graph.symbols[index].name.clone()),
                    calls: Vec::new(),
                });
                next_owner = Some(graph.symbols.len() - 1);
            }
        }
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push((child, next_owner));
        }
    }
    graph.symbols.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.column.cmp(&right.column))
    });
    Some(graph)
}

pub(crate) fn top_level_chunks(graph: &FileGraph) -> Vec<&CodeSymbol> {
    graph
        .symbols
        .iter()
        .filter(|symbol| symbol.container.is_none() || symbol.kind == "method")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_definitions_carry_their_impl_container_and_the_functions_they_call() {
        let source = "use std::fmt;\nstruct Parser;\nimpl Parser {\n    fn parse(&self) { self.lex(); helper(); }\n    fn lex(&self) {}\n}\nfn helper() {}\n";
        let graph = file_graph("src/parser.rs", source).unwrap();
        let summary: Vec<_> = graph
            .symbols
            .iter()
            .map(|symbol| {
                (
                    symbol.name.as_str(),
                    symbol.kind,
                    symbol.line,
                    symbol.container.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Parser", "struct", 2, None),
                ("impl Parser", "impl", 3, None),
                ("parse", "function", 4, Some("impl Parser")),
                ("lex", "function", 5, Some("impl Parser")),
                ("helper", "function", 7, None),
            ]
        );
        assert_eq!(graph.symbols[2].calls, vec!["lex", "helper"]);
        assert_eq!(graph.imports, vec!["use std::fmt;"]);
    }

    #[test]
    fn typescript_components_classes_and_methods_are_found_without_a_language_server() {
        let source = "import { a } from './a';\nexport const Panel = () => <div />;\nexport class Store {\n  load() { return fetchAll(); }\n}\nconst count = 3;\n";
        let graph = file_graph("src/Panel.tsx", source).unwrap();
        let names: Vec<_> = graph
            .symbols
            .iter()
            .map(|symbol| (symbol.name.as_str(), symbol.kind))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Panel", "variable"),
                ("Store", "class"),
                ("load", "method")
            ]
        );
        assert_eq!(graph.symbols[2].calls, vec!["fetchAll"]);
        assert_eq!(graph.imports.len(), 1);
    }

    #[test]
    fn python_go_and_css_symbols_and_unknown_files() {
        let python = file_graph("a.py", "class A:\n    def run(self):\n        pass\n").unwrap();
        assert_eq!(
            python
                .symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "run"]
        );
        let go = file_graph(
            "a.go",
            "package a\ntype T struct{}\nfunc (t T) Run() {}\nfunc main() { Run() }\n",
        )
        .unwrap();
        assert_eq!(
            go.symbols
                .iter()
                .map(|symbol| (symbol.name.as_str(), symbol.kind))
                .collect::<Vec<_>>(),
            vec![("T", "type"), ("Run", "method"), ("main", "function")]
        );
        let css = file_graph("a.css", ".card > .title { color: red; }\n").unwrap();
        assert_eq!(css.symbols[0].name, ".card > .title");
        assert!(file_graph("notes.txt", "hello").is_none());
        assert_eq!(top_level_chunks(&python).len(), 1);
    }
}
