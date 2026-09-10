use super::token::{Token, LexError};

pub type Location = usize;
pub type Spanned<T> = (Location, T, Location);
pub type LexItem<'a> = Result<Spanned<Token<'a>>, LexError>;

#[inline(always)]
fn is_ident_start(c: u8) -> bool {
    matches!(c, b'_' | b'a'..=b'z' | b'A'..=b'Z')
}

/// Length of the prefix containing neither a quote nor an escape introducer.
/// Safe word loads skip eight bytes at a time, including arbitrary UTF-8 bytes.
#[inline]
fn string_prefix_len(bytes: &[u8]) -> usize {
    let mut i = 0;
    let has_zero =
        |word: u64| word.wrapping_sub(0x0101_0101_0101_0101) & !word & 0x8080_8080_8080_8080 != 0;
    while let Some(chunk) = bytes[i..].first_chunk::<8>() {
        let word = u64::from_ne_bytes(*chunk);
        if has_zero(word ^ 0x2222_2222_2222_2222) || has_zero(word ^ 0x5c5c_5c5c_5c5c_5c5c) {
            break;
        }
        i += 8;
    }
    // The zero-byte mask is a presence test, not an exact lane bitmap:
    // borrows can mark adjacent lanes. Locate the first delimiter scalarly.
    i + bytes[i..]
        .iter()
        .position(|&b| b == b'"' || b == b'\\')
        .unwrap_or(bytes.len() - i)
}

