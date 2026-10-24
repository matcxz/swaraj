use std::collections::BTreeMap;

use crate::{Diagnostic, Located, SourceLocation, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Expression {
    kind: ExpressionKind,
    location: SourceLocation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExpressionKind {
    String(String),
    Integer(i64),
    Boolean(bool),
    Variable(String),
    List(Vec<Expression>),
    If(Box<Expression>, Box<Expression>, Box<Expression>),
    Unary(UnaryOperator, Box<Expression>),
    Binary(Box<Expression>, BinaryOperator, Box<Expression>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UnaryOperator {
    Not,
    Negate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    Append,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TokenKind {
    String(String),
    Integer(i64),
    Identifier(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
    Append,
    Bang,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,
    End,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    location: SourceLocation,
}

struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    location: SourceLocation,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str, location: SourceLocation) -> Self {
        Self {
            source,
            offset: 0,
            location,
        }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, Diagnostic> {
        let mut tokens = Vec::new();
        while let Some(character) = self.peek() {
            if character.is_whitespace() {
                self.advance();
                continue;
            }

            let location = self.location;
            let kind = match character {
                '"' => self.string()?,
                '0'..='9' => self.integer(location)?,
                'a'..='z' | 'A'..='Z' | '_' => self.identifier(),
                '+' => {
                    self.advance();
                    if self.consume('+') {
                        TokenKind::Append
                    } else {
                        TokenKind::Plus
                    }
                }
                '-' => {
                    self.advance();
                    TokenKind::Minus
                }
                '*' => {
                    self.advance();
                    TokenKind::Star
                }
                '/' => {
                    self.advance();
                    TokenKind::Slash
                }
                '%' => {
                    self.advance();
                    TokenKind::Percent
                }
                '=' => {
                    self.advance();
                    if self.consume('=') {
                        TokenKind::EqualEqual
                    } else {
                        return Err(self.unexpected(location, "use `==` for equality."));
                    }
                }
                '!' => {
                    self.advance();
                    if self.consume('=') {
                        TokenKind::BangEqual
                    } else {
                        TokenKind::Bang
                    }
                }
                '<' => {
                    self.advance();
                    if self.consume('=') {
                        TokenKind::LessEqual
                    } else {
                        TokenKind::Less
                    }
                }
                '>' => {
                    self.advance();
                    if self.consume('=') {
                        TokenKind::GreaterEqual
                    } else {
                        TokenKind::Greater
                    }
                }
                '&' => {
                    self.advance();
                    if self.consume('&') {
                        TokenKind::AndAnd
                    } else {
                        return Err(self.unexpected(location, "use `&&` for boolean AND."));
                    }
                }
                '|' => {
                    self.advance();
                    if self.consume('|') {
                        TokenKind::OrOr
                    } else {
                        return Err(self.unexpected(location, "use `||` for boolean OR."));
                    }
                }
                '(' => {
                    self.advance();
                    TokenKind::LeftParen
                }
                ')' => {
                    self.advance();
                    TokenKind::RightParen
                }
                '[' => {
                    self.advance();
                    TokenKind::LeftBracket
                }
                ']' => {
                    self.advance();
                    TokenKind::RightBracket
                }
                ',' => {
                    self.advance();
                    TokenKind::Comma
                }
                _ => {
                    return Err(
                        self.unexpected(location, "use a literal, variable, list, or operator.")
                    );
                }
            };
            tokens.push(Token { kind, location });
        }
        tokens.push(Token {
            kind: TokenKind::End,
            location: self.location,
        });
        Ok(tokens)
    }

    fn string(&mut self) -> Result<TokenKind, Diagnostic> {
        let opening = self.location;
        self.advance();
        let mut value = String::new();
        loop {
            let Some(character) = self.advance() else {
                return Err(Diagnostic::new(
                    opening.line,
                    opening.column,
                    "quoted string is not closed",
                    "close the string with `\"` on the same line.",
                ));
            };
            match character {
                '"' => return Ok(TokenKind::String(value)),
                '\\' => {
                    let Some(escaped) = self.advance() else {
                        return Err(Diagnostic::new(
                            self.location.line,
                            self.location.column,
                            "incomplete escape sequence",
                            "use `\\\\`, `\\\"`, `\\n`, `\\r` or `\\t`.",
                        ));
                    };
                    match escaped {
                        '\\' => value.push('\\'),
                        '"' => value.push('"'),
                        'n' => value.push('\n'),
                        'r' => value.push('\r'),
                        't' => value.push('\t'),
                        _ => {
                            return Err(Diagnostic::new(
                                self.location.line,
                                self.location.column.saturating_sub(1),
                                format!("unsupported escape sequence `\\{escaped}`"),
                                "use `\\\\`, `\\\"`, `\\n`, `\\r` or `\\t`.",
                            ));
                        }
                    }
                }
                '$' if self.peek() == Some('{') => {
                    self.advance();
                    let interpolation_location = self.location;
                    let mut name = String::new();
                    while let Some(next) = self.peek() {
                        if next == '}' {
                            break;
                        }
                        name.push(next);
                        self.advance();
                    }
                    if self.advance() != Some('}') || !is_identifier(&name) {
                        return Err(Diagnostic::new(
                            interpolation_location.line,
                            interpolation_location.column,
                            "invalid string interpolation",
                            "interpolate a variable with `${name}`.",
                        ));
                    }
                    value.push_str("${");
                    value.push_str(&name);
                    value.push('}');
                }
                character => value.push(character),
            }
        }
    }

    fn integer(&mut self, location: SourceLocation) -> Result<TokenKind, Diagnostic> {
        let start = self.offset;
        while matches!(self.peek(), Some('0'..='9')) {
            self.advance();
        }
        let digits = &self.source[start..self.offset];
        let value = digits.parse::<i64>().map_err(|_| {
            Diagnostic::new(
                location.line,
                location.column,
                "integer is out of range",
                "use an integer between -9223372036854775808 and 9223372036854775807.",
            )
        })?;
        Ok(TokenKind::Integer(value))
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.offset;
        self.advance();
        while matches!(
            self.peek(),
            Some('a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-')
        ) {
            self.advance();
        }
        let identifier = &self.source[start..self.offset];
        match identifier {
            "true" => TokenKind::Identifier("true".to_owned()),
            "false" => TokenKind::Identifier("false".to_owned()),
            _ => TokenKind::Identifier(identifier.to_owned()),
        }
    }

    fn unexpected(&mut self, location: SourceLocation, help: &str) -> Diagnostic {
        let character = self.advance().unwrap_or('\0');
        Diagnostic::new(
            location.line,
            location.column,
            format!("unexpected character `{character}`"),
            help,
        )
    }

    fn peek(&self) -> Option<char> {
        self.source[self.offset..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.offset += character.len_utf8();
        if character == '\n' {
            self.location.line += 1;
            self.location.column = 1;
        } else {
            self.location.column += 1;
        }
        Some(character)
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }
}

struct ExpressionParser {
    tokens: Vec<Token>,
    position: usize,
}

impl ExpressionParser {
    fn new(source: &str, location: SourceLocation) -> Result<Self, Diagnostic> {
        Ok(Self {
            tokens: Lexer::new(source, location).tokenize()?,
            position: 0,
        })
    }

    fn parse(mut self) -> Result<Expression, Diagnostic> {
        let expression = self.parse_precedence(0)?;
        if !matches!(self.current().kind, TokenKind::End) {
            return Err(self.error(
                self.current().location,
                "unexpected token after expression",
                "remove the extra token or add an operator between values.",
            ));
        }
        Ok(expression)
    }

    fn parse_precedence(&mut self, minimum: u8) -> Result<Expression, Diagnostic> {
        let mut left = self.parse_prefix()?;
        while let Some((operator, precedence)) = binary_operator(&self.current().kind) {
            if precedence < minimum {
                break;
            }
            let location = self.advance().location;
            let right = self.parse_precedence(precedence + 1)?;
            left = Expression {
                kind: ExpressionKind::Binary(Box::new(left), operator, Box::new(right)),
                location,
            };
        }
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expression, Diagnostic> {
        let token = self.advance();
        match token.kind {
            TokenKind::String(value) => Ok(Expression {
                kind: ExpressionKind::String(value),
                location: token.location,
            }),
            TokenKind::Integer(value) => Ok(Expression {
                kind: ExpressionKind::Integer(value),
                location: token.location,
            }),
            TokenKind::Identifier(identifier) if identifier == "true" => Ok(Expression {
                kind: ExpressionKind::Boolean(true),
                location: token.location,
            }),
            TokenKind::Identifier(identifier) if identifier == "false" => Ok(Expression {
                kind: ExpressionKind::Boolean(false),
                location: token.location,
            }),
            TokenKind::Identifier(identifier) if identifier == "if" => {
                self.parse_if(token.location)
            }
            TokenKind::Identifier(identifier) => Ok(Expression {
                kind: ExpressionKind::Variable(identifier),
                location: token.location,
            }),
            TokenKind::Bang | TokenKind::Minus => {
                let operator = if token.kind == TokenKind::Bang {
                    UnaryOperator::Not
                } else {
                    UnaryOperator::Negate
                };
                let operand = self.parse_precedence(8)?;
                Ok(Expression {
                    kind: ExpressionKind::Unary(operator, Box::new(operand)),
                    location: token.location,
                })
            }
            TokenKind::LeftParen => {
                let expression = self.parse_precedence(0)?;
                if !matches!(self.current().kind, TokenKind::RightParen) {
                    return Err(self.error(
                        self.current().location,
                        "expected `)` to close this expression",
                        "add a closing `)`.",
                    ));
                }
                self.advance();
                Ok(expression)
            }
            TokenKind::LeftBracket => self.parse_list(token.location),
            _ => Err(self.error(
                token.location,
                "expected a value or expression",
                "use a string, integer, boolean, variable, list, or parenthesized expression.",
            )),
        }
    }

    fn parse_if(&mut self, location: SourceLocation) -> Result<Expression, Diagnostic> {
        let condition = self.parse_precedence(0)?;
        if !self.take_keyword("then") {
            return Err(self.error(
                self.current().location,
                "expected `then` after the condition",
                "write `if condition then value else fallback`.",
            ));
        }
        let then_branch = self.parse_precedence(0)?;
        if !self.take_keyword("else") {
            return Err(self.error(
                self.current().location,
                "expected `else` after the first branch",
                "write `if condition then value else fallback`.",
            ));
        }
        let else_branch = self.parse_precedence(0)?;
        Ok(Expression {
            kind: ExpressionKind::If(
                Box::new(condition),
                Box::new(then_branch),
                Box::new(else_branch),
            ),
            location,
        })
    }

    fn parse_list(&mut self, location: SourceLocation) -> Result<Expression, Diagnostic> {
        let mut values = Vec::new();
        while !matches!(
            self.current().kind,
            TokenKind::RightBracket | TokenKind::End
        ) {
            values.push(self.parse_precedence(0)?);
            if matches!(self.current().kind, TokenKind::Comma) {
                self.advance();
                if matches!(self.current().kind, TokenKind::RightBracket) {
                    break;
                }
            } else if !matches!(
                self.current().kind,
                TokenKind::RightBracket | TokenKind::End
            ) {
                return Err(self.error(
                    self.current().location,
                    "expected `,` between list items",
                    "separate list items with commas.",
                ));
            }
        }
        if !matches!(self.current().kind, TokenKind::RightBracket) {
            return Err(self.error(
                self.current().location,
                "list is missing its closing `]`",
                "add `]` after the final list item.",
            ));
        }
        self.advance();
        Ok(Expression {
            kind: ExpressionKind::List(values),
            location,
        })
    }

    fn take_keyword(&mut self, keyword: &str) -> bool {
        if matches!(&self.current().kind, TokenKind::Identifier(identifier) if identifier == keyword)
        {
            self.advance();
            true
        } else {
            false
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.position].clone();
        if !matches!(token.kind, TokenKind::End) {
            self.position += 1;
        }
        token
    }

    fn error(&self, location: SourceLocation, message: &str, help: &str) -> Diagnostic {
        Diagnostic::new(location.line, location.column, message, help)
    }
}

fn binary_operator(kind: &TokenKind) -> Option<(BinaryOperator, u8)> {
    Some(match kind {
        TokenKind::OrOr => (BinaryOperator::Or, 1),
        TokenKind::AndAnd => (BinaryOperator::And, 2),
        TokenKind::EqualEqual => (BinaryOperator::Equal, 3),
        TokenKind::BangEqual => (BinaryOperator::NotEqual, 3),
        TokenKind::Less => (BinaryOperator::Less, 4),
        TokenKind::LessEqual => (BinaryOperator::LessEqual, 4),
        TokenKind::Greater => (BinaryOperator::Greater, 4),
        TokenKind::GreaterEqual => (BinaryOperator::GreaterEqual, 4),
        TokenKind::Append => (BinaryOperator::Append, 5),
        TokenKind::Plus => (BinaryOperator::Add, 6),
        TokenKind::Minus => (BinaryOperator::Subtract, 6),
        TokenKind::Star => (BinaryOperator::Multiply, 7),
        TokenKind::Slash => (BinaryOperator::Divide, 7),
        TokenKind::Percent => (BinaryOperator::Remainder, 7),
        _ => return None,
    })
}

struct Evaluator<'a> {
    bindings: &'a BTreeMap<String, Expression>,
    resolved: BTreeMap<String, Value>,
    resolving: Vec<String>,
}

impl<'a> Evaluator<'a> {
    fn new(bindings: &'a BTreeMap<String, Expression>) -> Self {
        Self {
            bindings,
            resolved: BTreeMap::new(),
            resolving: Vec::new(),
        }
    }

    fn evaluate(&mut self, expression: &Expression) -> Result<Value, Diagnostic> {
        match &expression.kind {
            ExpressionKind::String(value) => {
                Ok(Value::String(self.interpolate(value, expression.location)?))
            }
            ExpressionKind::Integer(value) => Ok(Value::Integer(*value)),
            ExpressionKind::Boolean(value) => Ok(Value::Boolean(*value)),
            ExpressionKind::Variable(name) => self.resolve(name, expression.location),
            ExpressionKind::List(items) => items
                .iter()
                .map(|item| {
                    Ok(Located {
                        value: self.evaluate(item)?,
                        location: item.location,
                    })
                })
                .collect::<Result<Vec<_>, Diagnostic>>()
                .map(Value::List),
            ExpressionKind::If(condition, then_branch, else_branch) => {
                let condition = self.evaluate(condition)?;
                match condition {
                    Value::Boolean(true) => self.evaluate(then_branch),
                    Value::Boolean(false) => self.evaluate(else_branch),
                    other => Err(type_error(
                        expression.location,
                        "conditional condition",
                        "a boolean",
                        &other,
                    )),
                }
            }
            ExpressionKind::Unary(operator, operand) => {
                let value = self.evaluate(operand)?;
                match (operator, value) {
                    (UnaryOperator::Not, Value::Boolean(value)) => Ok(Value::Boolean(!value)),
                    (UnaryOperator::Negate, Value::Integer(value)) => value
                        .checked_neg()
                        .map(Value::Integer)
                        .ok_or_else(|| arithmetic_overflow(expression.location)),
                    (UnaryOperator::Not, value) => Err(type_error(
                        expression.location,
                        "operator `!`",
                        "a boolean",
                        &value,
                    )),
                    (UnaryOperator::Negate, value) => Err(type_error(
                        expression.location,
                        "unary `-`",
                        "an integer",
                        &value,
                    )),
                }
            }
            ExpressionKind::Binary(left, BinaryOperator::And, right) => {
                match self.evaluate(left)? {
                    Value::Boolean(false) => Ok(Value::Boolean(false)),
                    Value::Boolean(true) => self.boolean(right, expression.location, "&&"),
                    value => Err(type_error(
                        expression.location,
                        "operator `&&`",
                        "a boolean",
                        &value,
                    )),
                }
            }
            ExpressionKind::Binary(left, BinaryOperator::Or, right) => {
                match self.evaluate(left)? {
                    Value::Boolean(true) => Ok(Value::Boolean(true)),
                    Value::Boolean(false) => self.boolean(right, expression.location, "||"),
                    value => Err(type_error(
                        expression.location,
                        "operator `||`",
                        "a boolean",
                        &value,
                    )),
                }
            }
            ExpressionKind::Binary(left, operator, right) => {
                let left = self.evaluate(left)?;
                let right = self.evaluate(right)?;
                apply_binary(*operator, left, right, expression.location)
            }
        }
    }

    fn boolean(
        &mut self,
        expression: &Expression,
        location: SourceLocation,
        operator: &str,
    ) -> Result<Value, Diagnostic> {
        match self.evaluate(expression)? {
            Value::Boolean(value) => Ok(Value::Boolean(value)),
            value => Err(type_error(
                location,
                &format!("operator `{operator}`"),
                "a boolean",
                &value,
            )),
        }
    }

    fn resolve(&mut self, name: &str, location: SourceLocation) -> Result<Value, Diagnostic> {
        if let Some(value) = self.resolved.get(name) {
            return Ok(value.clone());
        }
        let Some(expression) = self.bindings.get(name) else {
            return Err(Diagnostic::new(
                location.line,
                location.column,
                format!("undefined variable `{name}`"),
                "declare this name in the file's `let:` block or quote it if it is literal text.",
            ));
        };
        if self.resolving.iter().any(|active| active == name) {
            return Err(Diagnostic::new(
                location.line,
                location.column,
                format!("recursive definition involving `{name}`"),
                "remove the circular reference between `let:` bindings.",
            ));
        }

        self.resolving.push(name.to_owned());
        let result = self.evaluate(expression);
        self.resolving.pop();
        let value = result?;
        self.resolved.insert(name.to_owned(), value.clone());
        Ok(value)
    }

    fn interpolate(&mut self, value: &str, location: SourceLocation) -> Result<String, Diagnostic> {
        let mut result = String::new();
        let mut rest = value;
        while let Some(start) = rest.find("${") {
            result.push_str(&rest[..start]);
            let after_open = &rest[start + 2..];
            let Some(end) = after_open.find('}') else {
                return Err(Diagnostic::new(
                    location.line,
                    location.column + start,
                    "string interpolation is missing `}`",
                    "close the variable reference, for example `${version}`.",
                ));
            };
            let name = &after_open[..end];
            let resolved = self.resolve(name, location)?;
            match resolved {
                Value::String(value) => result.push_str(&value),
                Value::Integer(value) => result.push_str(&value.to_string()),
                Value::Boolean(value) => result.push_str(if value { "true" } else { "false" }),
                _ => {
                    return Err(Diagnostic::new(
                        location.line,
                        location.column + start,
                        "only strings, integers, and booleans can be interpolated",
                        "convert this value to a scalar before using it in a string.",
                    ));
                }
            }
            rest = &after_open[end + 1..];
        }
        result.push_str(rest);
        Ok(result)
    }
}

fn apply_binary(
    operator: BinaryOperator,
    left: Value,
    right: Value,
    location: SourceLocation,
) -> Result<Value, Diagnostic> {
    let type_mismatch =
        |expected: &str, value: &Value| type_error(location, "binary operator", expected, value);
    match (operator, left, right) {
        (BinaryOperator::Equal, left, right) => Ok(Value::Boolean(values_equal(&left, &right))),
        (BinaryOperator::NotEqual, left, right) => Ok(Value::Boolean(!values_equal(&left, &right))),
        (BinaryOperator::Add, Value::Integer(left), Value::Integer(right)) => left
            .checked_add(right)
            .map(Value::Integer)
            .ok_or_else(|| arithmetic_overflow(location)),
        (BinaryOperator::Subtract, Value::Integer(left), Value::Integer(right)) => left
            .checked_sub(right)
            .map(Value::Integer)
            .ok_or_else(|| arithmetic_overflow(location)),
        (BinaryOperator::Multiply, Value::Integer(left), Value::Integer(right)) => left
            .checked_mul(right)
            .map(Value::Integer)
            .ok_or_else(|| arithmetic_overflow(location)),
        (BinaryOperator::Divide, Value::Integer(_), Value::Integer(0))
        | (BinaryOperator::Remainder, Value::Integer(_), Value::Integer(0)) => {
            Err(Diagnostic::new(
                location.line,
                location.column,
                "division by zero",
                "use a non-zero divisor.",
            ))
        }
        (BinaryOperator::Divide, Value::Integer(left), Value::Integer(right)) => left
            .checked_div(right)
            .map(Value::Integer)
            .ok_or_else(|| arithmetic_overflow(location)),
        (BinaryOperator::Remainder, Value::Integer(left), Value::Integer(right)) => left
            .checked_rem(right)
            .map(Value::Integer)
            .ok_or_else(|| arithmetic_overflow(location)),
        (BinaryOperator::Less, Value::Integer(left), Value::Integer(right)) => {
            Ok(Value::Boolean(left < right))
        }
        (BinaryOperator::LessEqual, Value::Integer(left), Value::Integer(right)) => {
            Ok(Value::Boolean(left <= right))
        }
        (BinaryOperator::Greater, Value::Integer(left), Value::Integer(right)) => {
            Ok(Value::Boolean(left > right))
        }
        (BinaryOperator::GreaterEqual, Value::Integer(left), Value::Integer(right)) => {
            Ok(Value::Boolean(left >= right))
        }
        (BinaryOperator::Append, Value::List(mut left), Value::List(right)) => {
            left.extend(right);
            Ok(Value::List(left))
        }
        (BinaryOperator::Less, left @ Value::String(_), right @ Value::String(_)) => {
            let (Value::String(left), Value::String(right)) = (left, right) else {
                unreachable!()
            };
            Ok(Value::Boolean(left < right))
        }
        (BinaryOperator::LessEqual, Value::String(left), Value::String(right)) => {
            Ok(Value::Boolean(left <= right))
        }
        (BinaryOperator::Greater, Value::String(left), Value::String(right)) => {
            Ok(Value::Boolean(left > right))
        }
        (BinaryOperator::GreaterEqual, Value::String(left), Value::String(right)) => {
            Ok(Value::Boolean(left >= right))
        }
        (BinaryOperator::Add, left, right)
        | (BinaryOperator::Subtract, left, right)
        | (BinaryOperator::Multiply, left, right)
        | (BinaryOperator::Divide, left, right)
        | (BinaryOperator::Remainder, left, right) => Err(type_mismatch(
            "integers on both sides",
            if !matches!(left, Value::Integer(_)) {
                &left
            } else {
                &right
            },
        )),
        (BinaryOperator::Append, left, right) => Err(type_mismatch(
            "lists on both sides of `++`",
            if !matches!(left, Value::List(_)) {
                &left
            } else {
                &right
            },
        )),
        (
            BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual,
            left,
            right,
        ) => Err(type_mismatch(
            "two integers or two strings",
            if std::mem::discriminant(&left) != std::mem::discriminant(&right) {
                &right
            } else {
                &left
            },
        )),
        (BinaryOperator::And | BinaryOperator::Or, _, _) => {
            unreachable!("boolean operators are handled before binary evaluation")
        }
    }
}

fn type_error(
    location: SourceLocation,
    operation: &str,
    expected: &str,
    value: &Value,
) -> Diagnostic {
    let actual = match value {
        Value::String(_) => "a string",
        Value::Integer(_) => "an integer",
        Value::Boolean(_) => "a boolean",
        Value::List(_) => "a list",
        Value::Map(_) => "a mapping",
    };
    Diagnostic::new(
        location.line,
        location.column,
        format!("{operation} expects {expected}, but got {actual}"),
        "change the value's type or use an operator that accepts it.",
    )
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Integer(left), Value::Integer(right)) => left == right,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::List(left), Value::List(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| values_equal(&left.value, &right.value))
        }
        (Value::Map(left), Value::Map(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, left_value)| {
                    right.get(key).is_some_and(|right_value| {
                        values_equal(&left_value.value, &right_value.value)
                    })
                })
        }
        _ => false,
    }
}

fn arithmetic_overflow(location: SourceLocation) -> Diagnostic {
    Diagnostic::new(
        location.line,
        location.column,
        "integer arithmetic overflow",
        "keep the result within the signed 64-bit integer range.",
    )
}

fn is_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(character) if character.is_ascii_alphabetic() || character == '_')
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        })
}

pub(crate) fn parse(source: &str, location: SourceLocation) -> Result<Expression, Diagnostic> {
    ExpressionParser::new(source, location)?.parse()
}

pub(crate) fn evaluate(
    expression: &Expression,
    bindings: &BTreeMap<String, Expression>,
) -> Result<Value, Diagnostic> {
    Evaluator::new(bindings).evaluate(expression)
}
