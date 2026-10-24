//! Parser and syntax model for the initial Swaraj language.
//!
//! A source file contains one package declaration and an indented mapping of
//! fields. Expressions in fields can use bindings declared in a `let:` block.

use std::collections::BTreeMap;

mod expression;

const MAX_NESTING_DEPTH: usize = 128;

/// A one-based source position measured in Unicode scalar values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLocation {
    /// One-based line number.
    pub line: usize,
    /// One-based column number.
    pub column: usize,
}

/// A value paired with the location where it begins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Located<T> {
    /// Parsed value.
    pub value: T,
    /// Position of the value in the source file.
    pub location: SourceLocation,
}

/// A parsed Swaraj value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    /// A string value.
    String(String),
    /// A signed 64-bit integer.
    Integer(i64),
    /// A boolean value.
    Boolean(bool),
    /// A list of values.
    List(Vec<Located<Value>>),
    /// A nested mapping of field names to values.
    Map(BTreeMap<String, Located<Value>>),
}

/// A parsed package declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Package {
    /// Package identifier from `package name:`.
    pub name: String,
    /// Source location of the package identifier.
    pub location: SourceLocation,
    /// Fields declared in the package body.
    pub fields: BTreeMap<String, Located<Value>>,
}

/// A human-readable syntax diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    /// One-based line number.
    pub line: usize,
    /// One-based column number.
    pub column: usize,
    /// Short explanation of the syntax error.
    pub message: String,
    /// Suggested correction.
    pub help: String,
}

impl Diagnostic {
    fn new(
        line: usize,
        column: usize,
        message: impl Into<String>,
        help: impl Into<String>,
    ) -> Self {
        Self {
            line,
            column,
            message: message.into(),
            help: help.into(),
        }
    }

    /// Render this diagnostic with a filename.
    pub fn render(&self, filename: &str) -> String {
        format!(
            "{filename}:{}:{}: error: {}\n  help: {}",
            self.line, self.column, self.message, self.help
        )
    }
}

#[derive(Debug)]
struct SourceLine<'a> {
    number: usize,
    indent: usize,
    text: &'a str,
}

