//! Parser and syntax model for the initial Swaraj language.
//!
//! A source file contains one package declaration and an indented mapping of
//! fields. See the repository README for the language's initial syntax.

use std::collections::BTreeMap;

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
    /// A quoted string or an unquoted atom. The initial language treats both
    /// as strings; quoting is required when a value contains punctuation or
    /// whitespace that is not allowed in an atom.
    String(String),
    /// A list of scalar strings.
    List(Vec<Located<String>>),
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

#[derive(Clone, Copy, Debug)]
struct LocatedChar {
    value: char,
    location: SourceLocation,
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
        let Some(declaration) = self.lines.first() else {
            return Err(Diagnostic::new(
                1,
                1,
                "expected a package declaration",
                "start with `package name:`.",
            ));
        };

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

        self.position = 1;
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

        let fields = self.parse_map(2)?;
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

    fn parse_map(&mut self, indent: usize) -> Result<BTreeMap<String, Located<Value>>, Diagnostic> {
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

        while let Some(line) = self.lines.get(self.position) {
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
                Value::Map(self.parse_map(indent + 2)?)
            } else if raw_value.starts_with('[') {
                Value::List(self.parse_list(raw_value, value_location, indent)?)
            } else {
                Value::String(parse_scalar(raw_value, value_location)?)
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

    fn parse_list(
        &mut self,
        first_part: &str,
        value_location: SourceLocation,
        field_indent: usize,
    ) -> Result<Vec<Located<String>>, Diagnostic> {
        let mut chars = Vec::new();
        let mut first_part_chars = first_part.chars();
        let _opening_bracket = first_part_chars.next();
        let contents_column = value_location.column + 1;
        chars.extend(
            first_part_chars
                .enumerate()
                .map(|(offset, value)| LocatedChar {
                    value,
                    location: SourceLocation {
                        line: value_location.line,
                        column: contents_column + offset,
                    },
                }),
        );

        loop {
            match find_list_end(&chars) {
                ListEnd::Found(end) => {
                    if chars[end + 1..]
                        .iter()
                        .any(|character| !character.value.is_whitespace())
                    {
                        let trailing = chars[end + 1..]
                            .iter()
                            .find(|character| !character.value.is_whitespace())
                            .expect("a non-whitespace character was found");
                        return Err(Diagnostic::new(
                            trailing.location.line,
                            trailing.location.column,
                            "unexpected text after list",
                            "put the list value at the end of the field.",
                        ));
                    }
                    return parse_list_items(&chars[..end]);
                }
                ListEnd::UnclosedString(location) => {
                    return Err(Diagnostic::new(
                        location.line,
                        location.column,
                        "quoted string is not closed",
                        "close the string with `\"` before the list's closing `]`.",
                    ));
                }
                ListEnd::NestedList(location) => {
                    return Err(Diagnostic::new(
                        location.line,
                        location.column,
                        "nested lists are not supported",
                        "keep each list item scalar, such as `[gcc, make]`.",
                    ));
                }
                ListEnd::NotFound => {}
            }

            let Some(next) = self.lines.get(self.position) else {
                let location = chars
                    .last()
                    .map_or(value_location, |character| SourceLocation {
                        line: character.location.line,
                        column: character.location.column + 1,
                    });
                return Err(Diagnostic::new(
                    location.line,
                    location.column,
                    "list is missing its closing `]`",
                    "close the list, for example `[rust, cargo]`.",
                ));
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

            let next_column = next.indent + 1;
            chars.push(LocatedChar {
                value: ' ',
                location: SourceLocation {
                    line: next.number,
                    column: next_column,
                },
            });
            chars.extend(
                next.text
                    .chars()
                    .enumerate()
                    .map(|(offset, value)| LocatedChar {
                        value,
                        location: SourceLocation {
                            line: next.number,
                            column: next_column + offset,
                        },
                    }),
            );
            self.position += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ListEnd {
    Found(usize),
    UnclosedString(SourceLocation),
    NestedList(SourceLocation),
    NotFound,
}

fn find_list_end(chars: &[LocatedChar]) -> ListEnd {
    let mut quoted = None;
    let mut escaped = false;

    for (index, character) in chars.iter().enumerate() {
        if escaped {
            escaped = false;
        } else if quoted.is_some() && character.value == '\\' {
            escaped = true;
        } else if character.value == '"' {
            if quoted.is_some() {
                quoted = None;
            } else {
                quoted = Some(character.location);
            }
        } else if quoted.is_none() && character.value == '[' {
            return ListEnd::NestedList(character.location);
        } else if quoted.is_none() && character.value == ']' {
            return ListEnd::Found(index);
        }
    }

    quoted.map_or(ListEnd::NotFound, ListEnd::UnclosedString)
}

fn parse_list_items(chars: &[LocatedChar]) -> Result<Vec<Located<String>>, Diagnostic> {
    let mut items = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;

    for (index, character) in chars.iter().enumerate() {
        if escaped {
            escaped = false;
        } else if quoted && character.value == '\\' {
            escaped = true;
        } else if character.value == '"' {
            quoted = !quoted;
        } else if !quoted && character.value == ',' {
            push_list_item(&mut items, &chars[start..index], Some(character.location))?;
            start = index + 1;
        }
    }

    push_list_item(&mut items, &chars[start..], None)?;
    Ok(items)
}

fn push_list_item(
    items: &mut Vec<Located<String>>,
    chars: &[LocatedChar],
    comma: Option<SourceLocation>,
) -> Result<(), Diagnostic> {
    let start = chars
        .iter()
        .position(|character| !character.value.is_whitespace());
    let Some(start) = start else {
        if let Some(location) = comma {
            return Err(Diagnostic::new(
                location.line,
                location.column,
                "expected a list item",
                "add an item before this comma.",
            ));
        }
        return Ok(());
    };

    let end = chars
        .iter()
        .rposition(|character| !character.value.is_whitespace())
        .expect("a non-whitespace character was found")
        + 1;
    let item_chars = &chars[start..end];
    let value = item_chars
        .iter()
        .map(|character| character.value)
        .collect::<String>();
    let location = item_chars[0].location;
    let parsed = parse_scalar(&value, location)?;
    items.push(Located {
        value: parsed,
        location,
    });
    Ok(())
}

fn parse_scalar(value: &str, location: SourceLocation) -> Result<String, Diagnostic> {
    if value.starts_with('"') {
        let mut result = String::new();
        let mut chars = value.chars().enumerate();
        let _opening_quote = chars.next();

        while let Some((offset, character)) = chars.next() {
            match character {
                '"' => {
                    if let Some((trailing_offset, _)) =
                        chars.find(|(_, trailing)| !trailing.is_whitespace())
                    {
                        return Err(Diagnostic::new(
                            location.line,
                            location.column + trailing_offset,
                            "unexpected text after quoted string",
                            "put the entire value inside one pair of double quotes.",
                        ));
                    }
                    return Ok(result);
                }
                '\\' => {
                    let Some((escape_offset, escaped)) = chars.next() else {
                        return Err(Diagnostic::new(
                            location.line,
                            location.column + offset,
                            "incomplete escape sequence",
                            "use `\\\\`, `\\\"`, `\\n`, `\\r` or `\\t`.",
                        ));
                    };
                    match escaped {
                        '\\' => result.push('\\'),
                        '"' => result.push('"'),
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        _ => {
                            return Err(Diagnostic::new(
                                location.line,
                                location.column + escape_offset,
                                format!("unsupported escape sequence `\\{escaped}`"),
                                "use `\\\\`, `\\\"`, `\\n`, `\\r` or `\\t`.",
                            ));
                        }
                    }
                }
                character => result.push(character),
            }
        }

        return Err(Diagnostic::new(
            location.line,
            location.column,
            "quoted string is not closed",
            "close the string with `\"` on the same line.",
        ));
    }

    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_./:+-".contains(character))
    {
        return Ok(value.to_owned());
    }

    let invalid_offset = value
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || "_./:+-".contains(*character))
        .count();
    Err(Diagnostic::new(
        location.line,
        location.column + invalid_offset,
        format!("cannot read `{value}` as a value"),
        "use one unquoted atom, a double-quoted string, or a list such as `[rust, cargo]`.",
    ))
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
    use super::{Diagnostic, Value, parse};

    #[test]
    fn parses_nested_fields_and_inline_dependency_lists() {
        let package = parse(
            r#"package vim:
  version: "9.1"
  dependencies:
    build: [gcc, make, ncurses]
    runtime: [ncurses]
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
                super::Located {
                    value: "gcc".to_owned(),
                    location: super::SourceLocation {
                        line: 4,
                        column: 13
                    },
                },
                super::Located {
                    value: "make".to_owned(),
                    location: super::SourceLocation {
                        line: 4,
                        column: 18
                    },
                },
                super::Located {
                    value: "ncurses".to_owned(),
                    location: super::SourceLocation {
                        line: 4,
                        column: 24
                    },
                },
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
                super::Located {
                    value: "echo a,b".to_owned(),
                    location: super::SourceLocation { line: 4, column: 7 },
                },
                super::Located {
                    value: "make".to_owned(),
                    location: super::SourceLocation { line: 5, column: 7 },
                },
            ])
        );
    }

    #[test]
    fn parses_comments_without_truncating_quoted_hashes() {
        let package = parse(
            r#"   # oddly indented comment-only line
package demo: # package comment
  url: "https://example.test/archive#source" # trailing comment
  dependencies: [one, two] # another comment
  labels: ["alpha#beta", gamma]
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
                super::Located {
                    value: "alpha#beta".to_owned(),
                    location: super::SourceLocation {
                        line: 5,
                        column: 12
                    },
                },
                super::Located {
                    value: "gamma".to_owned(),
                    location: super::SourceLocation {
                        line: 5,
                        column: 26
                    },
                },
            ])
        );
    }

    #[test]
    fn parses_empty_lists_trailing_commas_and_crlf() {
        let package = parse("package demo:\r\n  dependencies: [one, two,]\r\n  optional: []\r\n")
            .expect("CRLF, trailing commas, and empty lists should parse");

        assert_eq!(
            package.fields["dependencies"].value,
            Value::List(vec![
                super::Located {
                    value: "one".to_owned(),
                    location: super::SourceLocation {
                        line: 2,
                        column: 18
                    },
                },
                super::Located {
                    value: "two".to_owned(),
                    location: super::SourceLocation {
                        line: 2,
                        column: 23
                    },
                },
            ])
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
                message: "unexpected text after quoted string".to_owned(),
                help: "put the entire value inside one pair of double quotes.".to_owned(),
            }
        );
    }

