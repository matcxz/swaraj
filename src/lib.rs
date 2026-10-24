//! Parser and syntax model for the initial Swaraj language.
//!
//! A source file contains one package declaration and an indented mapping of
//! fields. Expressions in fields can use bindings declared in a `let:` block.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

mod expression;
pub use expression::FunctionValue;

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
    /// A function value. Functions can be used in expressions but not stored
    /// directly in package definitions.
    Function(FunctionValue),
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
    source_path: Option<PathBuf>,
    import_stack: Rc<RefCell<Vec<PathBuf>>>,
}

impl<'a> Parser<'a> {
    fn new(
        source: &'a str,
        source_path: Option<PathBuf>,
        import_stack: Rc<RefCell<Vec<PathBuf>>>,
    ) -> Result<Self, Diagnostic> {
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

        Ok(Self {
            lines,
            position: 0,
            source_path,
            import_stack,
        })
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

        let mut evaluator = expression::Evaluator::new(
            &bindings,
            self.source_path.as_deref(),
            self.import_stack.clone(),
        );
        let fields = self.parse_map(2, &mut evaluator)?;
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

        validate_package_fields(&fields, package_location)?;

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
        evaluator: &mut expression::Evaluator<'_>,
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
                Value::Map(self.parse_map(indent + 2, evaluator)?)
            } else {
                let source = self.collect_expression(&raw_value, indent)?;
                let parsed = expression::parse(&source, value_location)?;
                evaluator.evaluate(&parsed)?
            };

            fields.insert(
                key.to_owned(),
                Located {
                    value: reject_function_values(value, value_location)?,
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
        let mut map_depth = 0usize;
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
            } else if !quoted && character == '{' {
                map_depth += 1;
            } else if !quoted && character == '}' {
                map_depth = map_depth.saturating_sub(1);
            }
        }
        if depth == 0 && map_depth == 0 {
            return Ok(source);
        }

