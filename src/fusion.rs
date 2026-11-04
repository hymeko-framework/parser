//! Lossless recognition of complete literal arcs for opting-in consumers.
use crate::{
    SpannedLexItem,
    builder::LiteralReference,
    lexer::{Token as Original, common::CommonLexer},
};
use std::cell::RefCell;
mod workspace;
use workspace::FieldBuffer;

type Field<'a> = (&'a str, Option<crate::ast::Value<'a, &'a str>>);

#[derive(Default)]
struct Storage<'a> {
    slots: Vec<FieldBuffer<'a>>,
    free: Vec<usize>,
}

#[derive(Default)]
pub(crate) struct Scratch<'a>(RefCell<Storage<'a>>);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockToken<'a> {
    name: &'a str,
    slot: usize,
}

impl<'a> Scratch<'a> {
    fn acquire(&self) -> (usize, FieldBuffer<'a>) {
        let mut storage = self.0.borrow_mut();
        let slot = storage.free.pop().unwrap_or_else(|| {
            let slot = storage.slots.len();
            storage.slots.push(FieldBuffer::default());
            slot
        });
        (slot, std::mem::take(&mut storage.slots[slot]))
    }

    fn store(&self, slot: usize, fields: FieldBuffer<'a>) {
        self.0.borrow_mut().slots[slot] = fields;
    }

    fn release(&self, slot: usize, mut fields: FieldBuffer<'a>) {
        fields.clear();
        let mut storage = self.0.borrow_mut();
        storage.slots[slot] = fields;
        storage.free.push(slot);
    }

    pub(crate) fn finish<B: crate::builder::ConstructionBuilder<'a>>(
        &self,
        builder: &mut B,
        block: BlockToken<'a>,
    ) -> B::Item {
        let mut fields = std::mem::take(&mut self.0.borrow_mut().slots[block.slot]);
        let anno = || crate::ast::Anno {
            tags: Vec::new(),
            value: None,
        };
        let start = builder.begin_node(block.name, anno(), Vec::new());
        let mut leaves = fields.iter_mut();
        let leaf = |builder: &mut B, (name, value): &mut Field<'a>| {
            builder.finish_leaf(
                name,
                crate::ast::Anno {
                    tags: Vec::new(),
                    value: value.take(),
                },
            )
        };
        let items = if let Some(first) = leaves.next() {
            let first = leaf(builder, first);
            if let Some(second) = leaves.next() {
                let second = leaf(builder, second);
                let mut items = builder.two_items(first, second);
                for field in leaves {
                    let item = leaf(builder, field);
                    builder.push_item(&mut items, item);
                }
                items
            } else {
                builder.one_item(first)
            }
        } else {
            builder.empty_items()
        };
        let result = builder.finish_node(start, items);
        self.release(block.slot, fields);
        result
    }
}

/// A conservative dispatch hint; parentheses in strings/comments are harmless.
pub(crate) fn contains_arc_start(input: &[u8]) -> bool {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") {
        // SAFETY: runtime feature detection guards every call.
        return unsafe { contains_arc_start_avx2(input) };
    }
    input.contains(&b'(')
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn contains_arc_start_avx2(input: &[u8]) -> bool {
    use std::arch::x86_64::*;
    let needle = _mm256_set1_epi8(b'(' as i8);
    let mut chunks = input.chunks_exact(32);
    for chunk in &mut chunks {
        // SAFETY: chunks_exact guarantees 32 readable bytes; loadu needs no alignment.
        let bytes = unsafe { _mm256_loadu_si256(chunk.as_ptr().cast()) };
        if _mm256_movemask_epi8(_mm256_cmpeq_epi8(bytes, needle)) != 0 {
            return true;
        }
    }
    chunks.remainder().contains(&b'(')
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token<'a> {
    Plain(Original<'a>),
    Arc(Vec<LiteralReference<'a>>),
    Block(BlockToken<'a>),
}
pub(crate) struct Fuser<'s, 'a, I> {
    input: I,
    arcs: bool,
    blocks: bool,
    scratch: &'s Scratch<'a>,
    header_depth: usize,
    header_done: bool,
    item_start: bool,
}
impl<'s, 'a, I: Iterator<Item = SpannedLexItem<'a>> + CommonLexer<'a>> Fuser<'s, 'a, I> {
    pub fn with_blocks(input: I, arcs: bool, blocks: bool, scratch: &'s Scratch<'a>) -> Self {
        Self {
            input,
            arcs,
            blocks,
            scratch,
            header_depth: 0,
            header_done: false,
            item_start: false,
        }
    }
    fn token(&mut self) -> Option<(usize, Original<'a>, usize)> {
        self.input.next()?.ok()
    }
    fn block(&mut self, name: &'a str) -> Option<(BlockToken<'a>, usize)> {
        if !matches!(self.token()?.1, Original::LBrace) {
            return None;
        }
        let (slot, mut fields) = self.scratch.acquire();
        let end = (|| loop {
            let (_, token, end) = self.token()?;
            let field = match token {
                Original::RBrace => return Some(end),
                Original::Ident(field) => field,
                _ => return None,
            };
            let value = match self.token()?.1 {
                Original::Number(n) => crate::ast::Value::Num(n),
                Original::Str(s) => crate::ast::Value::Str(s),
                Original::Minus => match self.token()?.1 {
                    Original::Number(n) => crate::ast::Value::Num(-n),
                    _ => return None,
                },
                _ => return None,
            };
            if !matches!(self.token()?.1, Original::Semi) {
                return None;
            }
            if !fields.try_push((field, Some(value))) {
                return None;
            }
        })();
        if let Some(end) = end {
            self.scratch.store(slot, fields);
            Some((BlockToken { name, slot }, end))
        } else {
            self.scratch.release(slot, fields);
            None
        }
    }
    fn arc(&mut self) -> Option<(Vec<LiteralReference<'a>>, usize)> {
        read_arc(&mut self.input)
    }
}
impl<'a, I: Iterator<Item = SpannedLexItem<'a>> + CommonLexer<'a>> Iterator for Fuser<'_, 'a, I> {
    type Item = Result<(usize, Token<'a>, usize), crate::lexer::LexError>;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.input.next()?.map(|(start, token, end)| {
            if self.blocks
                && self.header_done
                && self.item_start
                && let Original::Ident(name) = token
            {
                let checkpoint = self.input.pos();
                if let Some((block, end)) = self.block(name) {
                    return (start, Token::Block(block), end);
                }
                self.input.set_pos(checkpoint);
                // Avoid repeatedly lexing unsupported prefixes in mixed documents.
                self.blocks = false;
            }
            self.item_start = matches!(token, Original::LBrace | Original::RBrace | Original::Semi);
            if !self.header_done {
                if matches!(token, Original::LBrace) {
                    self.header_depth += 1;
                } else if matches!(token, Original::RBrace) && self.header_depth > 0 {
                    self.header_depth -= 1;
                    self.header_done = self.header_depth == 0;
                }
            }
            if self.arcs && matches!(token, Original::LParen) {
                let checkpoint = self.input.pos();
                if let Some((refs, end)) = self.arc() {
                    self.item_start = true;
                    return (start, Token::Arc(refs), end);
                }
                self.input.set_pos(checkpoint);
            }
            (start, Token::Plain(token), end)
        }))
    }
}

