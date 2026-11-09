use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TokenKind {
    // Keywords
    Declare,
    Begin,
    Exception,
    When,
    Then,
    End,
    If,
    Elsif,
    Else,
    Loop,
    While,
    For,
    In,
    Reverse,
    Return,
    Null,
    Is,
    As,
    Procedure,
    Function,
    Package,
    Body,
    Raise,
    Select,
    Into,
    From,
    Where,
    And,
    Or,
    Not,
    Like,
    Constant,
    Cursor,
    Default,
    True,
    False,
    Others,

    // Identifiers & Binds
    Identifier(String),
    BindVariable(String), // Starts with ':', e.g. :BLOCK.ITEM

    // Literals
    StringLiteral(String),
    NumberLiteral(String),

    // Symbols & Operators
    Assign,             // :=
    Arrow,              // =>
    DotDot,             // ..
    Concat,             // ||
    Equal,              // =
    NotEqual,           // <> or !=
    LessThan,           // <
    LessThanOrEqual,    // <=
    GreaterThan,        // >
    GreaterThanOrEqual, // >=
    Plus,               // +
    Minus,              // -
    Star,               // *
    Slash,              // /
    OpenParen,          // (
    CloseParen,         // )
    Semicolon,          // ;
    Comma,              // ,
    Dot,                // .
    Percent,            // %
    Colon,              // :

    // Special
    Eof,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub line: usize,
    pub col: usize,
}