struct Parser<'a> {
    lines: Vec<SourceLine<'a>>,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Result<Self, Diagnostic> {
        let mut lines = Vec::new();

        for (index, raw) in source.lines().enumerate() {
            let number = index + 1;
            let uncommented = strip_comment(raw).trim_end();
            if uncommented.trim().is_empty() {
                continue;
            }
            let indent = uncommented
                .chars()
                .take_while(|character| *character == ' ')
                .count();
            let text = &uncommented[indent..];
            if text.starts_with('\t') {
                return Err(Diagnostic::new(
                    number,
                    indent + 1,
                    "tabs are not allowed for indentation",
                    "indent nested fields with two spaces.",
                ));
            }
            if indent % 2 != 0 {
                return Err(Diagnostic::new(
                    number,
                    1,
                    "indentation must use pairs of spaces",
                    "add or remove one space so each nesting level is two spaces.",
                ));
            }

            lines.push(SourceLine {
                number,
                indent,
                text,
            });
        }

        Ok(Self { lines, position: 0 })
    }

    fn parse(mut self) -> Result<Package, Diagnostic> {
        let mut bindings = BTreeMap::new();
        if self
            .lines
            .first()
            .is_some_and(|line| line.indent == 0 && line.text == "let:")
        {
            self.position = 1;
            bindings = self.parse_bindings()?;
            let Some(in_line) = self.lines.get(self.position) else {
                return Err(Diagnostic::new(
                    1,
                    1,
                    "expected `in:` after the bindings",
                    "add `in:` and then a package declaration.",
                ));
            };
            if in_line.indent != 0 || in_line.text != "in:" {
                return Err(Diagnostic::new(
                    in_line.number,
                    in_line.indent + 1,
                    "expected `in:` after the bindings",
                    "finish the `let:` block with `in:`.",
                ));
            }
            self.position += 1;
        }

        let Some(declaration) = self.lines.get(self.position) else {
            return Err(Diagnostic::new(
                1,
                1,
                "expected a package declaration",
                "start with `package name:`.",
            ));
        };
        let declaration_index = self.position;

        let Some(name) = declaration
            .text
            .strip_prefix("package ")
            .and_then(|rest| rest.strip_suffix(':'))
        else {
            return Err(Diagnostic::new(
                declaration.number,
                declaration.indent + 1,
                "expected a package declaration",
                "write `package name:` as the first non-empty line.",
            ));
        };

        if declaration.indent != 0 {
            return Err(Diagnostic::new(
                declaration.number,
                1,
                "package declaration must start at the beginning of the line",
                "remove indentation before `package`.",
            ));
        }
        if !is_identifier(name) {
            return Err(Diagnostic::new(
                declaration.number,
                "package ".chars().count() + 1,
                "invalid package name",
                "use a name starting with a letter or `_`; letters, digits, `_` and `-` are allowed.",
            ));
        }
        let package_name = name.to_owned();
        let package_location = SourceLocation {
            line: declaration.number,
            column: "package ".chars().count() + 1,
        };

        self.position = declaration_index + 1;
        if self.position == self.lines.len() {
            return Err(Diagnostic::new(
                declaration.number,
                declaration.text.chars().count() + 1,
                "package has no fields",
                "add an indented field, for example `  version: \"1.0\"`.",
            ));
        }
        if self.lines[self.position].indent != 2 {
            let line = &self.lines[self.position];
            return Err(Diagnostic::new(
                line.number,
                line.indent + 1,
                "package fields must be indented by two spaces",
                "indent the first field two spaces below the package declaration.",
            ));
        }

        let fields = self.parse_map(2, &bindings)?;
        if let Some(line) = self.lines.get(self.position) {
            if line.indent == 0 && line.text.starts_with("package ") {
                return Err(Diagnostic::new(
                    line.number,
                    1,
                    "a file can contain only one package declaration",
                    "put each package in its own `.sw` file.",
                ));
            }
            return Err(Diagnostic::new(
                line.number,
                line.indent + 1,
                "unexpected indentation level",
                "nested fields must be indented exactly two spaces more than their parent.",
            ));
        }

        Ok(Package {
            name: package_name,
            location: package_location,
            fields,
        })
    }

    fn parse_bindings(&mut self) -> Result<BTreeMap<String, expression::Expression>, Diagnostic> {
        let mut bindings = BTreeMap::new();
        let mut saw_binding = false;

        while self.position < self.lines.len() {
            let line = &self.lines[self.position];
            if line.indent == 0 && line.text == "in:" {
                break;
            }
            if line.indent != 2 {
                return Err(Diagnostic::new(
                    line.number,
                    line.indent + 1,
                    "bindings must be indented by two spaces",
                    "declare each binding as `  name: expression`.",
                ));
            }
            let line_number = line.number;
            let line_indent = line.indent;
            let line_text = line.text;
            let Some((name, raw_expression)) = line_text.split_once(':') else {
                return Err(Diagnostic::new(
                    line_number,
                    line_indent + 1,
                    "expected a binding in the form `name: expression`",
                    "write a name, a colon, and an expression.",
                ));
            };
            if !is_identifier(name) {
                return Err(Diagnostic::new(
                    line_number,
                    line_indent + 1,
                    "invalid binding name",
                    "binding names may contain letters, digits, `_` and `-`, and must start with a letter or `_`.",
                ));
            }
            if bindings.contains_key(name) {
                return Err(Diagnostic::new(
                    line_number,
                    line_indent + 1,
                    format!("binding `{name}` is declared more than once"),
                    "keep one declaration for this binding or rename one of them.",
                ));
            }

            let (raw_expression, leading_spaces) = trim_start_with_count(raw_expression);
            if raw_expression.is_empty() {
                return Err(Diagnostic::new(
                    line_number,
                    line_text.chars().count() + 1,
                    format!("binding `{name}` needs an expression"),
                    "add a value after the colon, such as `enabled: true`.",
                ));
            }
            let location = SourceLocation {
                line: line_number,
                column: line_indent + name.chars().count() + 2 + leading_spaces,
            };
            let field_indent = line_indent;
            let raw_expression = raw_expression.to_owned();
            self.position += 1;
            let source = self.collect_expression(&raw_expression, field_indent)?;
            let parsed = expression::parse(&source, location)?;
            bindings.insert(name.to_owned(), parsed);
            saw_binding = true;
        }

        if !saw_binding {
            let line = self.lines.get(self.position).map_or(1, |line| line.number);
            return Err(Diagnostic::new(
                line,
                1,
                "the `let:` block is empty",
                "add at least one binding before `in:`.",
            ));
        }
        Ok(bindings)
    }

    fn parse_map(
        &mut self,
        indent: usize,
        bindings: &BTreeMap<String, expression::Expression>,
    ) -> Result<BTreeMap<String, Located<Value>>, Diagnostic> {
        if indent / 2 > MAX_NESTING_DEPTH {
            let line = &self.lines[self.position];
            return Err(Diagnostic::new(
                line.number,
                line.indent + 1,
                format!("mapping nesting exceeds the limit of {MAX_NESTING_DEPTH} levels"),
                "reduce the number of nested mappings.",
            ));
        }

        let mut fields = BTreeMap::new();

        while self.position < self.lines.len() {
            let line = &self.lines[self.position];
            if line.indent < indent {
                break;
            }
            if line.indent > indent {
                return Err(Diagnostic::new(
                    line.number,
                    line.indent + 1,
                    "unexpected indentation",
                    "add a parent field ending in `:` or align this field with its siblings.",
                ));
            }

            let Some((key, raw_value)) = line.text.split_once(':') else {
                return Err(Diagnostic::new(
                    line.number,
                    indent + 1,
                    "expected a field in the form `name: value`",
                    "field names may contain letters, digits, `_` and `-`.",
                ));
            };
            if !is_identifier(key) {
                return Err(Diagnostic::new(
                    line.number,
                    indent + 1,
                    "invalid field name",
                    "field names may contain letters, digits, `_` and `-`, and must start with a letter or `_`.",
                ));
            }
            if fields.contains_key(key) {
                return Err(Diagnostic::new(
                    line.number,
                    indent + 1,
                    format!("field `{key}` is declared more than once"),
                    "keep one declaration for this field or rename one of them.",
                ));
            }

            let line_number = line.number;
            let key_column = indent + 1;
            let colon_column = key_column + key.chars().count();
            let (raw_value, leading_spaces) = trim_start_with_count(raw_value);
            let value_location = SourceLocation {
                line: line_number,
                column: colon_column + 1 + leading_spaces,
            };
            self.position += 1;

            let raw_value = raw_value.to_owned();
            let value = if raw_value.is_empty() {
                let Some(child) = self.lines.get(self.position) else {
                    return Err(Diagnostic::new(
                        line_number,
                        colon_column + 1,
                        format!("field `{key}` needs a value or nested fields"),
                        format!("add a value after `{key}:` or indent child fields by two spaces."),
                    ));
                };
                if child.indent != indent + 2 {
                    return Err(Diagnostic::new(
                        child.number,
                        child.indent + 1,
                        "nested fields must be indented by two spaces",
                        "indent this field exactly two spaces more than its parent.",
                    ));
                }
                Value::Map(self.parse_map(indent + 2, bindings)?)
            } else {
                let source = self.collect_expression(&raw_value, indent)?;
                let parsed = expression::parse(&source, value_location)?;
                expression::evaluate(&parsed, bindings)?
            };

            fields.insert(
                key.to_owned(),
                Located {
                    value,
                    location: value_location,
                },
            );
        }

        Ok(fields)
    }

    fn collect_expression(
        &mut self,
        initial: &str,
        field_indent: usize,
    ) -> Result<String, Diagnostic> {
        let mut source = initial.to_owned();
        let mut depth = 0usize;
        let mut quoted = false;
        let mut escaped = false;
        for character in initial.chars() {
            if escaped {
                escaped = false;
            } else if quoted && character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = !quoted;
            } else if !quoted && character == '[' {
                depth += 1;
            } else if !quoted && character == ']' {
                depth = depth.saturating_sub(1);
            }
        }
        if depth == 0 {
            return Ok(source);
        }

        while depth > 0 {
            let Some(next) = self.lines.get(self.position) else {
                return Ok(source);
            };
            let is_aligned_closing_line = next.indent == field_indent && next.text.starts_with(']');
            if !is_aligned_closing_line && next.indent != field_indent + 2 {
                return Err(Diagnostic::new(
                    next.number,
                    next.indent + 1,
                    "list continuation has unexpected indentation",
                    "indent list items two spaces below the field and align the closing `]` with the field.",
                ));
            }
            source.push('\n');
            source.push_str(&" ".repeat(next.indent));
            source.push_str(next.text);
            for character in next.text.chars() {
                if escaped {
                    escaped = false;
                } else if quoted && character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    quoted = !quoted;
                } else if !quoted && character == '[' {
                    depth += 1;
                } else if !quoted && character == ']' {
                    depth = depth.saturating_sub(1);
                }
            }
            self.position += 1;
        }
        Ok(source)
    }
}

