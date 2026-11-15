//! GDB/MI2 protocol: tokenizer and recursive-descent parser.
//!
//! The parser is a genuine two-stage implementation:
//!
//! 1. **Tokenizer** built on `nom` combinators, recognizing record prefixes
//!    (`^`, `*`, `=`, `~`, `@`, `&`), optional numeric tokens, identifiers,
//!    punctuation, C-quoted strings with escape decoding, and record
//!    boundaries (one record per line).
//! 2. **Recursive-descent parser** over the token stream, producing a typed
//!    AST (`MiRecord`, `MiValue`, `MiResult`, `MiListItem`) that preserves the
//!    original structure, including unknown record classes and unknown
//!    fields.
//!
//! Regexes are deliberately not used for protocol parsing; the only regexes
//! in this crate parse human-readable *values* printed by GDB, never MI
//! records.

use std::collections::HashMap;

use nom::{
    branch::alt,
    bytes::complete::take_while,
    character::complete::{digit1, satisfy},
    combinator::map_res,
    IResult,
};
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error, PartialEq)]
pub enum MiParseError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("unexpected token at byte {position}: {message}")]
    UnexpectedToken { position: usize, message: String },
    #[error("unterminated string starting at byte {0}")]
    UnterminatedString(usize),
    #[error("unrecognized record at byte {position}: {message}")]
    UnknownRecord { position: usize, message: String },
    #[error("trailing characters after record at byte {0}")]
    TrailingCharacters(usize),
    #[error("invalid numeric token at byte {0}")]
    InvalidNumber(usize),
}

// ---------------------------------------------------------------------------
// Tokens (lexer output)
// ---------------------------------------------------------------------------

/// A single lexical token of an MI record.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Optional numeric command token (`12^done`).
    Number(u64),
    Caret,
    Star,
    Equal,
    Tilde,
    At,
    Ampersand,
    Comma,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    /// C-style quoted string with escapes already decoded.
    QuotedString(String),
    /// Bare identifier (record class or result variable name).
    Ident(String),
}

// ---------------------------------------------------------------------------
// Typed MI AST
// ---------------------------------------------------------------------------

/// An MI value: c-constant, tuple of results, or list.
#[derive(Debug, Clone, PartialEq)]
pub enum MiValue {
    Const(String),
    Tuple(Vec<MiResult>),
    List(Vec<MiListItem>),
}

/// A `variable=value` binding.
#[derive(Debug, Clone, PartialEq)]
pub struct MiResult {
    pub variable: String,
    pub value: MiValue,
}

/// One element of an MI list: a plain value or an inline result.
#[derive(Debug, Clone, PartialEq)]
pub enum MiListItem {
    Value(MiValue),
    Result(MiResult),
}

/// Which of the three MI stream channels a stream record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    /// `~` console output (GDB's own UI text).
    Console,
    /// `@` remote target output.
    Target,
    /// `&` GDB log (diagnostics, warnings).
    Log,
}

/// One parsed MI record.
///
/// Unknown record classes are preserved rather than discarded: `class` keeps
/// whatever GDB sent and `results` keeps every field.
#[derive(Debug, Clone, PartialEq)]
pub enum MiRecord {
    /// `^done`, `^error`, `^running`, `^connected`, ... (with optional token)
    Result {
        token: Option<u64>,
        class: String,
        results: Vec<MiResult>,
    },
    /// `*running`, `*stopped`, ... (with optional token)
    Exec {
        token: Option<u64>,
        class: String,
        results: Vec<MiResult>,
    },
    /// `=thread-created`, `=library-loaded`, ... (never tokenized by GDB)
    Notify {
        class: String,
        results: Vec<MiResult>,
    },
    /// `~"..."`, `@"..."`, `&"..."`
    Stream { kind: StreamKind, text: String },
}

impl MiRecord {
    /// Class string for result/exec/notify records.
    pub fn class(&self) -> Option<&str> {
        match self {
            MiRecord::Result { class, .. }
            | MiRecord::Exec { class, .. }
            | MiRecord::Notify { class, .. } => Some(class),
            MiRecord::Stream { .. } => None,
        }
    }

