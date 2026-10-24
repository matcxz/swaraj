use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::{Diagnostic, Located, SourceLocation, Value};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Expression {
    kind: ExpressionKind,
    location: SourceLocation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionValue {
    parameter: String,
    body: Expression,
    captured_locals: Vec<BTreeMap<String, Value>>,
    captured_recursive_scopes: Vec<RecursiveScope>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExpressionKind {
    String(String),
    Integer(i64),
    Boolean(bool),
    Variable(String),
    List(Vec<Expression>),
    Map(Vec<(String, Expression)>),
    RecursiveMap(Vec<(String, Expression)>),
    Select(Box<Expression>, String),
    SelectOr(Box<Expression>, String, Box<Expression>),
    HasAttribute(Box<Expression>, String),
    Lambda(String, Box<Expression>),
    Apply(Box<Expression>, Box<Expression>),
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
    Merge,
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
    Merge,
    Bang,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Equal,
    Semicolon,
    Dot,
    Question,
    Colon,
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
                    if self.consume('/') {
                        TokenKind::Merge
                    } else {
                        TokenKind::Slash
                    }
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
                        TokenKind::Equal
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
                '{' => {
                    self.advance();
                    TokenKind::LeftBrace
                }
                '}' => {
                    self.advance();
                    TokenKind::RightBrace
                }
                ',' => {
                    self.advance();
                    TokenKind::Comma
                }
                ';' => {
                    self.advance();
                    TokenKind::Semicolon
                }
                '.' => {
                    self.advance();
                    TokenKind::Dot
                }
                '?' => {
                    self.advance();
                    TokenKind::Question
                }
                ':' => {
                    self.advance();
                    TokenKind::Colon
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
                    if self.advance() != Some('}') || !is_attribute_path(&name) {
                        return Err(Diagnostic::new(
                            interpolation_location.line,
                            interpolation_location.column,
                            "invalid string interpolation",
                            "interpolate a variable or attribute path, such as `${version}` or `${source.hash}`.",
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
            Some('a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '\'')
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
    parse_depth: usize,
}

const MAX_PARSE_DEPTH: usize = 64;

impl ExpressionParser {
    fn new(source: &str, location: SourceLocation) -> Result<Self, Diagnostic> {
        Ok(Self {
            tokens: Lexer::new(source, location).tokenize()?,
            position: 0,
            parse_depth: 0,
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
        if self.parse_depth >= MAX_PARSE_DEPTH {
            return Err(self.error(
                self.current().location,
                "maximum expression nesting depth exceeded",
                "reduce nested expressions or split them into simpler bindings.",
            ));
        }
        self.parse_depth += 1;
        let result = self.parse_precedence_inner(minimum);
        self.parse_depth -= 1;
        result
    }

    fn parse_precedence_inner(&mut self, minimum: u8) -> Result<Expression, Diagnostic> {
        let mut left = self.parse_prefix()?;
        loop {
            if matches!(self.current().kind, TokenKind::Dot | TokenKind::Question) {
                if 9 < minimum {
                    break;
                }
                let operator = self.advance();
                let attribute = self.advance();
                let TokenKind::Identifier(name) = attribute.kind else {
                    return Err(self.error(
                        attribute.location,
                        "expected an attribute name",
                        "write a field name after `.` or `?`.",
                    ));
                };
                left = Expression {
                    kind: if operator.kind == TokenKind::Dot {
                        if self.take_keyword("or") {
                            let fallback = self.parse_precedence(0)?;
                            ExpressionKind::SelectOr(Box::new(left), name, Box::new(fallback))
                        } else {
                            ExpressionKind::Select(Box::new(left), name)
                        }
                    } else {
                        ExpressionKind::HasAttribute(Box::new(left), name)
                    },
                    location: operator.location,
                };
                continue;
            }
            if 8 >= minimum && begins_expression(&self.current().kind) {
                let argument = self.parse_precedence(9)?;
                let location = left.location;
                left = Expression {
                    kind: ExpressionKind::Apply(Box::new(left), Box::new(argument)),
                    location,
                };
                continue;
            }
            let Some((operator, precedence)) = binary_operator(&self.current().kind) else {
                break;
            };
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
            TokenKind::Identifier(identifier)
                if identifier == "rec" && matches!(self.current().kind, TokenKind::LeftBrace) =>
            {
                self.advance();
                self.parse_map(token.location, true)
            }
            TokenKind::Identifier(identifier)
                if matches!(self.current().kind, TokenKind::Colon) =>
            {
                self.advance();
                let body = self.parse_precedence(0)?;
                Ok(Expression {
                    kind: ExpressionKind::Lambda(identifier, Box::new(body)),
                    location: token.location,
                })
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
            TokenKind::LeftBrace => self.parse_map(token.location, false),
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

    fn parse_map(
        &mut self,
        location: SourceLocation,
        recursive: bool,
    ) -> Result<Expression, Diagnostic> {
        let mut fields: Vec<(String, Expression)> = Vec::new();
        while !matches!(self.current().kind, TokenKind::RightBrace | TokenKind::End) {
            let key = self.advance();
            let TokenKind::Identifier(name) = key.kind else {
                return Err(self.error(
                    key.location,
                    "expected an attribute name",
                    "use `name = value;` inside an attribute set.",
                ));
            };
            if !matches!(self.current().kind, TokenKind::Equal) {
                return Err(self.error(
                    self.current().location,
                    "expected `=` after the attribute name",
                    "write an attribute assignment such as `version = \"1.0\";`.",
                ));
            }
            self.advance();
            let value = self.parse_precedence(0)?;
            if fields.iter().any(|(existing, _)| existing == &name) {
                return Err(self.error(
                    key.location,
                    "attribute is declared more than once",
                    "keep one assignment for each attribute name.",
                ));
            }
            fields.push((name, value));
            if !matches!(self.current().kind, TokenKind::Semicolon) {
                return Err(self.error(
                    self.current().location,
                    "expected `;` after the attribute value",
                    "terminate each attribute assignment with a semicolon.",
                ));
            }
            self.advance();
        }
        if !matches!(self.current().kind, TokenKind::RightBrace) {
            return Err(self.error(
                self.current().location,
                "attribute set is missing its closing `}`",
                "add `}` after the final attribute assignment.",
            ));
        }
        self.advance();
        Ok(Expression {
            kind: if recursive {
                ExpressionKind::RecursiveMap(fields)
            } else {
                ExpressionKind::Map(fields)
            },
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
        TokenKind::Merge => (BinaryOperator::Merge, 5),
        TokenKind::Plus => (BinaryOperator::Add, 6),
        TokenKind::Minus => (BinaryOperator::Subtract, 6),
        TokenKind::Star => (BinaryOperator::Multiply, 7),
        TokenKind::Slash => (BinaryOperator::Divide, 7),
        TokenKind::Percent => (BinaryOperator::Remainder, 7),
        _ => return None,
    })
}

fn begins_expression(kind: &TokenKind) -> bool {
    match kind {
        TokenKind::String(_)
        | TokenKind::Integer(_)
        | TokenKind::LeftParen
        | TokenKind::LeftBracket
        | TokenKind::LeftBrace => true,
        TokenKind::Identifier(name) => !matches!(name.as_str(), "then" | "else" | "or"),
        _ => false,
    }
}

pub(crate) struct Evaluator<'a> {
    bindings: &'a BTreeMap<String, Expression>,
    resolved: BTreeMap<String, Value>,
    resolving: Vec<String>,
    locals: Vec<BTreeMap<String, Value>>,
    recursive_scopes: Vec<RecursiveScope>,
    source_path: Option<PathBuf>,
    import_stack: Rc<RefCell<Vec<PathBuf>>>,
    evaluation_depth: usize,
}

const MAX_EVALUATION_DEPTH: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecursiveScope {
    definitions: BTreeMap<String, Expression>,
    resolved: BTreeMap<String, Value>,
    resolving: Vec<String>,
}

impl<'a> Evaluator<'a> {
    pub(crate) fn new(
        bindings: &'a BTreeMap<String, Expression>,
        source_path: Option<&Path>,
        import_stack: Rc<RefCell<Vec<PathBuf>>>,
    ) -> Self {
        Self {
            bindings,
            resolved: BTreeMap::new(),
            resolving: Vec::new(),
            locals: Vec::new(),
            recursive_scopes: Vec::new(),
            source_path: source_path.map(Path::to_path_buf),
            import_stack,
            evaluation_depth: 0,
        }
    }

    pub(crate) fn evaluate(&mut self, expression: &Expression) -> Result<Value, Diagnostic> {
        if self.evaluation_depth >= MAX_EVALUATION_DEPTH {
            return Err(Diagnostic::new(
                expression.location.line,
                expression.location.column,
                "maximum expression evaluation depth exceeded",
                "reduce recursive calls or deeply nested expressions.",
            ));
        }
        self.evaluation_depth += 1;
        let result = self.evaluate_inner(expression);
        self.evaluation_depth -= 1;
        result
    }

    fn evaluate_inner(&mut self, expression: &Expression) -> Result<Value, Diagnostic> {
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
            ExpressionKind::Map(fields) => {
                let mut result = BTreeMap::new();
                for (name, field) in fields {
                    result.insert(
                        name.clone(),
                        Located {
                            value: self.evaluate(field)?,
                            location: field.location,
                        },
                    );
                }
                Ok(Value::Map(result))
            }
            ExpressionKind::RecursiveMap(fields) => {
                let mut definitions = BTreeMap::new();
                for (name, expression) in fields {
                    if definitions
                        .insert(name.clone(), expression.clone())
                        .is_some()
                    {
                        return Err(Diagnostic::new(
                            expression.location.line,
                            expression.location.column,
                            format!("attribute `{name}` is declared more than once"),
                            "keep one assignment for each attribute name.",
                        ));
                    }
                }
                self.recursive_scopes.push(RecursiveScope {
                    definitions,
                    resolved: BTreeMap::new(),
                    resolving: Vec::new(),
                });
                let names = self
                    .recursive_scopes
                    .last()
                    .expect("recursive scope was just added")
                    .definitions
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>();
                let result = names
                    .into_iter()
                    .map(|name| {
                        let value = self.resolve(&name, expression.location)?;
                        let field_location = self
                            .recursive_scopes
                            .last()
                            .and_then(|scope| scope.definitions.get(&name))
                            .map_or(expression.location, |field| field.location);
                        Ok((
                            name,
                            Located {
                                value,
                                location: field_location,
                            },
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, Diagnostic>>();
                self.recursive_scopes.pop();
                result.map(Value::Map)
            }
            ExpressionKind::Select(map, name) => {
                if let Some(fields) = self.recursive_map_fields(map, &mut Vec::new()) {
                    return self.evaluate_recursive_attribute(&fields, name, expression.location);
                }
                let value = self.evaluate(map)?;
                select_attribute(value, name, expression.location)
            }
            ExpressionKind::SelectOr(map, name, fallback) => {
                if let Some(fields) = self.recursive_map_fields(map, &mut Vec::new()) {
                    return if fields.iter().any(|(field, _)| field == name) {
                        self.evaluate_recursive_attribute(&fields, name, expression.location)
                    } else {
                        self.evaluate(fallback)
                    };
                }
                let value = self.evaluate(map)?;
                match value {
                    Value::Map(fields) => match fields.get(name) {
                        Some(value) => Ok(value.value.clone()),
                        None => self.evaluate(fallback),
                    },
                    other => Err(type_error(
                        expression.location,
                        "attribute selection",
                        "a mapping",
                        &other,
                    )),
                }
            }
            ExpressionKind::HasAttribute(map, name) => {
                if let Some(fields) = self.recursive_map_fields(map, &mut Vec::new()) {
                    return Ok(Value::Boolean(
                        fields.iter().any(|(field, _)| field == name),
                    ));
                }
                let value = self.evaluate(map)?;
                match value {
                    Value::Map(fields) => Ok(Value::Boolean(fields.contains_key(name))),
                    other => Err(type_error(
                        expression.location,
                        "attribute existence check",
                        "a mapping",
                        &other,
                    )),
                }
            }
            ExpressionKind::Lambda(parameter, body) => {
                let mut captured_recursive_scopes = self.recursive_scopes.clone();
                for scope in &mut captured_recursive_scopes {
                    scope.resolving.clear();
                }
                Ok(Value::Function(FunctionValue {
                    parameter: parameter.clone(),
                    body: (**body).clone(),
                    captured_locals: self.locals.clone(),
                    captured_recursive_scopes,
                }))
            }
            ExpressionKind::Apply(_, _) => {
                let mut arguments = Vec::new();
                let head = collect_application(expression, &mut arguments);
                arguments.reverse();
                if let Some(name) = builtin_name(head) {
                    return self.apply_builtin_expression(name, &arguments, expression.location);
                }
                let function = self.evaluate(head)?;
                self.apply_function_expressions(function, &arguments, expression.location)
            }
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
        if let Some(value) = self.locals.iter().rev().find_map(|scope| scope.get(name)) {
            return Ok(value.clone());
        }
        if let Some(scope_index) = self
            .recursive_scopes
            .iter()
            .rposition(|scope| scope.definitions.contains_key(name))
        {
            return self.resolve_recursive(scope_index, name, location);
        }
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
        let caller_locals = std::mem::take(&mut self.locals);
        let caller_recursive_scopes = std::mem::take(&mut self.recursive_scopes);
        let result = self.evaluate(expression);
        self.locals = caller_locals;
        self.recursive_scopes = caller_recursive_scopes;
        self.resolving.pop();
        let value = result?;
        self.resolved.insert(name.to_owned(), value.clone());
        Ok(value)
    }

    fn resolve_recursive(
        &mut self,
        scope_index: usize,
        name: &str,
        location: SourceLocation,
    ) -> Result<Value, Diagnostic> {
        if let Some(value) = self.recursive_scopes[scope_index].resolved.get(name) {
            return Ok(value.clone());
        }
        if self.recursive_scopes[scope_index]
            .resolving
            .iter()
            .any(|active| active == name)
        {
            return Err(Diagnostic::new(
                location.line,
                location.column,
                format!("recursive attribute definition involving `{name}`"),
                "remove the circular reference between attributes.",
            ));
        }
        let expression = self.recursive_scopes[scope_index]
            .definitions
            .get(name)
            .cloned()
            .expect("recursive attribute exists");
        self.recursive_scopes[scope_index]
            .resolving
            .push(name.to_owned());
        let result = self.evaluate(&expression);
        self.recursive_scopes[scope_index].resolving.pop();
        let value = result?;
        self.recursive_scopes[scope_index]
            .resolved
            .insert(name.to_owned(), value.clone());
        Ok(value)
    }

    fn recursive_map_fields(
        &self,
        expression: &Expression,
        seen: &mut Vec<String>,
    ) -> Option<Vec<(String, Expression)>> {
        match &expression.kind {
            ExpressionKind::RecursiveMap(fields) => Some(fields.clone()),
            ExpressionKind::Variable(name) if !seen.iter().any(|item| item == name) => {
                seen.push(name.clone());
                self.bindings
                    .get(name)
                    .and_then(|binding| self.recursive_map_fields(binding, seen))
            }
            _ => None,
        }
    }

    fn evaluate_recursive_attribute(
        &mut self,
        fields: &[(String, Expression)],
        name: &str,
        location: SourceLocation,
    ) -> Result<Value, Diagnostic> {
        let mut definitions = BTreeMap::new();
        for (field_name, field) in fields {
            if definitions
                .insert(field_name.clone(), field.clone())
                .is_some()
            {
                return Err(Diagnostic::new(
                    field.location.line,
                    field.location.column,
                    format!("attribute `{field_name}` is declared more than once"),
                    "keep one assignment for each attribute name.",
                ));
            }
        }
        if !definitions.contains_key(name) {
            return Err(Diagnostic::new(
                location.line,
                location.column,
                format!("attribute `{name}` does not exist"),
                "check the attribute name or provide it in the attribute set.",
            ));
        }
        self.recursive_scopes.push(RecursiveScope {
            definitions,
            resolved: BTreeMap::new(),
            resolving: Vec::new(),
        });
        let result = self.resolve(name, location);
        self.recursive_scopes.pop();
        result
    }

    fn apply_builtin_expression(
        &mut self,
        name: &str,
        arguments: &[&Expression],
        location: SourceLocation,
    ) -> Result<Value, Diagnostic> {
        match (name, arguments) {
            ("map", [function, values]) | ("filter", [function, values]) => {
                let values = self.evaluate(values)?;
                let Value::List(values) = values else {
                    return Err(type_error(
                        location,
                        &format!("builtins.{name}"),
                        "a list as the second argument",
                        &values,
                    ));
                };
                let mut result = Vec::with_capacity(values.len());
                for item in values {
                    let output = self.apply_function(function, item.value.clone(), location)?;
                    if name == "filter" {
                        match output {
                            Value::Boolean(true) => result.push(item),
                            Value::Boolean(false) => {}
                            other => {
                                return Err(type_error(
                                    location,
                                    "builtins.filter function",
                                    "a boolean",
                                    &other,
                                ));
                            }
                        }
                    } else {
                        result.push(Located {
                            value: output,
                            location: item.location,
                        });
                    }
                }
                Ok(Value::List(result))
            }
            ("genList", [function, length]) => {
                let length = self.evaluate(length)?;
                let Value::Integer(length) = length else {
                    return Err(type_error(
                        location,
                        "builtins.genList",
                        "an integer length",
                        &length,
                    ));
                };
                let length = usize::try_from(length).map_err(|_| {
                    Diagnostic::new(
                        location.line,
                        location.column,
                        "builtins.genList requires a non-negative length",
                        "use a length between zero and the platform list-size limit.",
                    )
                })?;
                if length > 1_000_000 {
                    return Err(Diagnostic::new(
                        location.line,
                        location.column,
                        "builtins.genList length exceeds the limit of 1000000",
                        "use a smaller list length.",
                    ));
                }
                (0..length)
                    .map(|index| {
                        let index =
                            i64::try_from(index).map_err(|_| arithmetic_overflow(location))?;
                        Ok(Located {
                            value: self.apply_function(
                                function,
                                Value::Integer(index),
                                location,
                            )?,
                            location,
                        })
                    })
                    .collect::<Result<Vec<_>, Diagnostic>>()
                    .map(Value::List)
            }
            ("foldl'", [function, initial, values]) => {
                let mut accumulator = self.evaluate(initial)?;
                let values = self.evaluate(values)?;
                let Value::List(values) = values else {
                    return Err(type_error(
                        location,
                        "builtins.foldl'",
                        "a list as the third argument",
                        &values,
                    ));
                };
                for value in values {
                    accumulator = self.apply_function_values(
                        function,
                        &[accumulator, value.value],
                        location,
                    )?;
                }
                Ok(accumulator)
            }
            ("concatLists", [values]) => match self.evaluate(values)? {
                Value::List(values) => {
                    let mut result = Vec::new();
                    for item in values {
                        match item.value {
                            Value::List(items) => result.extend(items),
                            other => {
                                return Err(type_error(
                                    location,
                                    "builtins.concatLists",
                                    "a list of lists",
                                    &other,
                                ));
                            }
                        }
                    }
                    Ok(Value::List(result))
                }
                other => Err(type_error(
                    location,
                    "builtins.concatLists",
                    "a list of lists",
                    &other,
                )),
            },
            ("import", [path]) => {
                let path = self.evaluate(path)?;
                let Value::String(path) = path else {
                    return Err(type_error(
                        location,
                        "builtins.import",
                        "a string path",
                        &path,
                    ));
                };
                crate::import_package(
                    self.source_path.as_deref(),
                    self.import_stack.clone(),
                    &path,
                    location,
                )
            }
            ("readFile", [path]) | ("pathExists", [path]) => {
                let path = self.evaluate(path)?;
                let Value::String(path) = path else {
                    return Err(type_error(
                        location,
                        &format!("builtins.{name}"),
                        "a string path",
                        &path,
                    ));
                };
                let path = self.resolve_path(&path, location)?;
                if name == "pathExists" {
                    path.try_exists().map(Value::Boolean).map_err(|error| {
                        Diagnostic::new(
                            location.line,
                            location.column,
                            format!("could not check path `{}`: {error}", path.display()),
                            "check the path and its permissions.",
                        )
                    })
                } else {
                    std::fs::read_to_string(&path)
                        .map(Value::String)
                        .map_err(|error| {
                            Diagnostic::new(
                                location.line,
                                location.column,
                                format!("could not read file `{}`: {error}", path.display()),
                                "check that the file exists, is readable, and contains UTF-8 text.",
                            )
                        })
                }
            }
            ("listToAttrs", [values]) => match self.evaluate(values)? {
                Value::List(values) => {
                    let mut result = BTreeMap::new();
                    for item in values {
                        let Value::Map(attributes) = item.value else {
                            return Err(type_error(
                                location,
                                "builtins.listToAttrs",
                                "a list of attribute sets with `name` and `value`",
                                &item.value,
                            ));
                        };
                        let name = attributes.get("name").ok_or_else(|| {
                            Diagnostic::new(
                                location.line,
                                location.column,
                                "builtins.listToAttrs item is missing `name`",
                                "include a string `name` attribute in every item.",
                            )
                        })?;
                        let Value::String(name) = &name.value else {
                            return Err(type_error(
                                location,
                                "builtins.listToAttrs item `name`",
                                "a string",
                                &name.value,
                            ));
                        };
                        let value = attributes.get("value").ok_or_else(|| {
                            Diagnostic::new(
                                location.line,
                                location.column,
                                "builtins.listToAttrs item is missing `value`",
                                "include a `value` attribute in every item.",
                            )
                        })?;
                        result.insert(name.clone(), value.clone());
                    }
                    Ok(Value::Map(result))
                }
                other => Err(type_error(
                    location,
                    "builtins.listToAttrs",
                    "a list",
                    &other,
                )),
            },
            ("mapAttrs", [function, attributes]) => match self.evaluate(attributes)? {
                Value::Map(attributes) => {
                    let mut result = BTreeMap::new();
                    for (name, value) in attributes {
                        let mapped = self.apply_function_values(
                            function,
                            &[Value::String(name.clone()), value.value],
                            location,
                        )?;
                        result.insert(
                            name,
                            Located {
                                value: mapped,
                                location: value.location,
                            },
                        );
                    }
                    Ok(Value::Map(result))
                }
                other => Err(type_error(
                    location,
                    "builtins.mapAttrs",
                    "an attribute set",
                    &other,
                )),
            },
            ("filterAttrs", [function, attributes]) => match self.evaluate(attributes)? {
                Value::Map(attributes) => {
                    let mut result = BTreeMap::new();
                    for (name, value) in attributes {
                        match self.apply_function_values(
                            function,
                            &[Value::String(name.clone()), value.value.clone()],
                            location,
                        )? {
                            Value::Boolean(true) => {
                                result.insert(name, value);
                            }
                            Value::Boolean(false) => {}
                            other => {
                                return Err(type_error(
                                    location,
                                    "builtins.filterAttrs function",
                                    "a boolean",
                                    &other,
                                ));
                            }
                        }
                    }
                    Ok(Value::Map(result))
                }
                other => Err(type_error(
                    location,
                    "builtins.filterAttrs",
                    "an attribute set",
                    &other,
                )),
            },
            ("zipListsWith", [function, left, right]) => {
                let left = self.evaluate(left)?;
                let right = self.evaluate(right)?;
                let (Value::List(left), Value::List(right)) = (left, right) else {
                    return Err(Diagnostic::new(
                        location.line,
                        location.column,
                        "builtins.zipListsWith expects two lists",
                        "pass a list as both the second and third argument.",
                    ));
                };
                left.into_iter()
                    .zip(right)
                    .map(|(left, right)| {
                        Ok(Located {
                            value: self.apply_function_values(
                                function,
                                &[left.value, right.value],
                                location,
                            )?,
                            location: left.location,
                        })
                    })
                    .collect::<Result<Vec<_>, Diagnostic>>()
                    .map(Value::List)
            }
            ("concatStringsSep" | "elem" | "hasAttr" | "getAttr", [first, second]) => {
                let first = self.evaluate(first)?;
                let second = self.evaluate(second)?;
                apply_binary_builtin(name, first, second, location)
            }
            ("substring" | "replaceStrings", [first, second, third]) => {
                let first = self.evaluate(first)?;
                let second = self.evaluate(second)?;
                let third = self.evaluate(third)?;
                if name == "substring" {
                    substring(first, second, third, location)
                } else {
                    replace_strings(first, second, third, location)
                }
            }
            ("seq", [first, second]) | ("deepSeq", [first, second]) => {
                self.evaluate(first)?;
                self.evaluate(second)
            }
            (
                "length" | "head" | "tail" | "attrNames" | "attrValues" | "toString" | "isString"
                | "isInt" | "isBool" | "isList" | "isAttrs" | "isFunction" | "typeOf",
                [argument],
            ) => apply_builtin(name, self.evaluate(argument)?, location),
            ("stringLength" | "baseNameOf" | "dirOf", [argument]) => {
                apply_builtin(name, self.evaluate(argument)?, location)
            }
            ("map" | "filter" | "genList" | "mapAttrs" | "filterAttrs", _) => {
                Err(arity_error(location, name, 2))
            }
            ("listToAttrs" | "concatLists", _) => Err(arity_error(location, name, 1)),
            ("foldl'", _) => Err(arity_error(location, name, 3)),
            (
                "length" | "head" | "tail" | "attrNames" | "attrValues" | "toString" | "isString"
                | "isInt" | "isBool" | "isList" | "isAttrs" | "isFunction" | "typeOf"
                | "stringLength" | "baseNameOf" | "dirOf",
                _,
            ) => Err(arity_error(location, name, 1)),
            ("concatStringsSep" | "elem" | "hasAttr" | "getAttr", _) => {
                Err(arity_error(location, name, 2))
            }
            ("zipListsWith", _) => Err(arity_error(location, name, 3)),
            ("substring" | "replaceStrings", _) => Err(arity_error(location, name, 3)),
            ("import", _) => Err(arity_error(location, name, 1)),
            ("readFile" | "pathExists", _) => Err(arity_error(location, name, 1)),
            ("seq" | "deepSeq", _) => Err(arity_error(location, name, 2)),
            _ => Err(Diagnostic::new(
                location.line,
                location.column,
                format!("unknown built-in `{name}`"),
                "use a supported function from the `builtins` namespace.",
            )),
        }
    }

    fn apply_function(
        &mut self,
        function: &Expression,
        argument: Value,
        location: SourceLocation,
    ) -> Result<Value, Diagnostic> {
        self.apply_function_values(function, &[argument], location)
    }

    fn resolve_path(&self, path: &str, location: SourceLocation) -> Result<PathBuf, Diagnostic> {
        let Some(source_path) = &self.source_path else {
            return Err(Diagnostic::new(
                location.line,
                location.column,
                format!("builtins path `{path}` requires a source file path"),
                "use `parse_with_path` or the `swaraj check` command for file operations.",
            ));
        };
        let candidate = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            source_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(path)
        };
        Ok(candidate)
    }

    fn apply_function_values(
        &mut self,
        function: &Expression,
        arguments: &[Value],
        location: SourceLocation,
    ) -> Result<Value, Diagnostic> {
        let mut function = self.evaluate(function)?;
        for argument in arguments {
            function = self.apply_function_value(function, argument.clone(), location)?;
        }
        Ok(function)
    }

    fn apply_function_expressions(
        &mut self,
        function: Value,
        arguments: &[&Expression],
        location: SourceLocation,
    ) -> Result<Value, Diagnostic> {
        if !matches!(&function, Value::Function(_)) {
            return Err(type_error(
                location,
                "function application",
                "a function",
                &function,
            ));
        }
        let mut function = function;
        for argument in arguments {
            let argument = self.evaluate(argument)?;
            function = self.apply_function_value(function, argument, location)?;
        }
        Ok(function)
    }

    fn apply_function_value(
        &mut self,
        function: Value,
        argument: Value,
        location: SourceLocation,
    ) -> Result<Value, Diagnostic> {
        let Value::Function(function) = function else {
            return Err(type_error(
                location,
                "function application",
                "a function",
                &function,
            ));
        };
        let caller_locals = std::mem::replace(&mut self.locals, function.captured_locals);
        let caller_recursive_scopes = std::mem::replace(
            &mut self.recursive_scopes,
            function.captured_recursive_scopes,
        );
        self.locals
            .push(BTreeMap::from([(function.parameter, argument)]));
        let result = self.evaluate(&function.body);
        self.locals = caller_locals;
        self.recursive_scopes = caller_recursive_scopes;
        result
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
            let path = &after_open[..end];
            let mut segments = path.split('.');
            let name = segments.next().unwrap_or_default();
            let mut resolved = self.resolve(name, location)?;
            for segment in segments {
                resolved = select_attribute(resolved, segment, location)?;
            }
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
        (BinaryOperator::Merge, Value::Map(mut left), Value::Map(right)) => {
            left.extend(right);
            Ok(Value::Map(left))
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
        (BinaryOperator::Merge, left, right) => Err(type_mismatch(
            "mappings on both sides of `//`",
            if !matches!(left, Value::Map(_)) {
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
        Value::Function(_) => "a function",
    };
    Diagnostic::new(
        location.line,
        location.column,
        format!("{operation} expects {expected}, but got {actual}"),
        "change the value's type or use an operator that accepts it.",
    )
}

fn select_attribute(
    value: Value,
    name: &str,
    location: SourceLocation,
) -> Result<Value, Diagnostic> {
    match value {
        Value::Map(fields) => fields
            .get(name)
            .map(|field| field.value.clone())
            .ok_or_else(|| {
                Diagnostic::new(
                    location.line,
                    location.column,
                    format!("attribute `{name}` does not exist"),
                    "check the attribute name or provide it in the attribute set.",
                )
            }),
        other => Err(type_error(
            location,
            "attribute selection",
            "a mapping",
            &other,
        )),
    }
}

fn builtin_name(expression: &Expression) -> Option<&str> {
    match &expression.kind {
        ExpressionKind::Select(namespace, name) if matches!(&namespace.kind, ExpressionKind::Variable(value) if value == "builtins") => {
            Some(name)
        }
        _ => None,
    }
}

fn collect_application<'a>(
    expression: &'a Expression,
    arguments: &mut Vec<&'a Expression>,
) -> &'a Expression {
    match &expression.kind {
        ExpressionKind::Apply(function, argument) => {
            arguments.push(argument);
            collect_application(function, arguments)
        }
        _ => expression,
    }
}

fn arity_error(location: SourceLocation, name: &str, expected: usize) -> Diagnostic {
    Diagnostic::new(
        location.line,
        location.column,
        format!("builtins.{name} expects {expected} argument(s)"),
        "pass the required number of arguments.",
    )
}

fn apply_builtin(
    name: &str,
    argument: Value,
    location: SourceLocation,
) -> Result<Value, Diagnostic> {
    match (name, argument) {
        ("isString", value) => Ok(Value::Boolean(matches!(value, Value::String(_)))),
        ("isInt", value) => Ok(Value::Boolean(matches!(value, Value::Integer(_)))),
        ("isBool", value) => Ok(Value::Boolean(matches!(value, Value::Boolean(_)))),
        ("isList", value) => Ok(Value::Boolean(matches!(value, Value::List(_)))),
        ("isAttrs", value) => Ok(Value::Boolean(matches!(value, Value::Map(_)))),
        ("isFunction", value) => Ok(Value::Boolean(matches!(value, Value::Function(_)))),
        ("typeOf", value) => Ok(Value::String(
            match value {
                Value::String(_) => "string",
                Value::Integer(_) => "int",
                Value::Boolean(_) => "bool",
                Value::List(_) => "list",
                Value::Map(_) => "set",
                Value::Function(_) => "lambda",
            }
            .to_owned(),
        )),
        ("length", Value::List(values)) => i64::try_from(values.len())
            .map(Value::Integer)
            .map_err(|_| arithmetic_overflow(location)),
        ("head", Value::List(values)) => values
            .first()
            .map(|value| value.value.clone())
            .ok_or_else(|| {
                Diagnostic::new(
                    location.line,
                    location.column,
                    "builtins.head received an empty list",
                    "pass a list containing at least one element.",
                )
            }),
        ("tail", Value::List(values)) => {
            if values.is_empty() {
                Err(Diagnostic::new(
                    location.line,
                    location.column,
                    "builtins.tail received an empty list",
                    "pass a list containing at least one element.",
                ))
            } else {
                Ok(Value::List(values.into_iter().skip(1).collect()))
            }
        }
        ("attrNames", Value::Map(fields)) => Ok(Value::List(
            fields
                .keys()
                .map(|name| Located {
                    value: Value::String(name.clone()),
                    location,
                })
                .collect(),
        )),
        ("attrValues", Value::Map(fields)) => Ok(Value::List(fields.into_values().collect())),
        ("stringLength", Value::String(value)) => i64::try_from(value.chars().count())
            .map(Value::Integer)
            .map_err(|_| arithmetic_overflow(location)),
        ("baseNameOf", Value::String(value)) => Ok(Value::String(
            Path::new(&value)
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        )),
        ("dirOf", Value::String(value)) => {
            let parent = Path::new(&value)
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            Ok(Value::String(parent.to_string_lossy().into_owned()))
        }
        ("toString", value @ Value::String(_)) => Ok(value),
        ("toString", Value::Integer(value)) => Ok(Value::String(value.to_string())),
        ("toString", Value::Boolean(value)) => {
            Ok(Value::String(if value { "1" } else { "" }.to_owned()))
        }
        ("toString", Value::List(values)) => Ok(Value::String(
            values
                .iter()
                .map(|value| match &value.value {
                    Value::String(value) => value.clone(),
                    Value::Integer(value) => value.to_string(),
                    Value::Boolean(value) => {
                        if *value {
                            "1".to_owned()
                        } else {
                            String::new()
                        }
                    }
                    Value::List(_) | Value::Map(_) | Value::Function(_) => String::new(),
                })
                .collect::<Vec<_>>()
                .join(" "),
        )),
        ("length" | "head" | "tail", value) => Err(type_error(
            location,
            &format!("builtins.{name}"),
            "a list",
            &value,
        )),
        ("attrNames", value) => Err(type_error(
            location,
            "builtins.attrNames",
            "a mapping",
            &value,
        )),
        ("attrValues", value) => Err(type_error(
            location,
            "builtins.attrValues",
            "a mapping",
            &value,
        )),
        ("stringLength" | "baseNameOf" | "dirOf", value) => Err(type_error(
            location,
            &format!("builtins.{name}"),
            "a string",
            &value,
        )),
        ("toString", Value::Map(_)) => Err(Diagnostic::new(
            location.line,
            location.column,
            "builtins.toString cannot convert an attribute set",
            "convert one of the set's scalar attributes instead.",
        )),
        _ => Err(Diagnostic::new(
            location.line,
            location.column,
            format!("unknown built-in `{name}`"),
            "use a supported function from the `builtins` namespace.",
        )),
    }
}

fn substring(
    start: Value,
    length: Value,
    string: Value,
    location: SourceLocation,
) -> Result<Value, Diagnostic> {
    let (Value::Integer(start), Value::Integer(length), Value::String(string)) =
        (start, length, string)
    else {
        return Err(Diagnostic::new(
            location.line,
            location.column,
            "builtins.substring expects two integers and a string",
            "use `builtins.substring start length string`.",
        ));
    };
    let start = usize::try_from(start).map_err(|_| {
        Diagnostic::new(
            location.line,
            location.column,
            "builtins.substring start must be non-negative",
            "use a start index of zero or greater.",
        )
    })?;
    let length = usize::try_from(length).map_err(|_| {
        Diagnostic::new(
            location.line,
            location.column,
            "builtins.substring length must be non-negative",
            "use a length of zero or greater.",
        )
    })?;
    Ok(Value::String(
        string.chars().skip(start).take(length).collect(),
    ))
}

fn replace_strings(
    old: Value,
    new: Value,
    string: Value,
    location: SourceLocation,
) -> Result<Value, Diagnostic> {
    let (Value::List(old), Value::List(new), Value::String(string)) = (old, new, string) else {
        return Err(Diagnostic::new(
            location.line,
            location.column,
            "builtins.replaceStrings expects two lists and a string",
            "use `builtins.replaceStrings oldStrings newStrings input`.",
        ));
    };
    if old.len() != new.len() {
        return Err(Diagnostic::new(
            location.line,
            location.column,
            "builtins.replaceStrings lists must have equal lengths",
            "provide one replacement string for each search string.",
        ));
    }
    let mut replacements = Vec::with_capacity(old.len());
    for (old, new) in old.iter().zip(new) {
        let (Value::String(old), Value::String(new)) = (&old.value, &new.value) else {
            let invalid = if !matches!(&old.value, Value::String(_)) {
                &old.value
            } else {
                &new.value
            };
            return Err(type_error(
                location,
                "builtins.replaceStrings",
                "lists containing only strings",
                invalid,
            ));
        };
        if !old.is_empty() {
            replacements.push((old.clone(), new.clone()));
        }
    }

    let mut result = String::new();
    let mut rest = string.as_str();
    while !rest.is_empty() {
        if let Some((old, new)) = replacements.iter().find(|(old, _)| rest.starts_with(old)) {
            result.push_str(new);
            rest = &rest[old.len()..];
        } else {
            let character = rest.chars().next().expect("non-empty string");
            result.push(character);
            rest = &rest[character.len_utf8()..];
        }
    }
    Ok(Value::String(result))
}

fn apply_binary_builtin(
    name: &str,
    first: Value,
    second: Value,
    location: SourceLocation,
) -> Result<Value, Diagnostic> {
    match (name, first, second) {
        ("concatStringsSep", Value::String(separator), Value::List(values)) => {
            let mut strings = Vec::with_capacity(values.len());
            for value in values {
                let Value::String(value) = value.value else {
                    return Err(type_error(
                        location,
                        "builtins.concatStringsSep",
                        "a list of strings",
                        &value.value,
                    ));
                };
                strings.push(value);
            }
            Ok(Value::String(strings.join(&separator)))
        }
        ("hasAttr", Value::String(name), Value::Map(fields)) => {
            Ok(Value::Boolean(fields.contains_key(&name)))
        }
        ("getAttr", Value::String(name), Value::Map(fields)) => fields
            .get(&name)
            .map(|value| value.value.clone())
            .ok_or_else(|| {
                Diagnostic::new(
                    location.line,
                    location.column,
                    format!("attribute `{name}` does not exist"),
                    "check the attribute name or provide it in the attribute set.",
                )
            }),
        ("elem", value, Value::List(values)) => Ok(Value::Boolean(
            values.iter().any(|item| values_equal(&value, &item.value)),
        )),
        ("intersectAttrs", Value::Map(first), Value::Map(second)) => Ok(Value::Map(
            first
                .into_iter()
                .filter(|(key, _)| second.contains_key(key))
                .collect(),
        )),
        ("removeAttrs", Value::Map(mut attributes), Value::List(names)) => {
            for name in names {
                match name.value {
                    Value::String(name) => {
                        attributes.remove(&name);
                    }
                    other => {
                        return Err(type_error(
                            location,
                            "builtins.removeAttrs",
                            "a list of strings",
                            &other,
                        ));
                    }
                }
            }
            Ok(Value::Map(attributes))
        }
        ("concatStringsSep", separator, _) if !matches!(&separator, Value::String(_)) => {
            Err(type_error(
                location,
                "builtins.concatStringsSep",
                "a string separator",
                &separator,
            ))
        }
        ("concatStringsSep", Value::String(_), values) => Err(type_error(
            location,
            "builtins.concatStringsSep",
            "a list of strings",
            &values,
        )),
        ("hasAttr" | "getAttr", name, _) if !matches!(&name, Value::String(_)) => Err(type_error(
            location,
            "builtins.hasAttr or builtins.getAttr",
            "a string attribute name",
            &name,
        )),
        ("hasAttr" | "getAttr", Value::String(_), attributes) => Err(type_error(
            location,
            "builtins.hasAttr or builtins.getAttr",
            "a mapping as the second argument",
            &attributes,
        )),
        ("elem", _, list) => Err(type_error(
            location,
            "builtins.elem",
            "a list as its second argument",
            &list,
        )),
        ("intersectAttrs" | "removeAttrs", attributes, _) => Err(type_error(
            location,
            &format!("builtins.{name}"),
            "a mapping as the first argument",
            &attributes,
        )),
        _ => Err(Diagnostic::new(
            location.line,
            location.column,
            format!("unknown binary built-in `{name}`"),
            "use `concatStringsSep`, `hasAttr`, `getAttr`, or `elem`.",
        )),
    }
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
        (Value::Function(_), Value::Function(_)) => false,
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

fn is_attribute_path(value: &str) -> bool {
    !value.is_empty() && value.split('.').all(is_identifier)
}

pub(crate) fn parse(source: &str, location: SourceLocation) -> Result<Expression, Diagnostic> {
    ExpressionParser::new(source, location)?.parse()
}
