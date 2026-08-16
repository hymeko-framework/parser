use super::token::{Token, LexError};

pub type Location = usize;
pub type Spanned<T> = (Location, T, Location);
pub type LexItem<'a> = Result<Spanned<Token<'a>>, LexError>;

#[inline(always)]
fn is_ident_start(c: u8) -> bool {
    matches!(c, b'_' | b'a'..=b'z' | b'A'..=b'Z')
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
        let mut has_escape = false;
        let len = self.len();

        // Pass 1: Fast-scan via index, avoiding direct slice holding
        while i < len {
            let Some(c) = self.byte_at(i) else { break; };
            if c == b'"' {
                break;
            } else if c == b'\\' {
                has_escape = true;
                i += 2; // Skip the backslash and the escaped character
            } else {
                i += 1;
            }
        }

        if i >= len || self.byte_at(i) != Some(b'"') {
            return Err(LexError { at: start, msg: "Unterminated string literal".into() });
        }

        let end = i;

        // Pass 2: Strictly scope the immutable borrow
        let token = {
            let b = self.bytes();
            // CORRECTED: Start at `start + 1` to exclude the opening quote
            let slice = &b[start + 1 .. end];

            if !has_escape {
                let text = unsafe { std::str::from_utf8_unchecked(slice) };
                Token::Str(std::borrow::Cow::Borrowed(text))
            } else {
                let mut out = String::with_capacity(slice.len());
                let mut j = 0;
                while j < slice.len() {
                    if slice[j] == b'\\' && j + 1 < slice.len() {
                        match slice[j + 1] {
                            b'n' => out.push('\n'),
                            b'r' => out.push('\r'),
                            b't' => out.push('\t'),
                            b'\\' => out.push('\\'),
                            b'"' => out.push('"'),
                            other => out.push(other as char),
                        }
                        j += 2;
                    } else {
                        out.push(slice[j] as char);
                        j += 1;
                    }
                }
                Token::Str(std::borrow::Cow::Owned(out)) // Heap allocation only when strictly required.
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