    /// Token for result/exec records.
    pub fn token(&self) -> Option<u64> {
        match self {
            MiRecord::Result { token, .. } | MiRecord::Exec { token, .. } => *token,
            _ => None,
        }
    }

    /// Result list for result/exec/notify records.
    pub fn results(&self) -> &[MiResult] {
        match self {
            MiRecord::Result { results, .. }
            | MiRecord::Exec { results, .. }
            | MiRecord::Notify { results, .. } => results,
            MiRecord::Stream { .. } => &[],
        }
    }

    /// First result whose variable equals `name`.
    pub fn result(&self, name: &str) -> Option<&MiValue> {
        self.results()
            .iter()
            .find(|r| r.variable == name)
            .map(|r| &r.value)
    }

    /// The result list as a map (later duplicates win, like GDB).
    pub fn result_map(&self) -> HashMap<&str, &MiValue> {
        self.results()
            .iter()
            .map(|r| (r.variable.as_str(), &r.value))
            .collect()
    }
}

/// Extract the string of a `Const` value, if it is one.
pub fn as_const(value: &MiValue) -> Option<&str> {
    match value {
        MiValue::Const(s) => Some(s),
        _ => None,
    }
}

/// Interpret a value like GDB does for `frame={...}` tuples: list of results.
pub fn as_tuple(value: &MiValue) -> Option<&[MiResult]> {
    match value {
        MiValue::Tuple(rs) => Some(rs),
        _ => None,
    }
}

pub fn as_list(value: &MiValue) -> Option<&[MiListItem]> {
    match value {
        MiValue::List(items) => Some(items),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.'
}

fn lex_number(input: &[u8]) -> IResult<&[u8], Token> {
    map_res(digit1, |digits: &[u8]| {
        let s = std::str::from_utf8(digits).unwrap_or("");
        s.parse::<u64>()
            .map(Token::Number)
            .map_err(|_| MiParseError::InvalidNumber(0))
    })(input)
}

fn lex_symbol(token: Token) -> impl Fn(&[u8]) -> IResult<&[u8], Token> {
    move |input: &[u8]| {
        let (rest, _) = nom::bytes::complete::tag::<_, _, nom::error::Error<&[u8]>>(match token {
            Token::Caret => b"^",
            Token::Star => b"*",
            Token::Equal => b"=",
            Token::Tilde => b"~",
            Token::At => b"@",
            Token::Ampersand => b"&",
            Token::Comma => b",",
            Token::LBrace => b"{",
            Token::RBrace => b"}",
            Token::LBracket => b"[",
            Token::RBracket => b"]",
            _ => unreachable!("lex_symbol called with non-symbol token"),
        })(input)?;
        Ok((rest, token.clone()))
    }
}

fn lex_ident(input: &[u8]) -> IResult<&[u8], Token> {
    // First character may not be a digit (numbers are lexed first); the rest
    // of the identifier may contain digits ("frame2", "b0").
    let (rest, first) =
        satisfy(|c: char| is_ident_char(c as u64 as u8) && !c.is_ascii_digit())(input)?;
    let (rest, tail) = take_while(is_ident_char)(rest)?;
    let mut bytes = vec![first as u8];
    bytes.extend_from_slice(tail);
    Ok((
        rest,
        Token::Ident(String::from_utf8_lossy(&bytes).into_owned()),
    ))
}

fn lex_quoted_string(input: &[u8]) -> IResult<&[u8], Token> {
    if input.first() != Some(&b'"') {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    }
    let start = 1usize; // past the opening quote
    let mut bytes: Vec<u8> = Vec::new();
    let mut i = start;
    loop {
        let Some(&c) = input.get(i) else {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::TakeWhile1,
            )));
        };
        match c {
            b'"' => {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                return Ok((&input[i + 1..], Token::QuotedString(text)));
            }
            b'\\' => {
                let Some(&esc) = input.get(i + 1) else {
                    return Err(nom::Err::Error(nom::error::Error::new(
                        input,
                        nom::error::ErrorKind::TakeWhile1,
                    )));
                };
                match esc {
                    b'n' => bytes.push(b'\n'),
                    b'r' => bytes.push(b'\r'),
                    b't' => bytes.push(b'\t'),
                    b'\\' => bytes.push(b'\\'),
                    b'"' => bytes.push(b'"'),
                    b'\'' => bytes.push(b'\''),
                    b'e' => bytes.push(0x1b),
                    b'b' => bytes.push(0x08),
                    b'f' => bytes.push(0x0c),
                    b'v' => bytes.push(0x0b),
                    b'a' => bytes.push(0x07),
                    b'0' => bytes.push(0),
                    b'x' => {
                        // \xHH (two hex digits, as GDB escapes them)
                        let h1 = input.get(i + 2).copied();
                        let h2 = input.get(i + 3).copied();
                        match (h1, h2) {
                            (Some(h1), Some(h2))
                                if h1.is_ascii_hexdigit() && h2.is_ascii_hexdigit() =>
                            {
                                let v = (hex_val(h1) << 4) | hex_val(h2);
                                bytes.push(v);
                                i += 4;
                                continue;
                            }
                            _ => bytes.push(b'x'),
                        }
                    }
                    // Unknown escape: GDB never sends one, but we degrade
                    // gracefully by keeping the escaped character itself.
                    other => bytes.push(other),
                }
                i += 2;
            }
            _ => {
                bytes.push(c);
                i += 1;
            }
        }
    }
}