fn read_arc<'a, I: Iterator<Item = SpannedLexItem<'a>>>(
    input: &mut I,
) -> Option<(Vec<LiteralReference<'a>>, usize)> {
    let mut token = || input.next()?.ok();

    let mut refs = Vec::new();
    loop {
        let sign = match token()?.1 {
            Original::Plus => 1,
            Original::Minus => -1,
            Original::Tilde => 0,
            _ => return None,
        };
        let (target_offset, Original::Ident(target), _) = token()? else {
            return None;
        };
        if !matches!(token()?.1, Original::LBrack) {
            return None;
        }
        let weight = match token()?.1 {
            Original::Number(n) => n,
            Original::Minus => match token()?.1 {
                Original::Number(n) => -n,
                _ => return None,
            },
            _ => return None,
        };
        if !matches!(token()?.1, Original::RBrack) {
            return None;
        }
        refs.push(LiteralReference {
            target,
            target_offset,
            sign,
            weight,
        });
        match token()?.1 {
            Original::Comma => continue,
            Original::RParen => break,
            _ => return None,
        }
    }
    let (_, Original::Semi, end) = token()? else {
        return None;
    };
    Some((refs, end))
}

/// Original arc-only iterator; block speculation does not enter this path.
pub(crate) struct ArcFuser<I>(pub I);
impl<'a, I: Iterator<Item = SpannedLexItem<'a>> + CommonLexer<'a>> Iterator for ArcFuser<I> {
    type Item = Result<(usize, Token<'a>, usize), crate::lexer::LexError>;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.0.next()?.map(|(start, token, end)| {
            if matches!(token, Original::LParen) {
                let checkpoint = self.0.pos();
                if let Some((refs, end)) = read_arc(&mut self.0) {
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
    fn block_scratch_reuses_buffers_and_bounds_pending_lookahead() {
        let source = format!("T{{}} {}", "n{x 1; text \"escaped\\n\";}".repeat(100));
        let scratch = Scratch::default();
        let mut builder = crate::builder::AstBuilder;
        let ast = crate::block_grammar::DescriptionParser::new()
            .parse(
                &mut builder,
                &scratch,
                Fuser::with_blocks(ScalarLexer(CoreLexer::new(&source)), true, true, &scratch),
            )
            .unwrap();
        assert_eq!(ast, crate::parse_description(&source).unwrap());
        assert!(
            scratch.0.borrow().slots.len() <= 2,
            "one lookahead token plus one reduction"
        );
        let (slot, mut fields) = scratch.acquire();
        assert!(fields.try_push(("field", Some(crate::ast::Value::Num(1.0)))));
        let pointer = fields.as_ptr();
        scratch.release(slot, fields);
        let (reused, fields) = scratch.acquire();
        assert_eq!(reused, slot);
        assert!(fields.is_empty());
        assert_eq!(fields.as_ptr(), pointer);
        scratch.release(reused, fields);
    }

    #[test]
    fn arc_scan_matches_scalar_at_boundaries_and_unaligned_starts() {
        for offset in 0..32 {
            for len in 0..=130 {
                let mut storage = vec![b'x'; offset + len];
                assert!(!contains_arc_start(&storage[offset..]));
                for position in offset..storage.len() {
                    storage[position] = b'(';
                    let input = &storage[offset..];
                    assert_eq!(contains_arc_start(input), input.contains(&b'('));
                    storage[position] = b'x';
                }
            }
        }
        assert!(contains_arc_start(
            "unicode \u{e1}\u{1f600} ( tail".as_bytes()
        ));
        assert!(!contains_arc_start("unicode \u{e1}\u{1f600}".as_bytes()));
    }

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
            let actual: Vec<_> = Fuser::with_blocks(
                ScalarLexer(CoreLexer::new(source)),
                true,
                false,
                &Scratch::default(),
            )
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
                let items: Vec<_> = Fuser::with_blocks(
                    ScalarLexer(CoreLexer::new(&source)),
                    true,
                    false,
                    &Scratch::default(),
                )
                .collect();
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