/// A közös lexer “backend” trait.
/// A SIMD/simple lexer csak ezt implementálja + a 2 speciális hookot.
pub trait CommonLexer<'a> {
    fn bytes(&self) -> &'a[u8];
    fn pos(&self) -> usize;
    fn set_pos(&mut self, i: usize);

    #[inline(always)]
    fn byte_at(&self, idx: usize) -> Option<u8> {
        let b = self.bytes();
        if idx < b.len() { Some(b[idx]) } else { None }
    }

    #[allow(dead_code)]
    #[inline(always)]
    fn peek2(&self) -> Option<(u8, u8)> {
        let i = self.pos();
        Some((self.byte_at(i)?, self.byte_at(i + 1)?))
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.bytes().len()
    }

    #[inline(always)]
    fn peek(&self) -> Option<u8> {
        let i = self.pos();
        let b = self.bytes();
        if i < b.len() { Some(b[i]) } else { None }
    }

    #[inline(always)]
    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.set_pos(self.pos() + 1);
        Some(c)
    }

    #[allow(dead_code)]
    /// Hook #1: whitespace skip (SIMD vagy scalar)
    fn skip_ws(&mut self);

    /// Hook #2: ident tail scan (SIMD vagy scalar)
    fn scan_ident_tail(&mut self);

    #[inline(always)]
    fn skip_ws_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'/') => {
                    let start = self.pos();
                    self.bump();
                    match self.peek() {
                        Some(b'/') => {
                            self.bump();
                            while let Some(c) = self.bump() {
                                if c == b'\n' { break; }
                            }
                        }
                        Some(b'*') => {
                            self.bump();
                            let mut closed = false;
                            while let Some(c) = self.bump() {
                                if c == b'*' && self.peek() == Some(b'/') {
                                    self.bump();
                                    closed = true;
                                    break;
                                }
                            }
                            if !closed {
                                return Err(LexError {
                                    at: start,
                                    msg: "Unterminated multi-line comment".into()
                                });
                            }
                        }
                        _ => {
                            self.set_pos(start);
                            return Ok(());
                        }
                    }
                }
                _ => return Ok(())
            }
        }
    }

    #[inline(always)]
    fn lex_ident(&mut self, start: usize) -> Token<'a> {
        self.scan_ident_tail();
        // SAFETY: Identifier characters are exclusively valid ASCII.
        let text = unsafe { std::str::from_utf8_unchecked(&self.bytes()[start..self.pos()]) };
        match text {
            "using" => Token::Using,
            "as"    => Token::As,
            "const" => Token::Const,
            _       => Token::Ident(text),
        }
    }

    #[inline(always)]
    fn lex_number(&mut self, start: usize) -> Result<Token<'a>, LexError> {
        let mut i = self.pos();
        let mut seen_dot = false;

        let b = self.bytes();

        while i < b.len() {
            let c = b[i];
            if c.is_ascii_digit() {
                i += 1;
            } else if c == b'.' && !seen_dot {
                seen_dot = true;
                i += 1;
            } else {
                break;
            }
        }

        let text = unsafe { std::str::from_utf8_unchecked(&b[start..i]) };

        // Construct the owned types here to end the borrow of `b` and `text`.
        let result = text.parse::<f64>()
            .map(Token::Number)
            .map_err(|_| LexError {
                at: start,
                msg: format!("Bad number literal: {}", text)
            });

        // The immutable borrow is mathematically proven to be dead here.
        self.set_pos(i);

        result
    }

    #[inline(always)]
    fn lex_string(&mut self, start: usize) -> Result<Token<'a>, LexError> {
        let mut i = self.pos(); // self.pos() is already at start + 1
        let mut first_escape = None;
        let len = self.len();

        // Skip ordinary runs without changing lexer position on an error.
        while i < len {
            i += string_prefix_len(&self.bytes()[i..]);
            let Some(c) = self.byte_at(i) else {
                break;
            };
            if c == b'"' {
                break;
            } else {
                first_escape.get_or_insert(i);
                i += 2; // Skip the backslash and the escaped character
            }
        }

        if i >= len || self.byte_at(i) != Some(b'"') {
            return Err(LexError {
                at: start,
                msg: "Unterminated string literal".into(),
            });
        }

        let end = i;

        // Pass 2: Strictly scope the immutable borrow
        let token = {
            let b = self.bytes();
            // CORRECTED: Start at `start + 1` to exclude the opening quote
            let slice = &b[start + 1..end];

            if let Some(first_escape) = first_escape {
                // Preserve UTF-8 bytes on the escaped path too. Casting each
                // byte to char corrupts non-ASCII text whenever any escape occurs.
                let mut out = Vec::with_capacity(slice.len());
                let mut j = first_escape - start - 1;
                out.extend_from_slice(&slice[..j]);
                while j < slice.len() {
                    // The delimiter scan proved this is a complete escape.
                    match slice[j + 1] {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'\\' => out.push(b'\\'),
                        b'"' => out.push(b'"'),
                        other => out.push(other),
                    }
                    j += 2;
                    let next = j + string_prefix_len(&slice[j..]);
                    out.extend_from_slice(&slice[j..next]);
                    j = next;
                }
                let text = String::from_utf8(out).map_err(|_| LexError {
                    at: start,
                    msg: "Invalid UTF-8 in string literal".into(),
                })?;
                Token::Str(std::borrow::Cow::Owned(text)) // Heap allocation only when strictly required.
            } else {
                let text = unsafe { std::str::from_utf8_unchecked(slice) };
                Token::Str(std::borrow::Cow::Borrowed(text))
            }
        };

        // Pass 3: Safe mutation
        self.set_pos(end + 1);

        Ok(token)
    }
}

/// Közös `next()` implementáció.
/// A lexer típusa adja a hookokat.
pub fn next_token<'a, L: CommonLexer<'a>>(lex: &mut L) -> Option<LexItem<'a>> {
    if let Err(e) = lex.skip_ws_and_comments() {
        return Some(Err(e));
    }

    let start = lex.pos();
    let bytes = lex.bytes();

    // 2. Handle EOF correctly for the LALRPOP anchor [cite: 2026-02-08]
    if start >= bytes.len() {
        return None;
    }

    let c = lex.bump()?;

    let tok = match c {
        b'{' => Token::LBrace,
        b'}' => Token::RBrace,
        b'(' => Token::LParen,
        b')' => Token::RParen,
        b'[' => Token::LBrack,
        b']' => Token::RBrack,
        b':' => Token::Colon,
        b'<' => Token::LAngle,
        b'>' => Token::RAngle,
        b',' => Token::Comma,
        b';' => Token::Semi,
        b'.' => Token::Dot,
        b'@' => Token::At,
        b'+' => Token::Plus,
        b'~' => Token::Tilde,

        b'-' => {
            let i = lex.pos();
            if lex.byte_at(i) == Some(b'>') {
                lex.set_pos(i + 1);
                Token::Arrow
            } else {
                Token::Minus
            }
        }

        b'"' => match lex.lex_string(start) {
            Ok(t) => t,
            Err(e) => return Some(Err(e)),
        },
        b'?' => Token::Question,
        b'*' => Token::Star,
        b'/' => Token::Slash,
        b'=' => Token::Equals,


        d if d.is_ascii_digit() => match lex.lex_number(start) {
            Ok(t) => t,
            Err(e) => return Some(Err(e)),
        },

        a if is_ident_start(a) => lex.lex_ident(start),


        other => {
            return Some(Err(LexError {
                at: start,
                msg: format!("Unexpected char: {:?}", other as char),
            }))
        }
    };

    let end = lex.pos();
    Some(Ok((start, tok, end)))
}

