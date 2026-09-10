//! Lossless recognition of complete literal arcs for opting-in consumers.
use crate::{
    SpannedLexItem,
    builder::LiteralReference,
    lexer::{Token as Original, common::CommonLexer},
};
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token<'a> {
    Plain(Original<'a>),
    Arc(Vec<LiteralReference<'a>>),
}
pub(crate) struct Fuser<I>(I);
impl<'a, I: Iterator<Item = SpannedLexItem<'a>> + CommonLexer<'a>> Fuser<I> {
    pub fn new(input: I) -> Self {
        Self(input)
    }
    fn token(&mut self) -> Option<(usize, Original<'a>, usize)> {
        self.0.next()?.ok()
    }
    fn arc(&mut self) -> Option<(Vec<LiteralReference<'a>>, usize)> {
        let mut refs = Vec::new();
        loop {
            let sign = match self.token()?.1 {
                Original::Plus => 1,
                Original::Minus => -1,
                Original::Tilde => 0,
                _ => return None,
            };
            let (target_offset, Original::Ident(target), _) = self.token()? else {
                return None;
            };
            if !matches!(self.token()?.1, Original::LBrack) {
                return None;
            }
            let weight = match self.token()?.1 {
                Original::Number(n) => n,
                Original::Minus => match self.token()?.1 {
                    Original::Number(n) => -n,
                    _ => return None,
                },
                _ => return None,
            };
            if !matches!(self.token()?.1, Original::RBrack) {
                return None;
            }
            refs.push(LiteralReference {
                target,
                target_offset,
                sign,
                weight,
            });
            match self.token()?.1 {
                Original::Comma => continue,
                Original::RParen => break,
                _ => return None,
            }
        }
        let (_, Original::Semi, end) = self.token()? else {
            return None;
        };
        Some((refs, end))
    }
}
impl<'a, I: Iterator<Item = SpannedLexItem<'a>> + CommonLexer<'a>> Iterator for Fuser<I> {
    type Item = Result<(usize, Token<'a>, usize), crate::lexer::LexError>;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.0.next()?.map(|(start, token, end)| {
            if matches!(token, Original::LParen) {
                let checkpoint = self.0.pos();
                if let Some((refs, end)) = self.arc() {
                    return (start, Token::Arc(refs), end);
                }
                self.0.set_pos(checkpoint);
            }
            (start, Token::Plain(token), end)
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::simd::{CoreLexer, ScalarLexer};

    #[test]
    fn mismatches_keep_tokens_spans_and_lexical_errors() {
        for source in [
            "(",
            "(+",
            "(+a",
            "(+a[",
            "(+a[1",
            "(+a[-",
            "(+a[1],-b[2]@);",
            "(+a[1],);",
            "(+a[1])<t>;",
            "+a[1,]",
            "+a.b[1]",
            "+<t>a[1]",
            "+a[\"x\"]",
            "+a[1\"bad",
            "-3; +a[1,2];",
            "+a/*unterminated",
        ] {
            let original: Vec<_> = ScalarLexer(CoreLexer::new(source)).collect();
            let actual: Vec<_> = Fuser::new(ScalarLexer(CoreLexer::new(source)))
                .map(|item| {
                    item.map(|(l, t, r)| {
                        let Token::Plain(t) = t else {
                            panic!("unexpected fusion: {source}")
                        };
                        (l, t, r)
                    })
                })
                .collect();
            assert_eq!(actual, original, "{source}");
        }
    }

    #[test]
    fn literal_arcs_preserve_sign_weight_and_comments() {
        for (prefix, sign) in [("+", 1), ("-", -1), ("~", 0)] {
            for (literal, expected) in [("1", 1.0f64), ("-0.0", -0.0), ("-3.5", -3.5)] {
                let source = format!("({prefix} /*c*/ target [ {literal} ]);");
                let items: Vec<_> = Fuser::new(ScalarLexer(CoreLexer::new(&source))).collect();
                assert_eq!(items.len(), 1);
                let Ok((0, Token::Arc(refs), end)) = &items[0] else {
                    panic!("fusion")
                };
                assert_eq!(*end, source.len());
                let reference = &refs[0];
                assert_eq!(refs.len(), 1);
                assert_eq!(reference.target_offset, source.find("target").unwrap());
                assert_eq!(reference.target, "target");
                assert_eq!(reference.sign, sign);
                assert_eq!(reference.weight.to_bits(), expected.to_bits());
            }
        }
    }
}
