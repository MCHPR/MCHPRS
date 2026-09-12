use std::fmt::Display;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn utf16(self, input: &str) -> Option<(usize, usize)> {
        let start = input.get(..self.start)?.encode_utf16().count();
        let length = input.get(self.start..self.end)?.encode_utf16().count();
        Some((start, length))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct SyntaxError {
    pub cursor: usize,
    pub message: String,
}

impl SyntaxError {
    pub fn new(cursor: usize, message: impl Into<String>) -> Self {
        Self {
            cursor,
            message: message.into(),
        }
    }

    pub fn contextual(&self, input: &str) -> String {
        let cursor = self.cursor.min(input.len());
        let prefix = input.get(..cursor).unwrap_or(input);
        let start = prefix.char_indices().rev().nth(9).map_or(0, |(i, _)| i);
        format!(
            "{} at position {}: {}{}<--[HERE]",
            self.message,
            prefix.encode_utf16().count(),
            if start > 0 { "..." } else { "" },
            &prefix[start..]
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Reader<'a> {
    input: &'a str,
    cursor: usize,
}

impl<'a> Reader<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, cursor: 0 }
    }

    pub fn input(&self) -> &'a str {
        self.input
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn remaining(&self) -> &'a str {
        &self.input[self.cursor..]
    }

    pub fn can_read(&self) -> bool {
        self.cursor < self.input.len()
    }

    pub fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    pub fn read(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.cursor += ch.len_utf8();
        Some(ch)
    }

    pub fn error(&self, message: impl Into<String>) -> SyntaxError {
        SyntaxError::new(self.cursor, message)
    }

    pub fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.read();
        }
    }

    pub fn take_while(&mut self, predicate: impl Fn(char) -> bool) -> &'a str {
        let start = self.cursor;
        while self.peek().is_some_and(&predicate) {
            self.read();
        }
        &self.input[start..self.cursor]
    }

    pub fn token(&mut self) -> Result<&'a str, SyntaxError> {
        let start = self.cursor;
        let token = self.take_while(|ch| !ch.is_whitespace());
        if token.is_empty() {
            Err(SyntaxError::new(start, "Expected a value"))
        } else {
            Ok(token)
        }
    }

    pub fn word(&mut self) -> String {
        self.take_while(Self::allowed_word).to_owned()
    }

    pub fn allowed_word(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '+')
    }

    pub fn custom_string(&mut self) -> Result<String, SyntaxError> {
        match self.peek() {
            Some('\'' | '"') => self.quoted(),
            _ => self.token().map(str::to_owned),
        }
    }

    pub fn quoted(&mut self) -> Result<String, SyntaxError> {
        let Some(quote @ ('\'' | '"')) = self.peek() else {
            return Err(self.error("Expected a quote"));
        };
        self.read();
        let mut value = String::new();
        while let Some(ch) = self.read() {
            if ch == quote {
                return Ok(value);
            }
            if ch == '\\' {
                let cursor = self.cursor;
                match self.read() {
                    Some(escaped) if escaped == quote || escaped == '\\' => value.push(escaped),
                    Some(_) => {
                        self.cursor = cursor;
                        return Err(self.error("Invalid escape sequence"));
                    }
                    None => break,
                }
            } else {
                value.push(ch);
            }
        }
        Err(self.error("Unclosed quoted string"))
    }

    pub fn greedy(&mut self) -> String {
        let value = self.remaining().to_owned();
        self.cursor = self.input.len();
        value
    }

    pub fn number<T: FromStr + PartialOrd + Display + Copy + Into<f64>>(
        &mut self,
        min: T,
        max: T,
    ) -> Result<T, SyntaxError> {
        let start = self.cursor;
        let text = self.take_while(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '-'));
        let value = text.parse::<T>().map_err(|_| {
            self.cursor = start;
            SyntaxError::new(start, "Expected a number")
        })?;
        if !value.into().is_finite() {
            self.cursor = start;
            return Err(SyntaxError::new(start, "Expected a finite number"));
        }
        if value < min || value > max {
            self.cursor = start;
            return Err(SyntaxError::new(
                start,
                format!("Number must be between {min} and {max}, found {value}"),
            ));
        }
        Ok(value)
    }
}
