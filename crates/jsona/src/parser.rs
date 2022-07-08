//! JSONA document to syntax tree parsing.

use crate::syntax::{SyntaxKind, SyntaxKind::*, SyntaxNode};
use crate::util::escape::check_escape;
use logos::{Lexer, Logos};
use rowan::{GreenNode, GreenNodeBuilder, TextRange, TextSize};
use std::convert::TryInto;

macro_rules! with_node {
    ($builder:expr, $kind:ident, $($content:tt)*) => {
        {
            $builder.start_node($kind.into());
            let res = $($content)*;
            $builder.finish_node();
            res
        }
    };
}

/// A syntax error that can occur during parsing.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Error {
    /// The span of the error.
    pub range: TextRange,

    /// Human-friendly error message.
    pub message: String,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({:?})", &self.message, &self.range)
    }
}
impl std::error::Error for Error {}

/// Parse a JSONA document into a [Rowan green tree](rowan::GreenNode).
///
/// The parsing will not stop at unexpected or invalid tokens.
/// Instead errors will be collected with their character offsets and lengths,
/// and the invalid token(s) will have the `ERROR` kind in the final tree.
///
/// The parser will also validate comment and string contents, looking for
/// invalid escape sequences and invalid characters.
/// These will also be reported as syntax errors.
///
/// This does not check for semantic errors such as duplicate keys.
pub fn parse(source: &str) -> Parse {
    Parser::new(source).parse()
}

/// A hand-written parser that uses the Logos lexer
/// to tokenize the source, then constructs
/// a Rowan green tree from them.
pub(crate) struct Parser<'p> {
    current_token: Option<SyntaxKind>,

    lexer: Lexer<'p, SyntaxKind>,
    builder: GreenNodeBuilder<'p>,
    errors: Vec<Error>,
}

/// This is just a convenience type during parsing.
/// It allows using "?", making the code cleaner.
type ParserResult<T> = Result<T, ()>;

impl<'p> Parser<'p> {
    pub(crate) fn new(source: &'p str) -> Self {
        Parser {
            current_token: None,
            lexer: SyntaxKind::lexer(source),
            builder: Default::default(),
            errors: Default::default(),
        }
    }

    fn parse(mut self) -> Parse {
        let _ = with_node!(self.builder, ROOT, self.parse_root());

        Parse {
            green_node: self.builder.finish(),
            errors: self.errors,
        }
    }

    fn parse_root(&mut self) -> ParserResult<()> {
        self.parse_annos()?;
        with_node!(self.builder, VALUE, self.parse_value())?;
        self.must_peek_eof()
    }

    fn parse_annos(&mut self) -> ParserResult<()> {
        if let Ok(AT) = self.peek_token() {
            self.builder.start_node(ANNOS.into());
            while let Ok(AT) = self.peek_token() {
                with_node!(self.builder, ENTRY, self.parse_anno_entry())?;
            }
            self.builder.finish_node();
        }
        Ok(())
    }