#[cfg(test)]
mod tests {
    use super::string_prefix_len;

    #[test]
    fn escaped_runs_match_character_oracle_and_preserve_spans() {
        use crate::lexer::Token;
        use crate::lexer::simd::{CoreLexer, ScalarLexer};
        use std::borrow::Cow;

        for padding in 0..=40 {
            for escape in ["\\n", "\\r", "\\t", "\\\\", "\\\"", "\\q", "\\é", "\\🙂"] {
                for middle in ["", "ascii", "árvíz日本語🙂", "\0\n\t"] {
                    let body = format!(
                        "{}{}{}{}{}",
                        "a".repeat(padding),
                        escape,
                        middle,
                        escape,
                        "z".repeat(padding)
                    );
                    // Decode Unicode scalar values, independently of the byte-run scanner.
                    let mut expected = String::new();
                    let mut chars = body.chars();
                    while let Some(ch) = chars.next() {
                        expected.push(if ch == '\\' {
                            match chars.next().unwrap() {
                                'n' => '\n',
                                'r' => '\r',
                                't' => '\t',
                                other => other,
                            }
                        } else {
                            ch
                        });
                    }
                    let source = format!("  \"{body}\";");
                    let mut lexer = ScalarLexer(CoreLexer::new(&source));
                    let (start, token, end) = lexer.next().unwrap().unwrap();
                    assert_eq!((start, end), (2, source.len() - 1));
                    assert!(matches!(&token, Token::Str(Cow::Owned(_))));
                    assert_eq!(token, Token::Str(Cow::Owned(expected)));
                    assert_eq!(lexer.next(), Some(Ok((end, Token::Semi, end + 1))));
                    assert_eq!(lexer.next(), None);
                }
            }
        }
    }

    #[test]
    fn unterminated_escaped_runs_preserve_error_and_cursor() {
        use super::CommonLexer;
        use crate::lexer::simd::{CoreLexer, ScalarLexer};

        for padding in 0..=40 {
            for tail in ["", "\\", "\\\"", "\\n日本語", "\\🙂"] {
                let source = format!("  \"{}{}", "é".repeat(padding), tail);
                let mut lexer = ScalarLexer(CoreLexer::new(&source));
                let error = lexer.next().unwrap().unwrap_err();
                assert_eq!(error.at, 2);
                assert_eq!(error.msg, "Unterminated string literal");
                // Failure must leave the cursor immediately after the opening quote.
                assert_eq!(lexer.pos(), 3);
            }
        }
    }

    #[test]
    fn string_prefix_matches_scalar_for_bytes_lanes_and_tails() {
        for len in 0..40 {
            for encoded in 0..512 {
                let byte = (encoded / 2) as u8;
                let delimiter = b"\"\\"[encoded % 2];
                let mut input = vec![byte; len];
                for index in 0..=len {
                    if index < len {
                        input[index] = delimiter;
                    }
                    let expected = input
                        .iter()
                        .position(|&b| b == b'"' || b == b'\\')
                        .unwrap_or(input.len());
                    assert_eq!(string_prefix_len(&input), expected, "{input:?}");
                    if index < len {
                        input[index] = byte;
                    }
                }
            }
        }
    }

    #[test]
    fn string_prefix_handles_cross_lane_borrows_and_multiple_delimiters() {
        for first in 0..=u8::MAX {
            for second in 0..=u8::MAX {
                let input = [0xff, first, second, b'!', b'"', b'\\', 0, 0x80];
                let expected = input.iter().position(|&b| b == b'"' || b == b'\\').unwrap();
                assert_eq!(string_prefix_len(&input), expected);
            }
        }
    }
}