fn hex_val(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        _ => c - b'A' + 10,
    }
}

/// Lex the next token, skipping whitespace.
fn next_token(input: &[u8]) -> IResult<&[u8], Token> {
    let (input, _) = nom::character::complete::space0(input)?;
    alt((
        lex_number,
        lex_quoted_string,
        lex_ident,
        lex_symbol(Token::Caret),
        lex_symbol(Token::Star),
        lex_symbol(Token::Equal),
        lex_symbol(Token::Tilde),
        lex_symbol(Token::At),
        lex_symbol(Token::Ampersand),
        lex_symbol(Token::Comma),
        lex_symbol(Token::LBrace),
        lex_symbol(Token::RBrace),
        lex_symbol(Token::LBracket),
        lex_symbol(Token::RBracket),
    ))(input)
}

/// Lex an entire line into tokens. Returns the tokens or a located error.
fn lex_line(input: &[u8]) -> Result<Vec<Token>, MiParseError> {
    let mut tokens = Vec::new();
    let mut rest = input;
    let mut offset = 0usize;
    loop {
        let ws = rest.iter().take_while(|c| c.is_ascii_whitespace()).count();
        offset += ws;
        rest = &rest[ws..];
        if rest.is_empty() {
            return Ok(tokens);
        }
        match next_token(rest) {
            Ok((remaining, tok)) => {
                tokens.push(tok);
                offset += rest.len() - remaining.len();
                rest = remaining;
            }
            Err(_) => {
                return Err(if rest.first() == Some(&b'"') {
                    MiParseError::UnterminatedString(offset)
                } else {
                    MiParseError::UnexpectedToken {
                        position: offset,
                        message: String::from_utf8_lossy(&rest[..rest.len().min(24)]).into_owned(),
                    }
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Recursive-descent parser
// ---------------------------------------------------------------------------

struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<&Token> {
        let t = self.tokens.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, expected: &Token) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// `value := c-string | tuple | list | bare-identifier`
    fn parse_value(&mut self) -> Result<MiValue, MiParseError> {
        match self.peek() {
            Some(Token::QuotedString(_)) => {
                if let Some(Token::QuotedString(s)) = self.next().cloned() {
                    Ok(MiValue::Const(s))
                } else {
                    unreachable!()
                }
            }
            Some(Token::LBrace) => {
                self.pos += 1;
                let results = self.parse_result_list(Token::RBrace)?;
                if !self.eat(&Token::RBrace) {
                    return Err(MiParseError::UnexpectedToken {
                        position: self.pos,
                        message: "expected '}'".into(),
                    });
                }
                Ok(MiValue::Tuple(results))
            }
            Some(Token::LBracket) => {
                self.pos += 1;
                let items = self.parse_list_items()?;
                if !self.eat(&Token::RBracket) {
                    return Err(MiParseError::UnexpectedToken {
                        position: self.pos,
                        message: "expected ']'".into(),
                    });
                }
                Ok(MiValue::List(items))
            }
            Some(Token::Ident(name)) => {
                // Lenient extension: some GDB variants emit bare identifiers
                // where a c-string would be legal. Preserve as Const.
                let name = name.clone();
                self.pos += 1;
                Ok(MiValue::Const(name))
            }
            other => Err(MiParseError::UnexpectedToken {
                position: self.pos,
                message: format!("expected value, found {:?}", other),
            }),
        }
    }

    /// `result := variable '=' value`
    fn parse_result(&mut self) -> Result<MiResult, MiParseError> {
        let variable = match self.peek() {
            Some(Token::Ident(name)) => name.clone(),
            _ => {
                return Err(MiParseError::UnexpectedToken {
                    position: self.pos,
                    message: "expected result variable name".into(),
                })
            }
        };
        self.pos += 1;
        if !self.eat(&Token::Equal) {
            return Err(MiParseError::UnexpectedToken {
                position: self.pos,
                message: format!("expected '=' after '{variable}'"),
            });
        }
        let value = self.parse_value()?;
        Ok(MiResult { variable, value })
    }

    /// `result-list := ε | (',' result)*` terminated by `end`.
    fn parse_result_list(&mut self, end: Token) -> Result<Vec<MiResult>, MiParseError> {
        let mut results = Vec::new();
        if self.peek() == Some(&end) {
            return Ok(results);
        }
        loop {
            // Lists of results may be preceded by a comma; tuples are not.
            if self.peek() == Some(&Token::Comma) {
                self.pos += 1;
                continue;
            }
            results.push(self.parse_result()?);
            if self.peek() == Some(&end) {
                return Ok(results);
            }
            if !self.eat(&Token::Comma) {
                return Err(MiParseError::UnexpectedToken {
                    position: self.pos,
                    message: "expected ',' or record end".into(),
                });
            }
            if self.peek() == Some(&end) {
                return Ok(results);
            }
        }
    }

    /// `list-items := ε | item (',' item)*` terminated by `]`
    fn parse_list_items(&mut self) -> Result<Vec<MiListItem>, MiParseError> {
        let mut items = Vec::new();
        if self.peek() == Some(&Token::RBracket) {
            return Ok(items);
        }
        loop {
            // Disambiguate `name=value` (result) from a plain value.
            let is_result = matches!(self.peek(), Some(Token::Ident(_)))
                && matches!(self.tokens.get(self.pos + 1), Some(Token::Equal));
            let item = if is_result {
                MiListItem::Result(self.parse_result()?)
            } else {
                MiListItem::Value(self.parse_value()?)
            };
            items.push(item);
            if self.peek() == Some(&Token::RBracket) {
                return Ok(items);
            }
            if !self.eat(&Token::Comma) {
                return Err(MiParseError::UnexpectedToken {
                    position: self.pos,
                    message: "expected ',' or ']'".into(),
                });
            }
            if self.peek() == Some(&Token::RBracket) {
                return Ok(items);
            }
        }
    }
}

/// Parse a single MI output line.
///
/// Returns `Ok(None)` for the `(gdb)` prompt and blank lines. Errors describe
/// malformed records instead of guessing.
pub fn parse_line(line: &str) -> Result<Option<MiRecord>, MiParseError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("(gdb)") {
        return Ok(None);
    }
    let tokens = lex_line(trimmed.as_bytes())?;
    if tokens.is_empty() {
        return Ok(None);
    }
    let mut p = Parser {
        tokens: &tokens,
        pos: 0,
    };

    // Optional numeric token prefix.
    let token = match p.peek() {
        Some(Token::Number(n)) => {
            let n = *n;
            p.pos += 1;
            Some(n)
        }
        _ => None,
    };

    let record = match p.next() {
        Some(Token::Caret) => {
            let class = expect_class(&mut p)?;
            let results = parse_to_end(&mut p)?;
            MiRecord::Result {
                token,
                class,
                results,
            }
        }
        Some(Token::Star) => {
            let class = expect_class(&mut p)?;
            let results = parse_to_end(&mut p)?;
            MiRecord::Exec {
                token,
                class,
                results,
            }
        }
        Some(Token::Equal) => {
            let class = expect_class(&mut p)?;
            let results = parse_to_end(&mut p)?;
            MiRecord::Notify { class, results }
        }
        Some(Token::Tilde) => {
            let text = expect_stream_text(&mut p)?;
            MiRecord::Stream {
                kind: StreamKind::Console,
                text,
            }
        }
        Some(Token::At) => {
            let text = expect_stream_text(&mut p)?;
            MiRecord::Stream {
                kind: StreamKind::Target,
                text,
            }
        }
        Some(Token::Ampersand) => {
            let text = expect_stream_text(&mut p)?;
            MiRecord::Stream {
                kind: StreamKind::Log,
                text,
            }
        }
        other => {
            let found = format!("{:?}", other);
            return Err(MiParseError::UnknownRecord {
                position: p.pos,
                message: format!("unexpected leading token {}", found),
            });
        }
    };
    Ok(Some(record))
}

fn expect_class(p: &mut Parser) -> Result<String, MiParseError> {
    match p.next() {
        Some(Token::Ident(name)) => Ok(name.clone()),
        _ => Err(MiParseError::UnexpectedToken {
            position: p.pos,
            message: "expected record class identifier".into(),
        }),
    }
}

fn expect_stream_text(p: &mut Parser) -> Result<String, MiParseError> {
    match p.next().cloned() {
        Some(Token::QuotedString(s)) => {
            if !p.at_end() {
                return Err(MiParseError::TrailingCharacters(p.pos));
            }
            Ok(s)
        }
        _ => Err(MiParseError::UnexpectedToken {
            position: p.pos,
            message: "expected quoted string after stream prefix".into(),
        }),
    }
}

/// Parse an optional `',' result-list` running to end of tokens.
fn parse_to_end(p: &mut Parser) -> Result<Vec<MiResult>, MiParseError> {
    let mut results = Vec::new();
    if p.at_end() {
        return Ok(results);
    }
    loop {
        if p.at_end() {
            return Ok(results);
        }
        if p.peek() == Some(&Token::Comma) {
            p.pos += 1;
            if p.at_end() {
                // Trailing comma: tolerate, GDB never sends one but be safe.
                return Ok(results);
            }
        }
        results.push(p.parse_result()?);
        if !p.at_end() {
            // Next must be a comma (continuation) — anything else is junk.
            if p.peek() != Some(&Token::Comma) {
                return Err(MiParseError::TrailingCharacters(p.pos));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> MiRecord {
        parse_line(s)
            .expect("parse should succeed")
            .expect("record")
    }

    #[test]
    fn simple_result_record() {
        let r = parse("6^done");
        match r {
            MiRecord::Result {
                token,
                class,
                results,
            } => {
                assert_eq!(token, Some(6));
                assert_eq!(class, "done");
                assert!(results.is_empty());
            }
            other => panic!("wrong record: {other:?}"),
        }
    }

    #[test]
    fn result_with_flat_results() {
        let r = parse("9^done,value=\"0x7fffffffdaa8\"");
        match &r {
            MiRecord::Result {
                token,
                class,
                results,
            } => {
                assert_eq!(*token, Some(9));
                assert_eq!(class, "done");
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].variable, "value");
            }
            other => panic!("wrong record: {other:?}"),
        }
        assert_eq!(
            as_const(r.result("value").unwrap()).unwrap(),
            "0x7fffffffdaa8"
        );
    }

    #[test]
    fn running_result() {
        let r = parse("17^running");
        assert_eq!(r.class(), Some("running"));
        assert_eq!(r.token(), Some(17));
    }

    #[test]
    fn error_result_with_escaped_quotes() {
        let r = parse("8^error,msg=\"No symbol table is loaded.  Use the \\\"file\\\" command.\"");
        match &r {
            MiRecord::Result { class, results, .. } => {
                assert_eq!(class, "error");
                assert_eq!(
                    as_const(&results[0].value).unwrap(),
                    "No symbol table is loaded.  Use the \"file\" command."
                );
            }
            other => panic!("wrong record: {other:?}"),
        }
    }

    #[test]
    fn exec_stopped_with_frame_tuple() {
        let line = "*stopped,reason=\"end-stepping-range\",frame={addr=\"0x00000000004004bd\",func=\"main\",args=[{name=\"argc\",value=\"1\"}],file=\"sample.c\",fullname=\"/tmp/sample.c\",line=\"13\",arch=\"i386:x86-64\"},thread-id=\"1\",stopped-threads=\"all\",core=\"1\"";
        let r = parse(line);
        match &r {
            MiRecord::Exec { class, .. } => {
                assert_eq!(class, "stopped");
                let frame = r.result("frame").unwrap();
                let fr = as_tuple(frame).unwrap();
                let get = |k: &str| -> String {
                    fr.iter()
                        .find(|x| x.variable == k)
                        .map(|x| match &x.value {
                            MiValue::Const(s) => s.clone(),
                            _ => String::new(),
                        })
                        .unwrap()
                };
                assert_eq!(get("func"), "main");
                assert_eq!(get("file"), "sample.c");
                assert_eq!(get("line"), "13");
                let args = fr.iter().find(|x| x.variable == "args").unwrap();
                if let MiValue::List(items) = &args.value {
                    assert_eq!(items.len(), 1);
                    match &items[0] {
                        // `args=[{name="argc",...}]` — a list of tuple VALUES
                        MiListItem::Value(MiValue::Tuple(t)) => {
                            assert_eq!(t[0].variable, "name");
                            assert_eq!(as_const(&t[0].value).unwrap(), "argc");
                        }
                        other => panic!("args item wrong: {other:?}"),
                    }
                } else {
                    panic!("args not a list");
                }
            }
            other => panic!("wrong record: {other:?}"),
        }
    }

    #[test]
    fn notify_library_loaded_with_ranges_list() {
        let line = "=library-loaded,id=\"/lib64/libc.so.6\",target-name=\"/lib64/libc.so.6\",host-name=\"/lib64/libc.so.6\",symbols-loaded=\"0\",thread-group=\"i1\",ranges=[{from=\"0x00007ffff7d9f000\",to=\"0x00007ffff7f98000\"}]";
        let r = parse(line);
        match &r {
            MiRecord::Notify { class, .. } => assert_eq!(class, "library-loaded"),
            other => panic!("wrong record: {other:?}"),
        }
        let ranges = as_list(r.result("ranges").unwrap()).unwrap();
        assert_eq!(ranges.len(), 1);
        match &ranges[0] {
            MiListItem::Value(MiValue::Tuple(t)) => {
                assert_eq!(t[0].variable, "from");
            }
            other => panic!("ranges item wrong: {other:?}"),
        }
    }

    #[test]
    fn stream_records() {
        assert_eq!(
            parse_line("~\"Reading symbols from bin...\\n\"")
                .unwrap()
                .unwrap(),
            MiRecord::Stream {
                kind: StreamKind::Console,
                text: "Reading symbols from bin...\n".into()
            }
        );
        assert_eq!(
            parse_line("&\"warning: something\\n\"").unwrap().unwrap(),
            MiRecord::Stream {
                kind: StreamKind::Log,
                text: "warning: something\n".into()
            }
        );
        assert_eq!(
            parse_line("@\"target text\"").unwrap().unwrap(),
            MiRecord::Stream {
                kind: StreamKind::Target,
                text: "target text".into()
            }
        );
    }

    #[test]
    fn prompt_and_blank_lines() {
        assert_eq!(parse_line("").unwrap(), None);
        assert_eq!(parse_line("   ").unwrap(), None);
        assert_eq!(parse_line("(gdb) ").unwrap(), None);
        assert_eq!(parse_line("(gdb)").unwrap(), None);
    }

    #[test]
    fn empty_string_and_empty_containers() {
        let r = parse("^done,x=\"\",l=[],t={}");
        assert_eq!(as_const(r.result("x").unwrap()).unwrap(), "");
        assert!(as_list(r.result("l").unwrap()).unwrap().is_empty());
        assert!(as_tuple(r.result("t").unwrap()).unwrap().is_empty());
    }

    #[test]
    fn deep_nesting() {
        let line = "^done,a=[{b=[{c=\"1\"},{d={e=\"2\",f=[\"x\",\"y\"]}}]},\"tail\"]";
        let r = parse(line);
        let a = as_list(r.result("a").unwrap()).unwrap();
        match &a[0] {
            MiListItem::Value(MiValue::Tuple(t)) => {
                let b = as_list(&t[0].value).unwrap();
                match &b[1] {
                    MiListItem::Value(MiValue::Tuple(inner)) => {
                        // inner = [d={e="2",f=["x","y"]}]
                        let d = as_tuple(&inner[0].value).unwrap();
                        let f = &d.iter().find(|x| x.variable == "f").unwrap().value;
                        match as_list(f).unwrap() {
                            [MiListItem::Value(MiValue::Const(x)), MiListItem::Value(MiValue::Const(y))] =>
                            {
                                assert_eq!((x.as_str(), y.as_str()), ("x", "y"));
                            }
                            _ => panic!("f wrong"),
                        }
                    }
                    other => panic!("b[1] wrong: {other:?}"),
                }
            }
            other => panic!("a[0] wrong: {other:?}"),
        }
        assert_eq!(
            as_const(match &a[1] {
                MiListItem::Value(v) => v,
                _ => panic!(),
            })
            .unwrap(),
            "tail"
        );
    }

    #[test]
    fn backslash_and_special_characters() {
        // literal backslash, tab, escaped escape
        let r = parse("^done,v=\"a\\\\b\\tc\\\"d\\\\\\n\"");
        let v = as_const(r.result("v").unwrap()).unwrap();
        assert_eq!(v, "a\\b\tc\"d\\\n");
    }

    #[test]
    fn unknown_escape_is_lenient() {
        let r = parse("^done,v=\"we\\qird\"");
        assert_eq!(as_const(r.result("v").unwrap()).unwrap(), "weqird");
    }

    #[test]
    fn missing_optional_token() {
        let r = parse("^done");
        assert_eq!(r.token(), None);
        let r = parse("*running,thread-id=\"all\"");
        assert_eq!(r.token(), None);
        assert_eq!(as_const(r.result("thread-id").unwrap()).unwrap(), "all");
    }

    #[test]
    fn malformed_records_error() {
        assert!(parse_line("^done,{bad").is_err());
        assert!(parse_line("^done,a=[unclosed").is_err());
        assert!(parse_line("^done,a=\"unterminated").is_err());
        assert!(parse_line("^").is_err());
        assert!(parse_line("*stopped,frame={level=\"0\"} extra_junk").is_err());
        assert!(parse_line("~no quotes").is_err());
    }

    #[test]
    fn unknown_record_class_preserved() {
        let r = parse("^weird-class,x=\"1\"");
        match r {
            MiRecord::Result { class, results, .. } => {
                assert_eq!(class, "weird-class");
                assert_eq!(results[0].variable, "x");
            }
            other => panic!("wrong record: {other:?}"),
        }
        let r = parse("=brand-new-notify,a=\"1\",b=\"2\"");
        assert_eq!(r.class(), Some("brand-new-notify"));
        assert_eq!(r.results().len(), 2);
    }

    #[test]
    fn list_of_results_only() {
        let r = parse("^done,stack-args=[frame={level=\"0\",args=[]},frame={level=\"1\",args=[{name=\"x\",value=\"1\"}]}]");
        let items = as_list(r.result("stack-args").unwrap()).unwrap();
        assert_eq!(items.len(), 2);
        match &items[0] {
            MiListItem::Result(res) => assert_eq!(res.variable, "frame"),
            other => panic!("wrong: {other:?}"),
        }
    }

    #[test]
    fn list_of_plain_values() {
        let r = parse("^done,children=[\"a\",\"b\"]");
        let items = as_list(r.result("children").unwrap()).unwrap();
        assert_eq!(items.len(), 2);
        match &items[1] {
            MiListItem::Value(MiValue::Const(s)) => assert_eq!(s, "b"),
            other => panic!("wrong: {other:?}"),
        }
    }

    #[test]
    fn real_gdb_transcript_fixture() {
        // Lines taken verbatim from GDB 18.1 MI2 output.
        let lines = [
            "=thread-group-added,id=\"i1\"",
            "8^done,bkpt={number=\"1\",type=\"breakpoint\",disp=\"keep\",enabled=\"y\",addr=\"0x00000000004004af\",func=\"main\",file=\"sample.c\",fullname=\"/tmp/opencode/probe/sample.c\",line=\"12\",thread-groups=[\"i1\"],times=\"0\",original-location=\"main\"}",
            "*stopped,reason=\"breakpoint-hit\",disp=\"keep\",bkptno=\"1\",frame={addr=\"0x00000000004004af\",func=\"main\",args=[{name=\"argc\",value=\"1\"},{name=\"argv\",value=\"0x7fffffffdbe8\"}],file=\"sample.c\",fullname=\"/tmp/opencode/probe/sample.c\",line=\"12\",arch=\"i386:x86-64\"},thread-id=\"1\",stopped-threads=\"all\",core=\"0\"",
            "11^done,variables=[{name=\"argc\",arg=\"1\",value=\"1\"},{name=\"argv\",arg=\"1\",value=\"0x7fffffffdbe8\"},{name=\"arr\",value=\"0x0\"},{name=\"p\",value=\"{x = 0, y = 0}\"},{name=\"ptr\",value=\"0x0\"},{name=\"sum\",value=\"0\"}]",
            "12^done,memory=[{begin=\"0x00007fffffffdaa8\",offset=\"0x0000000000000000\",end=\"0x00007fffffffdab4\",contents=\"000000000000000060dbffff\"}]",
            "*stopped,reason=\"exited-normally\"",
        ];
        for line in lines {
            parse_line(line)
                .unwrap_or_else(|e| panic!("failed on: {line}\n{e}"))
                .unwrap();
        }
        let r = parse("12^done,memory=[{begin=\"0x00007fffffffdaa8\",offset=\"0x0000000000000000\",end=\"0x00007fffffffdab4\",contents=\"000000000000000060dbffff\"}]");
        let mem = as_list(r.result("memory").unwrap()).unwrap();
        match &mem[0] {
            MiListItem::Value(MiValue::Tuple(t)) => {
                let contents = t.iter().find(|x| x.variable == "contents").unwrap();
                assert_eq!(
                    as_const(&contents.value).unwrap(),
                    "000000000000000060dbffff"
                );
            }
            other => panic!("wrong: {other:?}"),
        }
    }

    #[test]
    fn token_overflow_is_error() {
        let big = format!("{}^done", u64::MAX);
        assert!(parse_line(&big).is_ok());
        let bigger = format!("{}9^done", u64::MAX);
        assert!(parse_line(&bigger).is_err());
    }

    #[test]
    fn whitespace_tolerant() {
        let r = parse("  3 ^ done,  a = \"x\" , b = \"y\"  ");
        // Note: real GDB never emits spaces; this documents lenient lexing.
        match r {
            MiRecord::Result { token, results, .. } => {
                assert_eq!(token, Some(3));
                assert_eq!(results.len(), 2);
            }
            other => panic!("wrong: {other:?}"),
        }
    }
}