pub struct Lexer<'a> {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    _raw: &'a str,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let unescaped = Self::unescape_xml(input);
        let chars: Vec<char> = unescaped.chars().collect();
        Self {
            chars,
            pos: 0,
            line: 1,
            col: 1,
            _raw: input,
        }
    }

    fn unescape_xml(s: &str) -> String {
        s.replace("&amp;#10;", "\n")
            .replace("&amp;#13;", "\r")
            .replace("&#10;", "\n")
            .replace("&#13;", "\r")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        if let Some(&ch) = self.chars.get(self.pos) {
            self.pos += 1;
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            Some(ch)
        } else {
            None
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else if ch == '-' && self.peek_next() == Some('-') {
                // Line comment
                while let Some(c) = self.peek() {
                    self.advance();
                    if c == '\n' {
                        break;
                    }
                }
            } else if ch == '/' && self.peek_next() == Some('*') {
                // Block comment
                self.advance(); // /
                self.advance(); // *
                while let Some(c) = self.advance() {
                    if c == '*' && self.peek() == Some('/') {
                        self.advance();
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();

        let start_line = self.line;
        let start_col = self.col;

        let ch = match self.peek() {
            Some(c) => c,
            None => {
                return Token {
                    kind: TokenKind::Eof,
                    text: String::new(),
                    line: start_line,
                    col: start_col,
                }
            }
        };

        // 1. Bind Variable: :BLOCK.ITEM or :VAR
        if ch == ':' && self.peek_next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false) {
            self.advance(); // consume ':'
            let mut text = String::from(":");
            while let Some(c) = self.peek() {
                if c.is_alphanumeric() || c == '_' || c == '.' || c == '$' {
                    text.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
            return Token {
                kind: TokenKind::BindVariable(text.clone()),
                text,
                line: start_line,
                col: start_col,
            };
        }

        // 2. String literal: '...' (with '' escape)
        if ch == '\'' {
            self.advance(); // consume opening quote
            let mut text = String::new();
            while let Some(c) = self.advance() {
                if c == '\'' {
                    if self.peek() == Some('\'') {
                        // Escaped quote
                        self.advance();
                        text.push('\'');
                    } else {
                        break;
                    }
                } else {
                    text.push(c);
                }
            }
            return Token {
                kind: TokenKind::StringLiteral(text.clone()),
                text,
                line: start_line,
                col: start_col,
            };
        }

        // 3. Numeric literal
        if ch.is_ascii_digit() {
            let mut text = String::new();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || (c == '.' && self.peek_next() != Some('.')) {
                    text.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
            return Token {
                kind: TokenKind::NumberLiteral(text.clone()),
                text,
                line: start_line,
                col: start_col,
            };
        }

        // 4. Identifiers and Keywords
        if ch.is_alphabetic() || ch == '_' {
            let mut text = String::new();
            while let Some(c) = self.peek() {
                if c.is_alphanumeric() || c == '_' || c == '$' {
                    text.push(c);
                    self.advance();
                } else {
                    break;
                }
            }

            let kind = match text.to_ascii_uppercase().as_str() {
                "DECLARE" => TokenKind::Declare,
                "BEGIN" => TokenKind::Begin,
                "EXCEPTION" => TokenKind::Exception,
                "WHEN" => TokenKind::When,
                "THEN" => TokenKind::Then,
                "END" => TokenKind::End,
                "IF" => TokenKind::If,
                "ELSIF" => TokenKind::Elsif,
                "ELSE" => TokenKind::Else,
                "LOOP" => TokenKind::Loop,
                "WHILE" => TokenKind::While,
                "FOR" => TokenKind::For,
                "IN" => TokenKind::In,
                "REVERSE" => TokenKind::Reverse,
                "RETURN" => TokenKind::Return,
                "NULL" => TokenKind::Null,
                "IS" => TokenKind::Is,
                "AS" => TokenKind::As,
                "PROCEDURE" => TokenKind::Procedure,
                "FUNCTION" => TokenKind::Function,
                "PACKAGE" => TokenKind::Package,
                "BODY" => TokenKind::Body,
                "RAISE" => TokenKind::Raise,
                "SELECT" => TokenKind::Select,
                "INTO" => TokenKind::Into,
                "FROM" => TokenKind::From,
                "WHERE" => TokenKind::Where,
                "AND" => TokenKind::And,
                "OR" => TokenKind::Or,
                "NOT" => TokenKind::Not,
                "LIKE" => TokenKind::Like,
                "CONSTANT" => TokenKind::Constant,
                "CURSOR" => TokenKind::Cursor,
                "DEFAULT" => TokenKind::Default,
                "TRUE" => TokenKind::True,
                "FALSE" => TokenKind::False,
                "OTHERS" => TokenKind::Others,
                _ => TokenKind::Identifier(text.clone()),
            };

            return Token {
                kind,
                text,
                line: start_line,
                col: start_col,
            };
        }

        // 5. Symbols & Multi-character Operators
        self.advance();
        let (kind, text) = match ch {
            ':' if self.peek() == Some('=') => {
                self.advance();
                (TokenKind::Assign, ":=".to_string())
            }
            '=' if self.peek() == Some('>') => {
                self.advance();
                (TokenKind::Arrow, "=>".to_string())
            }
            '.' if self.peek() == Some('.') => {
                self.advance();
                (TokenKind::DotDot, "..".to_string())
            }
            '|' if self.peek() == Some('|') => {
                self.advance();
                (TokenKind::Concat, "||".to_string())
            }
            '<' if self.peek() == Some('>') => {
                self.advance();
                (TokenKind::NotEqual, "<>".to_string())
            }
            '!' if self.peek() == Some('=') => {
                self.advance();
                (TokenKind::NotEqual, "!=".to_string())
            }
            '<' if self.peek() == Some('=') => {
                self.advance();
                (TokenKind::LessThanOrEqual, "<=".to_string())
            }
            '>' if self.peek() == Some('=') => {
                self.advance();
                (TokenKind::GreaterThanOrEqual, ">=".to_string())
            }
            '<' => (TokenKind::LessThan, "<".to_string()),
            '>' => (TokenKind::GreaterThan, ">".to_string()),
            '=' => (TokenKind::Equal, "=".to_string()),
            '+' => (TokenKind::Plus, "+".to_string()),
            '-' => (TokenKind::Minus, "-".to_string()),
            '*' => (TokenKind::Star, "*".to_string()),
            '/' => (TokenKind::Slash, "/".to_string()),
            '(' => (TokenKind::OpenParen, "(".to_string()),
            ')' => (TokenKind::CloseParen, ")".to_string()),
            ';' => (TokenKind::Semicolon, ";".to_string()),
            ',' => (TokenKind::Comma, ",".to_string()),
            '.' => (TokenKind::Dot, ".".to_string()),
            '%' => (TokenKind::Percent, "%".to_string()),
            ':' => (TokenKind::Colon, ":".to_string()),
            _ => (TokenKind::Identifier(ch.to_string()), ch.to_string()),
        };

        Token {
            kind,
            text,
            line: start_line,
            col: start_col,
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            let is_eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        tokens
    }
}