    fn parse_anno_entry(&mut self) -> ParserResult<()> {
        self.must_token_or(AT, r#"expected "@""#)?;
        with_node!(self.builder, KEY, self.parse_key())?;
        if let Ok(PARENTHESES_START) = self.peek_token() {
            with_node!(self.builder, VALUE, self.parse_anno_value())?;
        }
        Ok(())
    }

    fn parse_anno_value(&mut self) -> ParserResult<()> {
        self.must_token_or(PARENTHESES_START, r#"expected "(""#)?;
        with_node!(self.builder, VALUE, self.parse_value())?;
        self.must_token_or(PARENTHESES_END, r#"expected ")""#)?;
        Ok(())
    }

    fn parse_entry(&mut self) -> ParserResult<()> {
        with_node!(self.builder, KEY, self.parse_key())?;
        self.must_token_or(COLON, r#"expected ":""#)?;
        with_node!(self.builder, VALUE, self.parse_value_with_annos(BRACE_END))?;
        Ok(())
    }

    fn parse_value(&mut self) -> ParserResult<()> {
        let t = self.must_peek_token()?;
        match t {
            BRACE_START => {
                with_node!(self.builder, OBJECT, self.parse_object())
            }
            BRACKET_START => {
                with_node!(self.builder, ARRAY, self.parse_array())
            }
            NULL | BOOL => self.consume_current_token(),
            INTEGER => {
                // This could've been done more elegantly probably.
                if (self.lexer.slice().starts_with('0') && self.lexer.slice() != "0")
                    || (self.lexer.slice().starts_with("+0") && self.lexer.slice() != "+0")
                    || (self.lexer.slice().starts_with("-0") && self.lexer.slice() != "-0")
                {
                    self.consume_error_token("zero-padded integers are not allowed")
                } else if !validate_underscore_integer(self.lexer.slice(), 10) {
                    self.consume_error_token("invalid underscores")
                } else {
                    self.consume_current_token()
                }
            }
            INTEGER_BIN => {
                if !validate_underscore_integer(self.lexer.slice(), 2) {
                    self.consume_error_token("invalid underscores")
                } else {
                    self.consume_current_token()
                }
            }
            INTEGER_HEX => {
                if !validate_underscore_integer(self.lexer.slice(), 16) {
                    self.consume_error_token("invalid underscores")
                } else {
                    self.consume_current_token()
                }
            }
            INTEGER_OCT => {
                if !validate_underscore_integer(self.lexer.slice(), 8) {
                    self.consume_error_token("invalid underscores")
                } else {
                    self.consume_current_token()
                }
            }
            FLOAT => {
                let int_slice = if self.lexer.slice().contains('.') {
                    self.lexer.slice().split('.').next().unwrap()
                } else {
                    self.lexer.slice().split('e').next().unwrap()
                };

                if (int_slice.starts_with('0') && int_slice != "0")
                    || (int_slice.starts_with("+0") && int_slice != "+0")
                    || (int_slice.starts_with("-0") && int_slice != "-0")
                {
                    self.consume_error_token("zero-padded numbers are not allowed")
                } else if !validate_underscore_integer(self.lexer.slice(), 10) {
                    self.consume_error_token("invalid underscores")
                } else {
                    self.consume_current_token()
                }
            }
            DOUBLE_QUOTE | SINGLE_QUOTE => {
                match allowed_chars::string(self.lexer.slice()) {
                    Ok(_) => {}
                    Err(err_indices) => {
                        for e in err_indices {
                            let span = self.lexer.span();
                            self.add_error(&Error {
                                range: TextRange::new(
                                    TextSize::from((span.start + e) as u32),
                                    TextSize::from((span.start + e) as u32),
                                ),
                                message: "invalid character in string".into(),
                            });
                        }
                    }
                };
                match check_escape(self.lexer.slice()) {
                    Ok(_) => self.consume_current_token(),
                    Err(err_indices) => {
                        for e in err_indices {
                            self.add_error(&Error {
                                range: TextRange::new(
                                    (self.lexer.span().start + e).try_into().unwrap(),
                                    (self.lexer.span().start + e).try_into().unwrap(),
                                ),
                                message: "invalid escape sequence".into(),
                            });
                        }

                        // We proceed normally even if
                        // the string contains invalid escapes.
                        // It shouldn't affect the rest of the parsing.
                        self.consume_current_token()
                    }
                }
            }
            BACKTICK_QUOTE => {
                match allowed_chars::backtick_string(self.lexer.slice()) {
                    Ok(_) => {}
                    Err(err_indices) => {
                        for e in err_indices {
                            let span = self.lexer.span();
                            self.add_error(&Error {
                                range: TextRange::new(
                                    TextSize::from((span.start + e) as u32),
                                    TextSize::from((span.start + e) as u32),
                                ),
                                message: "invalid character in string".into(),
                            });
                        }
                    }
                };
                self.consume_current_token()
            }
            _ => self.consume_error_token("expected value"),
        }
    }

    fn parse_value_with_annos(&mut self, kind: SyntaxKind) -> ParserResult<()> {
        self.parse_value()?;
        let span = self.lexer.span();
        let mut is_comma = false;
        if let Ok(COMMA) = self.peek_token() {
            is_comma = true;
            self.consume_current_token()?;
        }
        self.parse_annos()?;
        let is_end = self.peek_token()? == kind;
        if !is_comma && !is_end {
            self.add_error(&Error {
                range: TextRange::new(
                    TextSize::from(span.start as u32),
                    TextSize::from(span.end as u32),
                ),
                message: r#"expect ",""#.into(),
            })
        }
        Ok(())
    }

    fn parse_object(&mut self) -> ParserResult<()> {
        self.must_token_or(BRACE_START, r#"expected "{""#)?;
        self.parse_annos()?;

        while let Ok(t) = self.must_peek_token() {
            match t {
                BRACE_END => {
                    return self.consume_current_token();
                }
                AT => {
                    let err = self.build_error(r#"unexpected "@""#);
                    self.add_error(&err);
                    self.parse_annos()?;
                }
                _ => {
                    let _ = with_node!(self.builder, ENTRY, self.parse_entry());
                }
            }
        }
        Ok(())
    }

    fn parse_array(&mut self) -> ParserResult<()> {
        self.must_token_or(BRACKET_START, r#"expected "[""#)?;
        let _ = self.parse_annos();

        while let Ok(t) = self.must_peek_token() {
            match t {
                BRACKET_END => {
                    return self.consume_current_token();
                }
                AT => {
                    let err = self.build_error(r#"unexpected "@""#);
                    self.add_error(&err);
                    self.parse_annos()?;
                }
                _ => {
                    let _ = with_node!(
                        self.builder,
                        VALUE,
                        self.parse_value_with_annos(BRACKET_END)
                    );
                }
            }
        }

        Ok(())
    }

    fn parse_key(&mut self) -> ParserResult<()> {
        let t = self.must_peek_token()?;

        match t {
            IDENT => self.consume_current_token(),
            NULL | BOOL => self.consume_current_token(),
            INTEGER_HEX | INTEGER_BIN | INTEGER_OCT => self.consume_current_token(),
            INTEGER => {
                if self.lexer.slice().starts_with('+') {
                    Err(())
                } else {
                    self.consume_current_token()
                }
            }
            SINGLE_QUOTE | DOUBLE_QUOTE => {
                match allowed_chars::string(self.lexer.slice()) {
                    Ok(_) => {}
                    Err(err_indices) => {
                        for e in err_indices {
                            let span = self.lexer.span();
                            self.add_error(&Error {
                                range: TextRange::new(
                                    TextSize::from((span.start + e) as u32),
                                    TextSize::from((span.start + e) as u32),
                                ),
                                message: "invalid control character in string".into(),
                            });
                        }
                    }
                };
                self.consume_current_token()
            }
            FLOAT => {
                if self.lexer.slice().starts_with('0') {
                    self.consume_error_token("zero-padded numbers are not allowed")
                } else if self.lexer.slice().starts_with('+') {
                    Err(())
                } else {
                    self.consume_current_token()
                }
            }
            _ => self.consume_error_token("expected identifier"),
        }
    }

    fn must_peek_token(&mut self) -> ParserResult<SyntaxKind> {
        match self.peek_token() {
            Ok(t) => Ok(t),
            Err(_) => {
                let err = self.build_error("unexpected EOF");
                self.add_error(&err);
                return Err(());
            }
        }
    }

    fn must_peek_eof(&mut self) -> ParserResult<()> {
        match self.peek_token() {
            Ok(_) => {
                let err = self.build_error("expected EOF");
                self.add_error(&err);
                Err(())
            }
            Err(_) => Ok(()),
        }
    }

    fn must_token_or(&mut self, kind: SyntaxKind, message: &str) -> ParserResult<()> {
        let t = self.must_peek_token()?;
        if kind == t {
            self.consume_current_token()
        } else {
            let err = self.build_error(message);
            self.add_error(&err);
            Err(())
        }
    }

    fn consume_current_token(&mut self) -> ParserResult<()> {
        match self.peek_token() {
            Err(_) => Err(()),
            Ok(token) => {
                self.consume_token(token, self.lexer.slice());
                Ok(())
            }
        }
    }

    fn consume_error_token(&mut self, message: &str) -> ParserResult<()> {
        let err = self.build_error(message);

        self.add_error(&err);

        self.consume_token(ERROR, self.lexer.slice());

        Err(())
    }

    fn peek_token(&mut self) -> ParserResult<SyntaxKind> {
        if self.current_token.is_none() {
            self.next_token();
        }

        self.current_token.ok_or(())
    }

    fn next_token(&mut self) {
        self.current_token = None;
        while let Some(token) = self.lexer.next() {
            match token {
                COMMENT_LINE | COMMENT_BLOCK => {
                    let multiline = token == COMMENT_BLOCK;
                    match allowed_chars::comment(self.lexer.slice(), multiline) {
                        Ok(_) => {}
                        Err(err_indices) => {
                            for e in err_indices {
                                let span = self.lexer.span();
                                self.add_error(&Error {
                                    range: TextRange::new(
                                        TextSize::from((span.start + e) as u32),
                                        TextSize::from((span.start + e) as u32),
                                    ),
                                    message: "invalid character in comment".into(),
                                });
                            }
                        }
                    };

                    self.consume_token(token, self.lexer.slice());
                }
                WHITESPACE | NEWLINE => {
                    self.consume_token(token, self.lexer.slice());
                }
                ERROR => {
                    self.consume_token(token, self.lexer.slice());
                    let span = self.lexer.span();
                    self.add_error(&Error {
                        range: TextRange::new(
                            TextSize::from(span.start as u32),
                            TextSize::from(span.end as u32),
                        ),
                        message: "unexpected token".into(),
                    })
                }
                _ => {
                    self.current_token = Some(token);
                    break;
                }
            }
        }
    }

    fn consume_token(&mut self, kind: SyntaxKind, text: &str) {
        self.builder.token(kind.into(), text);
        self.current_token = None;
    }

    fn build_error(&mut self, message: &str) -> Error {
        let span = self.lexer.span();

        Error {
            range: TextRange::new(
                TextSize::from(span.start as u32),
                TextSize::from(span.end as u32),
            ),
            message: message.into(),
        }
    }

    fn add_error(&mut self, e: &Error) {
        if let Some(last_err) = self.errors.last_mut() {
            if last_err.range == e.range {
                return;
            }
        }
        self.errors.push(e.clone());
    }
}

fn validate_underscore_integer(s: &str, radix: u32) -> bool {
    if s.starts_with('_') || s.ends_with('_') {
        return false;
    }

    let mut prev_char = 0 as char;

    for c in s.chars() {
        if c == '_' && !prev_char.is_digit(radix) {
            return false;
        }
        if !c.is_digit(radix) && prev_char == '_' {
            return false;
        }
        prev_char = c;
    }

    true
}

/// The final results of a parsing.
/// It contains the green tree, and
/// the errors that ocurred during parsing.
#[derive(Debug, Clone)]
pub struct Parse {
    pub green_node: GreenNode,
    pub errors: Vec<Error>,
}

impl Parse {
    /// Turn the parse into a syntax node.
    pub fn into_syntax(self) -> SyntaxNode {
        SyntaxNode::new_root(self.green_node)
    }
}

pub(crate) mod allowed_chars {
    pub(crate) fn comment(s: &str, multiline: bool) -> Result<(), Vec<usize>> {
        let mut err_indices = Vec::new();

        for (i, c) in s.chars().enumerate() {
            if multiline {
                if c != '\t' && c != '\n' && c != '\r' && c.is_control() {
                    err_indices.push(i);
                }
            } else {
                if c != '\t' && c.is_control() {
                    err_indices.push(i);
                }
            }
        }

        if err_indices.is_empty() {
            Ok(())
        } else {
            Err(err_indices)
        }
    }
    pub(crate) fn string(s: &str) -> Result<(), Vec<usize>> {
        let mut err_indices = Vec::new();

        for (i, c) in s.chars().enumerate() {
            if c != '\t'
                && (('\u{0000}'..='\u{0008}').contains(&c)
                    || ('\u{000A}'..='\u{001F}').contains(&c)
                    || c == '\u{007F}')
            {
                err_indices.push(i);
            }
        }

        if err_indices.is_empty() {
            Ok(())
        } else {
            Err(err_indices)
        }
    }
    pub(crate) fn backtick_string(s: &str) -> Result<(), Vec<usize>> {
        let mut err_indices = Vec::new();

        for (i, c) in s.chars().enumerate() {
            if c != '\t'
                && c != '\n'
                && c != '\r'
                && (('\u{0000}'..='\u{0008}').contains(&c)
                    || ('\u{000A}'..='\u{001F}').contains(&c)
                    || c == '\u{007F}')
            {
                err_indices.push(i);
            }
        }

        if err_indices.is_empty() {
            Ok(())
        } else {
            Err(err_indices)
        }
    }
}
