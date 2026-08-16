use super::common::{self, CommonLexer, LexItem};

pub struct Lexer<'i> {
    bytes: &'i [u8],
    i: usize,
}

impl<'i> Lexer<'i> {
    pub fn new(input: &'i str) -> Self {
        Self { bytes: input.as_bytes(), i: 0 }
    }

    #[inline(always)]
    fn is_ident_cont(c: u8) -> bool {
        (c.is_ascii_alphanumeric()) || c == b'_'
    }
}

impl<'i> CommonLexer<'i> for Lexer<'i> {
    #[inline(always)] fn bytes(&self) -> &'i[u8] { self.bytes }
    #[inline(always)] fn pos(&self) -> usize { self.i }
    #[inline(always)] fn set_pos(&mut self, v: usize) { self.i = v; }

    #[inline(always)]
    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() { self.i += 1; } else { break; }
        }
    }

    #[inline(always)]
    fn scan_ident_tail(&mut self) {
        while self.i < self.bytes.len() && Self::is_ident_cont(self.bytes[self.i]) {
            self.i += 1;
        }
    }
}

impl<'i> Iterator for Lexer<'i> {
    type Item = LexItem<'i>;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        common::next_token(self)
    }
}