fn strip_comment(text: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;

    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
        } else if quoted && character == '\\' {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if !quoted && character == '#' {
            return &text[..index];
        }
    }
    text
}

fn trim_start_with_count(value: &str) -> (&str, usize) {
    let Some((byte_offset, _)) = value
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
    else {
        return ("", value.chars().count());
    };
    (&value[byte_offset..], value[..byte_offset].chars().count())
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(character) if character.is_ascii_alphabetic() || character == '_')
        && chars.all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

/// Parse one source file as a Swaraj package definition.
///
/// The returned syntax tree preserves source locations for package names,
/// fields, and list entries. A file must contain exactly one package.
///
/// # Examples
///
/// ```
/// let package = swaraj::parse("package demo:\n  version: \"1.0\"\n").unwrap();
/// assert_eq!(package.name, "demo");
/// ```
pub fn parse(source: &str) -> Result<Package, Diagnostic> {
    Parser::new(source)?.parse()
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, Located, SourceLocation, Value, parse};

    fn string_item(value: &str, line: usize, column: usize) -> Located<Value> {
        Located {
            value: Value::String(value.to_owned()),
            location: SourceLocation { line, column },
        }
    }

    #[test]
    fn parses_nested_fields_and_inline_dependency_lists() {
        let package = parse(
            r#"package vim:
  version: "9.1"
  dependencies:
    build: ["gcc", "make", "ncurses"]
    runtime: ["ncurses"]
"#,
        )
        .expect("valid package should parse");

        assert_eq!(package.name, "vim");
        let Value::Map(dependencies) = &package.fields["dependencies"].value else {
            panic!("dependencies should be a mapping");
        };
        assert_eq!(
            dependencies["build"].value,
            Value::List(vec![
                string_item("gcc", 4, 13),
                string_item("make", 4, 20),
                string_item("ncurses", 4, 28),
            ])
        );
    }

    #[test]
    fn parses_multiline_command_lists_and_quoted_commas() {
        let package = parse(
            r#"package demo:
  build:
    commands: [
      "echo a,b",
      "make"
    ]
"#,
        )
        .expect("valid package should parse");

        let Value::Map(build) = &package.fields["build"].value else {
            panic!("build should be a mapping");
        };
        assert_eq!(
            build["commands"].value,
            Value::List(vec![
                string_item("echo a,b", 4, 7),
                string_item("make", 5, 7),
            ])
        );
    }

    #[test]
    fn parses_comments_without_truncating_quoted_hashes() {
        let package = parse(
            r#"   # oddly indented comment-only line
package demo: # package comment
  url: "https://example.test/archive#source" # trailing comment
  dependencies: ["one", "two"] # another comment
  labels: ["alpha#beta", "gamma"]
"#,
        )
        .expect("comments and quoted hashes should parse");

        assert_eq!(
            package.fields["url"].value,
            Value::String("https://example.test/archive#source".to_owned())
        );
        assert_eq!(
            package.fields["labels"].value,
            Value::List(vec![
                string_item("alpha#beta", 5, 12),
                string_item("gamma", 5, 26),
            ])
        );
    }

    #[test]
    fn parses_empty_lists_trailing_commas_and_crlf() {
        let package =
            parse("package demo:\r\n  dependencies: [\"one\", \"two\",]\r\n  optional: []\r\n")
                .expect("CRLF, trailing commas, and empty lists should parse");

        assert_eq!(
            package.fields["dependencies"].value,
            Value::List(vec![string_item("one", 2, 18), string_item("two", 2, 25),])
        );
        assert_eq!(package.fields["optional"].value, Value::List(vec![]));
    }

    #[test]
    fn parses_escaped_strings_and_unicode() {
        let package = parse(
            r#"package demo:
  text: "café says \"hello\"\nline"
"#,
        )
        .expect("escaped strings and Unicode should parse");

        assert_eq!(
            package.fields["text"].value,
            Value::String("café says \"hello\"\nline".to_owned())
        );
    }

    #[test]
    fn reports_duplicate_fields_with_source_location() {
        let error = parse("package demo:\n  version: \"1\"\n  version: \"2\"\n")
            .expect_err("duplicate field should fail");
        assert_eq!(error.line, 3);
        assert_eq!(error.column, 3);
        assert!(error.message.contains("declared more than once"));
    }

    #[test]
    fn reports_unicode_scalar_columns() {
        let error = parse("package demo:\n  title: \"café\" trailing\n")
            .expect_err("text after a string should fail");
        assert_eq!(
            error,
            Diagnostic {
                line: 2,
                column: 17,
                message: "unexpected token after expression".to_owned(),
                help: "remove the extra token or add an operator between values.".to_owned(),
            }
        );
    }

    #[test]
    fn reports_unclosed_list_and_string_separately() {
        let list_error = parse("package demo:\n  dependencies: [\"one\", \"two\"\n")
            .expect_err("unclosed list should fail");
        assert!(list_error.message.contains("missing its closing"));

        let string_error =
            parse("package demo:\n  commands: [\"echo\n").expect_err("unclosed string should fail");
        assert!(string_error.message.contains("quoted string is not closed"));
    }

    #[test]
    fn rejects_empty_list_items() {
        let error = parse("package demo:\n  dependencies: [\"one\",,]\n")
            .expect_err("empty list entries should fail");
        assert!(error.message.contains("expected a value or expression"));
    }

    #[test]
    fn parses_nested_lists() {
        let package = parse("package demo:\n  groups: [[\"one\"], [\"two\"]]\n")
            .expect("nested lists should parse");
        let Value::List(groups) = &package.fields["groups"].value else {
            panic!("groups should be a list");
        };
        assert!(matches!(groups[0].value, Value::List(_)));
    }

    #[test]
    fn rejects_tabs_and_odd_indentation() {
        let tab_error = parse("package demo:\n\tversion: 1\n").expect_err("tabs should fail");
        assert!(tab_error.message.contains("tabs are not allowed"));

        let indent_error =
            parse("package demo:\n   version: 1\n").expect_err("odd indentation should fail");
        assert!(indent_error.message.contains("pairs of spaces"));
    }

    #[test]
    fn rejects_fields_without_values_and_duplicate_root_documents() {
        let missing_value =
            parse("package demo:\n  version:\n").expect_err("empty field should fail");
        assert!(missing_value.message.contains("needs a value"));

        let extra = parse("package demo:\n  version: 1\npackage other:\n  version: 2\n")
            .expect_err("a file may contain only one package");
        assert!(extra.message.contains("only one package declaration"));
    }

    #[test]
    fn permits_same_field_names_in_separate_mappings() {
        let package = parse(
            "package demo:\n\
             \x20\x20source:\n\
            \x20\x20\x20\x20url: \"source-url\"\n\
             \x20\x20homepage:\n\
            \x20\x20\x20\x20url: \"home-url\"\n",
        )
        .expect("field names are only unique within their own mapping");

        let Value::Map(source) = &package.fields["source"].value else {
            panic!("source should be a mapping");
        };
        let Value::Map(homepage) = &package.fields["homepage"].value else {
            panic!("homepage should be a mapping");
        };
        assert_eq!(source["url"].value, Value::String("source-url".to_owned()));
        assert_eq!(homepage["url"].value, Value::String("home-url".to_owned()));
    }

    #[test]
    fn reports_invalid_headers_and_malformed_strings() {
        let header_error = parse("package 1invalid:\n  version: 1\n")
            .expect_err("package names must start with a letter or underscore");
        assert!(header_error.message.contains("invalid package name"));

        let quote_error = parse("package demo:\n  version: \"valid\" trailing\n")
            .expect_err("quoted strings must contain the whole scalar");
        assert!(
            quote_error
                .message
                .contains("unexpected token after expression")
        );

        let escape_error = parse("package demo:\n  version: \"invalid\\x\"\n")
            .expect_err("unsupported string escapes should fail");
        assert!(escape_error.message.contains("unsupported escape"));
    }

    #[test]
    fn requires_list_continuations_at_the_expected_indentation() {
        let error = parse("package demo:\n  dependencies: [\n    one,\n       two\n  ]\n")
            .expect_err("list continuation indentation must be consistent");
        assert!(error.message.contains("indentation"));
    }

    #[test]
    fn reports_unsupported_escapes_at_the_escape() {
        let error = parse(
            r#"package demo:
  value: "bad\q"
"#,
        )
        .expect_err("unknown string escapes should fail");
        assert_eq!(error.line, 2);
        assert_eq!(error.column, 15);
        assert!(error.message.contains("unsupported escape"));
    }

    #[test]
    fn limits_mapping_nesting_with_a_diagnostic() {
        let mut source = String::from("package deep:\n");
        for depth in 1..=super::MAX_NESTING_DEPTH + 1 {
            source.push_str(&" ".repeat(depth * 2));
            if depth == super::MAX_NESTING_DEPTH + 1 {
                source.push_str("value: 1\n");
            } else {
                source.push_str("nested:\n");
            }
        }

        let error = parse(&source).expect_err("excessive nesting should fail safely");
        assert!(error.message.contains("exceeds the limit"));
    }

    #[test]
    fn renders_actionable_diagnostics() {
        let error = parse("package demo:\n  values: [\"one\" \"two\"]\n")
            .expect_err("missing comma should fail");
        assert!(error.message.contains("expected `,` between list items"));
        assert!(error.render("demo.sw").starts_with("demo.sw:2:"));
        assert!(!error.help.is_empty());
    }

    #[test]
    fn allows_a_trailing_comma_but_rejects_empty_list_entries() {
        let package = parse("package demo:\n  dependencies: [\"one\", \"two\",]\n")
            .expect("a trailing comma should be allowed");
        assert!(matches!(
            package.fields["dependencies"].value,
            Value::List(_)
        ));

        let error = parse("package demo:\n  dependencies: [\"one\",,\"two\"]\n")
            .expect_err("empty list entries should fail");
        assert!(error.message.contains("expected a value or expression"));
    }

    #[test]
    fn evaluates_let_bindings_types_operators_lists_and_interpolation() {
        let package = parse(
            r#"let:
  name: "Swaraj"
  version: "${major}.${minor}"
  major: 0
  minor: 1
  enabled: true
  count: 2 + 3 * 4
  features: ["parser", "evaluator"]
  allFeatures: features ++ ["interpolation"]
  ready: enabled && count == 14
  release: if ready then "${name} ${version}" else "disabled"
in:
package swaraj:
  version: release
  enabled: enabled
  count: count
  features: allFeatures
"#,
        )
        .expect("the initial expression features should evaluate");

        assert_eq!(
            package.fields["version"].value,
            Value::String("Swaraj 0.1".to_owned())
        );
        assert_eq!(package.fields["enabled"].value, Value::Boolean(true));
        assert_eq!(package.fields["count"].value, Value::Integer(14));
        assert_eq!(
            package.fields["features"].value,
            Value::List(vec![
                string_item("parser", 8, 14),
                string_item("evaluator", 8, 24),
                string_item("interpolation", 9, 29),
            ])
        );
    }

    #[test]
    fn supports_forward_references_and_does_not_evaluate_unused_bindings() {
        let package = parse(
            r#"let:
  first: second + 1
  second: 41
  unused: missingName
in:
package demo:
  answer: first
"#,
        )
        .expect("forward references and unused bindings should be allowed");
        assert_eq!(package.fields["answer"].value, Value::Integer(42));
    }

    #[test]
    fn short_circuits_boolean_operators_and_conditionals() {
        let package = parse(
            r#"let:
  safeAnd: false && missing
  safeOr: true || missing
  chosen: if true then "yes" else missing
in:
package demo:
  and: safeAnd
  or: safeOr
  chosen: chosen
"#,
        )
        .expect("unselected short-circuit branches should not be evaluated");
        assert_eq!(package.fields["and"].value, Value::Boolean(false));
        assert_eq!(package.fields["or"].value, Value::Boolean(true));
        assert_eq!(
            package.fields["chosen"].value,
            Value::String("yes".to_owned())
        );
    }

    #[test]
    fn equality_ignores_source_locations_inside_lists() {
        let package = parse(
            r#"let:
  equal: ["same"] == ["same"]
in:
package demo:
  equal: equal
"#,
        )
        .expect("equal lists should evaluate");
        assert_eq!(package.fields["equal"].value, Value::Boolean(true));
    }

    #[test]
    fn reports_undefined_variables_cycles_type_errors_and_division_by_zero() {
        let undefined =
            parse("package demo:\n  value: missing\n").expect_err("unknown variables should fail");
        assert!(undefined.message.contains("undefined variable `missing`"));

        let cycle =
            parse("let:\n  first: second\n  second: first\nin:\npackage demo:\n  value: first\n")
                .expect_err("recursive bindings should fail");
        assert!(cycle.message.contains("recursive definition"));

        let type_error = parse("package demo:\n  value: true + 1\n")
            .expect_err("invalid operand types should fail");
        assert!(type_error.message.contains("expects integers"));

        let division =
            parse("package demo:\n  value: 10 / 0\n").expect_err("division by zero should fail");
        assert!(division.message.contains("division by zero"));
    }

    #[test]
    fn permits_trailing_commas_in_binding_and_nested_expression_lists() {
        let package = parse(
            r#"let:
  items: [
    ["one", "two",],
    ["three"],
  ]
in:
package demo:
  items: items
"#,
        )
        .expect("multiline nested lists and trailing commas should parse");
        assert!(matches!(package.fields["items"].value, Value::List(_)));
    }
}