        while depth > 0 || map_depth > 0 {
            let Some(next) = self.lines.get(self.position) else {
                return Ok(source);
            };
            let is_aligned_closing_line = next.indent == field_indent
                && (next.text.starts_with(']') || next.text.starts_with('}'));
            let indentation_is_invalid = if map_depth > 0 {
                next.indent < field_indent + 2 && !is_aligned_closing_line
            } else {
                !is_aligned_closing_line && next.indent != field_indent + 2
            };
            if indentation_is_invalid {
                return Err(Diagnostic::new(
                    next.number,
                    next.indent + 1,
                    if map_depth > 0 {
                        "attribute set continuation has unexpected indentation"
                    } else {
                        "list continuation has unexpected indentation"
                    },
                    if map_depth > 0 {
                        "indent attribute assignments below the field and align the closing `}` with the field."
                    } else {
                        "indent list items two spaces below the field and align the closing `]` with the field."
                    },
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
                } else if !quoted && character == '{' {
                    map_depth += 1;
                } else if !quoted && character == '}' {
                    map_depth = map_depth.saturating_sub(1);
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

fn validate_package_fields(
    fields: &BTreeMap<String, Located<Value>>,
    package_location: SourceLocation,
) -> Result<(), Diagnostic> {
    let version = fields.get("version").ok_or_else(|| {
        Diagnostic::new(
            package_location.line,
            package_location.column,
            "package is missing the required `version` field",
            "add a non-empty string version, for example `version: \"1.0\"`.",
        )
    })?;
    validate_non_empty_string(version, "version")?;

    if let Some(description) = fields.get("description") {
        validate_non_empty_string(description, "description")?;
    }
    if let Some(source) = fields.get("source") {
        validate_source(source)?;
    }
    if let Some(dependencies) = fields.get("dependencies") {
        validate_dependencies(dependencies)?;
    }
    for name in ["build", "install"] {
        if let Some(phase) = fields.get(name) {
            validate_commands(phase, name)?;
        }
    }

    Ok(())
}

fn validate_non_empty_string(field: &Located<Value>, name: &str) -> Result<(), Diagnostic> {
    match &field.value {
        Value::String(value) if !value.trim().is_empty() => Ok(()),
        Value::String(_) => Err(schema_error(
            field.location,
            name,
            "must not be empty",
            "provide a non-empty string.",
        )),
        _ => Err(schema_error(
            field.location,
            name,
            "must be a string",
            "use a quoted string value.",
        )),
    }
}

fn validate_source(source: &Located<Value>) -> Result<(), Diagnostic> {
    let Value::Map(fields) = &source.value else {
        return Err(schema_error(
            source.location,
            "source",
            "must be a mapping",
            "use `source:` with indented `url` and `sha256` fields.",
        ));
    };
    let url = fields.get("url").ok_or_else(|| {
        schema_error(
            source.location,
            "source.url",
            "is required when `source` is provided",
            "add a non-empty source locator.",
        )
    })?;
    let url_value = match &url.value {
        Value::String(value) if !value.trim().is_empty() => value,
        Value::String(_) => {
            return Err(schema_error(
                url.location,
                "source.url",
                "must not be empty",
                "provide a non-empty absolute source URI.",
            ));
        }
        _ => {
            return Err(schema_error(
                url.location,
                "source.url",
                "must be a string",
                "use a quoted absolute source URI.",
            ));
        }
    };
    if !has_uri_scheme(url_value) {
        return Err(schema_error(
            url.location,
            "source.url",
            "must be an absolute URI with a scheme",
            "use a source URI such as `https://...`, `file://...`, or `ipfs://...`.",
        ));
    }

    let checksum = fields.get("sha256").ok_or_else(|| {
        schema_error(
            source.location,
            "source.sha256",
            "is required when `source` is provided",
            "add the source archive's 64-character SHA-256 checksum.",
        )
    })?;
    match &checksum.value {
        Value::String(value)
            if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
        {
            Ok(())
        }
        Value::String(_) => Err(schema_error(
            checksum.location,
            "source.sha256",
            "must contain exactly 64 hexadecimal characters",
            "replace the placeholder with the archive's SHA-256 checksum.",
        )),
        _ => Err(schema_error(
            checksum.location,
            "source.sha256",
            "must be a string",
            "use a quoted 64-character hexadecimal SHA-256 checksum.",
        )),
    }
}

fn validate_dependencies(dependencies: &Located<Value>) -> Result<(), Diagnostic> {
    match &dependencies.value {
        Value::List(items) => validate_string_list(items, "dependencies"),
        Value::Map(groups) => {
            for (group_name, group) in groups {
                if !matches!(group_name.as_str(), "build" | "runtime") {
                    return Err(schema_error(
                        group.location,
                        &format!("dependencies.{group_name}"),
                        "is not a supported dependency group",
                        "use `build` or `runtime` dependency groups.",
                    ));
                }
                let Value::List(items) = &group.value else {
                    return Err(schema_error(
                        group.location,
                        &format!("dependencies.{group_name}"),
                        "must be a list of package names",
                        "use a list of quoted dependency names.",
                    ));
                };
                validate_string_list(items, &format!("dependencies.{group_name}"))?;
            }
            Ok(())
        }
        _ => Err(schema_error(
            dependencies.location,
            "dependencies",
            "must be a list or a mapping of dependency groups",
            "use a list of names or `build` and `runtime` lists.",
        )),
    }
}

fn validate_commands(phase: &Located<Value>, phase_name: &str) -> Result<(), Diagnostic> {
    let Value::Map(fields) = &phase.value else {
        return Err(schema_error(
            phase.location,
            phase_name,
            "must be a mapping containing `commands`",
            "use an indented `commands` list of quoted command strings.",
        ));
    };
    let commands = fields.get("commands").ok_or_else(|| {
        schema_error(
            phase.location,
            &format!("{phase_name}.commands"),
            "is required",
            "add a non-empty list of command strings.",
        )
    })?;
    let Value::List(items) = &commands.value else {
        return Err(schema_error(
            commands.location,
            &format!("{phase_name}.commands"),
            "must be a list of command strings",
            "use a list of quoted command strings.",
        ));
    };
    if items.is_empty() {
        return Err(schema_error(
            commands.location,
            &format!("{phase_name}.commands"),
            "must not be empty",
            "add at least one command string.",
        ));
    }
    validate_string_list(items, &format!("{phase_name}.commands"))
}

fn validate_string_list(items: &[Located<Value>], field_name: &str) -> Result<(), Diagnostic> {
    for item in items {
        validate_non_empty_string(item, field_name)?;
    }
    Ok(())
}

fn has_uri_scheme(value: &str) -> bool {
    let Some((scheme, remainder)) = value.split_once(':') else {
        return false;
    };
    let mut characters = scheme.chars();
    matches!(characters.next(), Some(character) if character.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '.' | '-')
        })
        && !remainder.trim().is_empty()
        && !value.chars().any(char::is_whitespace)
}

fn schema_error(
    location: SourceLocation,
    field_name: &str,
    message: &str,
    help: &str,
) -> Diagnostic {
    Diagnostic::new(
        location.line,
        location.column,
        format!("invalid `{field_name}`: {message}"),
        help,
    )
}

fn reject_function_values(value: Value, location: SourceLocation) -> Result<Value, Diagnostic> {
    let contains_function = match &value {
        Value::Function(_) => true,
        Value::List(items) => items
            .iter()
            .any(|item| contains_function_value(&item.value)),
        Value::Map(fields) => fields
            .values()
            .any(|field| contains_function_value(&field.value)),
        Value::String(_) | Value::Integer(_) | Value::Boolean(_) => false,
    };
    if contains_function {
        Err(Diagnostic::new(
            location.line,
            location.column,
            "function values cannot be stored in package definitions",
            "call the function and assign its result instead.",
        ))
    } else {
        Ok(value)
    }
}

fn contains_function_value(value: &Value) -> bool {
    match value {
        Value::Function(_) => true,
        Value::List(items) => items
            .iter()
            .any(|item| contains_function_value(&item.value)),
        Value::Map(fields) => fields
            .values()
            .any(|field| contains_function_value(&field.value)),
        Value::String(_) | Value::Integer(_) | Value::Boolean(_) => false,
    }
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
    Parser::new(source, None, Rc::new(RefCell::new(Vec::new())))?.parse()
}

/// Parse a package definition with a source path for relative imports.
pub fn parse_with_path(source: &str, path: impl AsRef<Path>) -> Result<Package, Diagnostic> {
    let path = path.as_ref().canonicalize().map_err(|error| {
        Diagnostic::new(
            1,
            1,
            format!("could not resolve source path: {error}"),
            "provide a readable path to the source file.",
        )
    })?;
    parse_with_context(source, path, Rc::new(RefCell::new(Vec::new())))
}

fn parse_with_context(
    source: &str,
    path: PathBuf,
    import_stack: Rc<RefCell<Vec<PathBuf>>>,
) -> Result<Package, Diagnostic> {
    if import_stack.borrow().contains(&path) {
        return Err(Diagnostic::new(
            1,
            1,
            format!("import cycle detected at `{}`", path.display()),
            "remove the circular chain of imports.",
        ));
    }
    if import_stack.borrow().len() >= MAX_NESTING_DEPTH {
        return Err(Diagnostic::new(
            1,
            1,
            "import nesting exceeds the limit of 128 files",
            "reduce the number of nested imports.",
        ));
    }
    import_stack.borrow_mut().push(path.clone());
    let result = match Parser::new(source, Some(path), import_stack.clone()) {
        Ok(parser) => parser.parse(),
        Err(error) => Err(error),
    };
    import_stack.borrow_mut().pop();
    result
}

pub(crate) fn import_package(
    source_path: Option<&Path>,
    import_stack: Rc<RefCell<Vec<PathBuf>>>,
    import: &str,
    location: SourceLocation,
) -> Result<Value, Diagnostic> {
    let Some(source_path) = source_path else {
        return Err(Diagnostic::new(
            location.line,
            location.column,
            "imports require a source file path",
            "use `parse_with_path` or the `swaraj check` command to enable relative imports.",
        ));
    };
    let path = source_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(import)
        .canonicalize()
        .map_err(|error| {
            Diagnostic::new(
                location.line,
                location.column,
                format!("could not import `{import}`: {error}"),
                "check that the relative import exists and can be read.",
            )
        })?;
    let source = std::fs::read_to_string(&path).map_err(|error| {
        Diagnostic::new(
            location.line,
            location.column,
            format!("could not read imported file `{}`: {error}", path.display()),
            "check the file permissions and encoding.",
        )
    })?;
    let package =
        parse_with_context(&source, path.clone(), import_stack).map_err(|mut error| {
            error.message = format!("in imported file `{}`: {}", path.display(), error.message);
            error
        })?;
    let mut fields = package.fields;
    if fields.contains_key("packageName") {
        return Err(Diagnostic::new(
            location.line,
            location.column,
            "imported package uses the reserved attribute `packageName`",
            "rename this field so the imported package name can be exposed.",
        ));
    }
    fields.insert(
        "packageName".to_owned(),
        Located {
            value: Value::String(package.name),
            location: package.location,
        },
    );
    Ok(Value::Map(fields))
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, Located, SourceLocation, Value, parse};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_IMPORT_TEST: AtomicUsize = AtomicUsize::new(0);

    fn string_item(value: &str, line: usize, column: usize) -> Located<Value> {
        Located {
            value: Value::String(value.to_owned()),
            location: SourceLocation { line, column },
        }
    }

    fn list_values(value: &Located<Value>) -> Vec<Value> {
        let Value::List(items) = &value.value else {
            panic!("expected a list");
        };
        items.iter().map(|item| item.value.clone()).collect()
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
  version: "1.0"
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
  version: "1.0"
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
        let package = parse(
            "package demo:\r\n  dependencies: [\"one\", \"two\",]\r\n  optional: []\r\n  version: \"1.0\"\r\n",
        )
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
  version: "1.0"
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
                column: 10,
                message: "function application expects a function, but got a string".to_owned(),
                help: "change the value's type or use an operator that accepts it.".to_owned(),
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
        let package =
            parse("package demo:\n  groups: [[\"one\"], [\"two\"]]\n  version: \"1.0\"\n")
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
    fn validates_package_schema_and_preserves_extension_fields() {
        let package = parse(
            r#"package demo:
  version: "1.0"
  description: "Demo package"
  source:
    url: "https://example.test/demo.tar.gz"
    sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  dependencies:
    build: ["compiler"]
    runtime: ["library"]
  build:
    commands: ["make"]
  install:
    commands: ["make install"]
  metadata: { maintainers = ["team"]; }
"#,
        )
        .expect("a package following the core schema should validate");
        assert!(package.fields.contains_key("metadata"));

        let errors = [
            (
                "package demo:\n  description: \"missing version\"\n",
                "required `version`",
            ),
            ("package demo:\n  version: \"\"\n", "invalid `version`"),
            (
                "package demo:\n  version: true\n",
                "invalid `version`: must be a string",
            ),
            (
                "package demo:\n  version: \"1.0\"\n  source:\n    url: \"relative/archive.tar.gz\"\n    sha256: \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n",
                "absolute URI",
            ),
            (
                "package demo:\n  version: \"1.0\"\n  source:\n    url: \"https://example.test/archive.tar.gz\"\n",
                "source.sha256",
            ),
            (
                "package demo:\n  version: \"1.0\"\n  source:\n    url: \"https://example.test/a.tar.gz\"\n    sha256: \"not-a-checksum\"\n",
                "64 hexadecimal characters",
            ),
            (
                "package demo:\n  version: \"1.0\"\n  dependencies: { build = \"compiler\"; }\n",
                "must be a list of package names",
            ),
            (
                "package demo:\n  version: \"1.0\"\n  build: { }\n",
                "build.commands",
            ),
            (
                "package demo:\n  version: \"1.0\"\n  dependencies: [\"\", \"library\"]\n",
                "invalid `dependencies`",
            ),
            (
                "package demo:\n  version: \"1.0\"\n  build:\n    commands: []\n",
                "must not be empty",
            ),
        ];
        for (source, expected) in errors {
            let error = parse(source).expect_err("invalid package schema should be rejected");
            assert!(error.message.contains(expected), "{error:?}");
        }
    }

    #[test]
    fn permits_same_field_names_in_separate_mappings() {
        let package = parse(
            "package demo:\n\
             \x20\x20source:\n\
            \x20\x20\x20\x20url: \"https://source-url\"\n\
            \x20\x20\x20\x20sha256: \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n\
             \x20\x20homepage:\n\
            \x20\x20\x20\x20url: \"home-url\"\n\
             \x20\x20version: \"1.0\"\n",
        )
        .expect("field names are only unique within their own mapping");

        let Value::Map(source) = &package.fields["source"].value else {
            panic!("source should be a mapping");
        };
        let Value::Map(homepage) = &package.fields["homepage"].value else {
            panic!("homepage should be a mapping");
        };
        assert_eq!(
            source["url"].value,
            Value::String("https://source-url".to_owned())
        );
        assert_eq!(homepage["url"].value, Value::String("home-url".to_owned()));
    }

    #[test]
    fn reports_invalid_headers_and_malformed_strings() {
        let header_error = parse("package 1invalid:\n  version: 1\n")
            .expect_err("package names must start with a letter or underscore");
        assert!(header_error.message.contains("invalid package name"));

        let quote_error = parse("package demo:\n  version: \"valid\" trailing\n")
            .expect_err("quoted strings must contain the whole scalar");
        assert!(quote_error.message.contains("function application"));

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
    fn limits_expression_nesting_with_a_diagnostic() {
        let opening = "(".repeat(100);
        let closing = ")".repeat(100);
        let source = format!("package demo:\n  value: {opening}0{closing}\n");
        let error = parse(&source).expect_err("excessive expression nesting should fail safely");
        assert!(error.message.contains("maximum expression nesting depth"));
    }

    #[test]
    fn renders_actionable_diagnostics() {
        let error = parse("package demo:\n  values: [\"one\",, \"two\"]\n")
            .expect_err("empty list entries should fail");
        assert!(error.message.contains("expected a value or expression"));
        assert!(error.render("demo.sw").starts_with("demo.sw:2:"));
        assert!(!error.help.is_empty());
    }

    #[test]
    fn allows_a_trailing_comma_but_rejects_empty_list_entries() {
        let package =
            parse("package demo:\n  dependencies: [\"one\", \"two\",]\n  version: \"1.0\"\n")
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
    fn evaluates_attribute_sets_selection_existence_and_merge() {
        let package = parse(
            r#"let:
  base: {
    version = "1.0";
    enabled = true;
    labels = ["stable"];
  }
  config: base // { version = "2.0"; }
  selectedOrDefault: config.channel or "stable"
  interpolated: "${config.version}"
in:
package demo:
  version: config.version
  selectedOrDefault: selectedOrDefault
  interpolated: interpolated
  enabled: config.enabled
  hasLabels: config ? labels
  hasMissing: config ? missing
  merged: config
"#,
        )
        .expect("attribute sets, selection, existence, and merge should evaluate");

        assert_eq!(
            package.fields["version"].value,
            Value::String("2.0".to_owned())
        );
        assert_eq!(
            package.fields["selectedOrDefault"].value,
            Value::String("stable".to_owned())
        );
        assert_eq!(
            package.fields["interpolated"].value,
            Value::String("2.0".to_owned())
        );
        assert_eq!(package.fields["enabled"].value, Value::Boolean(true));
        assert_eq!(package.fields["hasLabels"].value, Value::Boolean(true));
        assert_eq!(package.fields["hasMissing"].value, Value::Boolean(false));
        let Value::Map(merged) = &package.fields["merged"].value else {
            panic!("merged should be a mapping");
        };
        assert_eq!(merged["version"].value, Value::String("2.0".to_owned()));
        assert!(merged.contains_key("labels"));
    }

    #[test]
    fn reports_missing_attributes_and_invalid_map_operations() {
        let missing = parse(
            "let:\n  config: { version = \"1\"; }\nin:\npackage demo:\n  value: config.release\n",
        )
        .expect_err("missing attributes should be diagnosed");
        assert!(
            missing
                .message
                .contains("attribute `release` does not exist")
        );

        let mismatch = parse(
            "let:\n  config: { version = \"1\"; }\nin:\npackage demo:\n  value: config // [\"invalid\"]\n",
        )
        .expect_err("map merging should reject non-maps");
        assert!(mismatch.message.contains("mappings on both sides"));
    }

    #[test]
    fn evaluates_recursive_attribute_sets_on_demand() {
        let package = parse(
            r#"let:
  config: rec {
    major = 2;
    minor = 5;
    version = "${major}.${minor}";
    enabled = true;
    summary = if enabled then "v${version}" else missing;
    unused = missing;
  }
in:
package demo:
  version: config.version
  summary: config.summary
  hasUnused: config ? unused
"#,
        )
        .expect("recursive attributes and lazy unused values should evaluate");

        assert_eq!(
            package.fields["version"].value,
            Value::String("2.5".to_owned())
        );
        assert_eq!(
            package.fields["summary"].value,
            Value::String("v2.5".to_owned())
        );
        assert_eq!(package.fields["hasUnused"].value, Value::Boolean(true));
    }

    #[test]
    fn recursive_attribute_sets_support_recursive_functions() {
        let package = parse(
            r#"let:
  math: rec {
    factorial = number:
      if number == 0 then 1 else number * factorial (number - 1);
  }
in:
package demo:
  result: math.factorial 6
  version: "1.0"
"#,
        )
        .expect("recursive functions in recursive sets should evaluate");
        assert_eq!(package.fields["result"].value, Value::Integer(720));
    }

    #[test]
    fn bounds_recursive_expression_evaluation() {
        let error = parse("let:\n  loop: value: loop value\nin:\npackage demo:\n  value: loop 0\n")
            .expect_err("unbounded recursive calls should stop with a diagnostic");
        assert!(
            error
                .message
                .contains("maximum expression evaluation depth")
        );
    }

    #[test]
    fn reports_cycles_inside_recursive_attribute_sets() {
        let error = parse("package demo:\n  config: rec { first = second; second = first; }\n")
            .expect_err("recursive attribute cycles should be diagnosed");
        assert!(error.message.contains("recursive attribute definition"));
    }

    #[test]
    fn imports_package_files_relative_to_the_importing_file() {
        let id = NEXT_IMPORT_TEST.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("swaraj-import-{}-{id}", std::process::id()));
        fs::create_dir(&directory).expect("temporary import directory should be created");
        fs::create_dir(directory.join("sub")).expect("nested import directory should be created");
        fs::write(
            directory.join("sub/base.sw"),
            "package base:\n  version: \"1.2\"\n  requirements:\n    compiler: \"rustc\"\n",
        )
        .expect("imported package should be written");
        fs::write(directory.join("sub/version.txt"), "release-1.2\n")
            .expect("imported text file should be written");
        let source = r#"let:
  base: builtins.import "sub/base.sw"
  sourceText: builtins.readFile "sub/version.txt"
in:
package app:
  version: base.version
  importedName: base.packageName
  compiler: base.requirements.compiler
  sourceExists: builtins.pathExists "sub/base.sw"
  sourceText: sourceText
"#;
        let main_path = directory.join("main.sw");
        fs::write(&main_path, source).expect("main package should be written");
        let package = super::parse_with_path(source, &main_path)
            .expect("relative package imports should resolve");
        assert_eq!(
            package.fields["version"].value,
            Value::String("1.2".to_owned())
        );
        assert_eq!(
            package.fields["importedName"].value,
            Value::String("base".to_owned())
        );
        assert_eq!(
            package.fields["compiler"].value,
            Value::String("rustc".to_owned())
        );
        assert_eq!(package.fields["sourceExists"].value, Value::Boolean(true));
        assert_eq!(
            package.fields["sourceText"].value,
            Value::String("release-1.2\n".to_owned())
        );
        fs::remove_dir_all(directory).expect("temporary import directory should be removed");
    }

    #[test]
    fn detects_import_cycles_and_requires_a_source_path() {
        let id = NEXT_IMPORT_TEST.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("swaraj-import-{}-{id}", std::process::id()));
        fs::create_dir(&directory).expect("temporary import directory should be created");
        let first = directory.join("first.sw");
        let second = directory.join("second.sw");
        fs::write(
            &first,
            "let:\n  other: builtins.import \"second.sw\"\nin:\npackage first:\n  value: other.value\n",
        )
        .expect("first import should be written");
        fs::write(
            &second,
            "let:\n  other: builtins.import \"first.sw\"\nin:\npackage second:\n  value: other.value\n",
        )
        .expect("second import should be written");
        let source = fs::read_to_string(&first).expect("first import should be readable");
        let error =
            super::parse_with_path(&source, &first).expect_err("circular imports should fail");
        assert!(error.message.contains("import cycle detected"));

        let no_path = parse("package demo:\n  imported: builtins.import \"other.sw\"\n")
            .expect_err("string parsing has no source path for imports");
        assert!(no_path.message.contains("require a source file path"));
        fs::remove_dir_all(directory).expect("temporary import directory should be removed");
    }

    #[test]
    fn evaluates_single_argument_functions_with_lexical_bindings() {
        let package = parse(
            r#"let:
  offset: 40
  increment: value: value + 1
  addOffset: value: value + offset
  answer: addOffset (increment 1)
in:
package demo:
  answer: answer
  version: "1.0"
"#,
        )
        .expect("lambda bindings and function application should evaluate");
        assert_eq!(package.fields["answer"].value, Value::Integer(42));
    }

    #[test]
    fn supports_curried_functions_closures_and_higher_order_calls() {
        let package = parse(
            r#"let:
  offset: 10
  makeAdder: amount: value: amount + value + offset
  addFive: makeAdder 5
  apply: function: value: function value
  shadow: offset: offset
  values: builtins.map addFive [1, 2]
in:
package demo:
  answer: apply addFive 27
  shadowed: shadow 3
  values: values
  version: "1.0"
"#,
        )
        .expect("closures should capture lexical bindings and be first-class");
        assert_eq!(package.fields["answer"].value, Value::Integer(42));
        assert_eq!(package.fields["shadowed"].value, Value::Integer(3));
        assert_eq!(
            list_values(&package.fields["values"]),
            vec![Value::Integer(16), Value::Integer(17)]
        );
    }

    #[test]
    fn rejects_unapplied_functions_and_calls_to_non_functions() {
        let uncalled =
            parse("let:\n  identity: value: value\nin:\npackage demo:\n  value: identity\n")
                .expect_err("a function cannot be a package value");
        assert!(uncalled.message.contains("cannot be stored"));

        let not_function = parse("let:\n  value: 1\nin:\npackage demo:\n  result: value 2\n")
            .expect_err("non-function values cannot be called");
        assert!(not_function.message.contains("function application"));
    }

    #[test]
    fn evaluates_core_list_attribute_and_string_builtins() {
        let package = parse(
            r#"let:
  values: ["first", "second"]
  metadata: { z = 1; a = 2; }
in:
package demo:
  length: builtins.length values
  first: builtins.head values
  rest: builtins.tail values
  names: builtins.attrNames metadata
  text: builtins.toString 42
  joined: builtins.concatStringsSep ", " values
  containsSecond: builtins.elem "second" values
  hasAttribute: builtins.hasAttr "a" metadata
  attributeValue: builtins.getAttr "z" metadata
  values: builtins.attrValues metadata
  textLength: builtins.stringLength "café"
  basename: builtins.baseNameOf "source/archive.tar.gz"
  directory: builtins.dirOf "source/archive.tar.gz"
  excerpt: builtins.substring 1 2 "café"
  replaced: builtins.replaceStrings ["aa", "a"] ["x", "y"] "aaa"
  isText: builtins.isString "text"
  isNumber: builtins.isInt 42
  isAttrs: builtins.isAttrs metadata
  valueType: builtins.typeOf metadata
  sequenced: builtins.seq (builtins.length values) "ready"
  incremented: builtins.map (value: value + 1) [1, 2, 3]
  evens: builtins.filter (value: value % 2 == 0) [1, 2, 3, 4]
  flattened: builtins.concatLists [[1], [2, 3]]
  total: builtins.foldl' (acc: value: acc + value) 0 [1, 2, 3, 4]
  generated: builtins.genList (index: index * 2) 4
  zipped: builtins.zipListsWith (left: right: left + right) [1, 2] [10, 20]
  mappedAttributes: builtins.mapAttrs (name: value: value + 1) metadata
  filteredAttributes: builtins.filterAttrs (name: value: name == "a") metadata
  fromList: builtins.listToAttrs [{ name = "answer"; value = 42; }]
  version: "1.0"
"#,
        )
        .expect("supported built-ins should evaluate");

        assert_eq!(package.fields["length"].value, Value::Integer(2));
        assert_eq!(
            package.fields["first"].value,
            Value::String("first".to_owned())
        );
        let Value::List(rest) = &package.fields["rest"].value else {
            panic!("tail should return a list");
        };
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].value, Value::String("second".to_owned()));
        let Value::List(names) = &package.fields["names"].value else {
            panic!("attrNames should return a list");
        };
        assert_eq!(
            names
                .iter()
                .map(|item| match &item.value {
                    Value::String(name) => name.as_str(),
                    other => panic!("expected an attribute name, got {other:?}"),
                })
                .collect::<Vec<_>>(),
            vec!["a", "z"]
        );
        assert_eq!(package.fields["text"].value, Value::String("42".to_owned()));
        assert_eq!(
            package.fields["joined"].value,
            Value::String("first, second".to_owned())
        );
        assert_eq!(package.fields["containsSecond"].value, Value::Boolean(true));
        assert_eq!(package.fields["hasAttribute"].value, Value::Boolean(true));
        assert_eq!(package.fields["attributeValue"].value, Value::Integer(1));
        assert_eq!(
            list_values(&package.fields["values"]),
            vec![Value::Integer(2), Value::Integer(1)]
        );
        assert_eq!(package.fields["textLength"].value, Value::Integer(4));
        assert_eq!(
            package.fields["basename"].value,
            Value::String("archive.tar.gz".to_owned())
        );
        assert_eq!(
            package.fields["directory"].value,
            Value::String("source".to_owned())
        );
        assert_eq!(
            package.fields["excerpt"].value,
            Value::String("af".to_owned())
        );
        assert_eq!(
            package.fields["replaced"].value,
            Value::String("xy".to_owned())
        );
        assert_eq!(package.fields["isText"].value, Value::Boolean(true));
        assert_eq!(package.fields["isNumber"].value, Value::Boolean(true));
        assert_eq!(package.fields["isAttrs"].value, Value::Boolean(true));
        assert_eq!(
            package.fields["valueType"].value,
            Value::String("set".to_owned())
        );
        assert_eq!(
            package.fields["sequenced"].value,
            Value::String("ready".to_owned())
        );
        assert_eq!(
            list_values(&package.fields["incremented"]),
            vec![Value::Integer(2), Value::Integer(3), Value::Integer(4)]
        );
        assert_eq!(
            list_values(&package.fields["evens"]),
            vec![Value::Integer(2), Value::Integer(4)]
        );
        assert_eq!(
            list_values(&package.fields["flattened"]),
            vec![Value::Integer(1), Value::Integer(2), Value::Integer(3)]
        );
        assert_eq!(package.fields["total"].value, Value::Integer(10));
        assert_eq!(
            list_values(&package.fields["generated"]),
            vec![
                Value::Integer(0),
                Value::Integer(2),
                Value::Integer(4),
                Value::Integer(6)
            ]
        );
        assert_eq!(
            list_values(&package.fields["zipped"]),
            vec![Value::Integer(11), Value::Integer(22)]
        );
        let Value::Map(mapped) = &package.fields["mappedAttributes"].value else {
            panic!("mapAttrs should return an attribute set");
        };
        assert_eq!(mapped["a"].value, Value::Integer(3));
        let Value::Map(filtered) = &package.fields["filteredAttributes"].value else {
            panic!("filterAttrs should return an attribute set");
        };
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered["a"].value, Value::Integer(2));
        let Value::Map(from_list) = &package.fields["fromList"].value else {
            panic!("listToAttrs should return an attribute set");
        };
        assert_eq!(from_list["answer"].value, Value::Integer(42));
    }

    #[test]
    fn reports_invalid_builtin_arguments_and_names() {
        let empty_head = parse("package demo:\n  value: builtins.head []\n")
            .expect_err("head on an empty list should fail");
        assert!(empty_head.message.contains("empty list"));

        let unknown = parse("package demo:\n  value: builtins.notReal []\n")
            .expect_err("unknown built-ins should fail");
        assert!(unknown.message.contains("unknown built-in"));
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
  version: "1.0"
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
  version: "1.0"
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
  version: "1.0"
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
  version: "1.0"
"#,
        )
        .expect("multiline nested lists and trailing commas should parse");
        assert!(matches!(package.fields["items"].value, Value::List(_)));
    }
}