    #[test]
    fn reports_unclosed_list_and_string_separately() {
        let list_error = parse("package demo:\n  dependencies: [one, two\n")
            .expect_err("unclosed list should fail");
        assert!(list_error.message.contains("missing its closing"));

        let string_error =
            parse("package demo:\n  commands: [\"echo\n").expect_err("unclosed string should fail");
        assert!(string_error.message.contains("quoted string is not closed"));
    }

    #[test]
    fn rejects_trailing_list_comma_without_an_item() {
        let error = parse("package demo:\n  dependencies: [one,,]\n")
            .expect_err("empty list entries should fail");
        assert!(error.message.contains("expected a list item"));
    }

    #[test]
    fn rejects_nested_lists() {
        let error = parse("package demo:\n  dependencies: [one, [two]]\n")
            .expect_err("nested lists are not part of the initial language");
        assert!(error.message.contains("nested lists are not supported"));
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
             \x20\x20\x20\x20url: source-url\n\
             \x20\x20homepage:\n\
             \x20\x20\x20\x20url: home-url\n",
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
                .contains("unexpected text after quoted string")
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
        let error =
            parse("package demo:\n  values: [one two]\n").expect_err("missing comma should fail");
        assert_eq!(
            error.render("demo.sw"),
            "demo.sw:2:15: error: cannot read `one two` as a value\n  help: use one unquoted atom, a double-quoted string, or a list such as `[rust, cargo]`."
        );
    }

    #[test]
    fn allows_a_trailing_comma_but_rejects_empty_list_entries() {
        let package = parse("package demo:\n  dependencies: [one, two,]\n")
            .expect("a trailing comma should be allowed");
        assert!(matches!(
            package.fields["dependencies"].value,
            Value::List(_)
        ));

        let error = parse("package demo:\n  dependencies: [one,,two]\n")
            .expect_err("empty list entries should fail");
        assert!(error.message.contains("expected a list item"));
    }
}
