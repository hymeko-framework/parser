#![allow(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

use super::common::{self, CommonLexer, LexItem};

// 1. Base state
pub struct CoreLexer<'a> {
    s: &'a [u8],
    i: usize,
    n: usize,
}

impl<'a> CoreLexer<'a> {

    #[inline(always)]
    pub fn new(input: &'a str) -> Self {
        Self {
            s: input.as_bytes(),
            i: 0,
            n: input.len(),
        }
    }

    #[inline(always)]
    fn is_ident_cont(c: u8) -> bool {
        matches!(c, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
    }
}

// 2. Specific hardware variants
pub struct Avx2Lexer<'a>(pub CoreLexer<'a>);
pub struct Sse2Lexer<'a>(pub CoreLexer<'a>);
pub struct ScalarLexer<'a>(pub CoreLexer<'a>);


// --- Implement Iterator directly on the monomorphized types ---

impl<'a> Iterator for Avx2Lexer<'a> {
    type Item = LexItem<'a>;
    #[inline(always)] fn next(&mut self) -> Option<Self::Item> { common::next_token(self) }
}

impl<'a> Iterator for Sse2Lexer<'a> {
    type Item = LexItem<'a>;
    #[inline(always)] fn next(&mut self) -> Option<Self::Item> { common::next_token(self) }
}

impl<'a> Iterator for ScalarLexer<'a> {
    type Item = LexItem<'a>;
    #[inline(always)] fn next(&mut self) -> Option<Self::Item> { common::next_token(self) }
}

// 5. Trait Implementations

// --- AVX2 ---
impl<'a> Avx2Lexer<'a> {
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    unsafe fn skip_ws_avx2(&mut self) {
        use std::arch::x86_64::*;
        // Phase 1: SIMD - process 32 bytes at a time until we find a non-whitespace character or run out of input.
        while self.0.i + 32 <= self.0.n {
            let p = self.0.s.as_ptr().add(self.0.i) as *const __m256i;
            let chunk = _mm256_loadu_si256(p);

            let sp = _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(b' ' as i8));
            let tb = _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(b'\t' as i8));
            let nl = _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(b'\n' as i8));
            let cr = _mm256_cmpeq_epi8(chunk, _mm256_set1_epi8(b'\r' as i8));

            let ws = _mm256_or_si256(_mm256_or_si256(sp, tb), _mm256_or_si256(nl, cr));
            let mask = _mm256_movemask_epi8(ws) as u32;

            if mask == 0xFFFF_FFFF {
                self.0.i += 32;
            } else {
                self.0.i += (!mask).trailing_zeros() as usize;
                return;
            }
        }
        // Phase 2: Scalar - handle the remaining bytes one by one.
        while self.0.i < self.0.n {
            match self.0.s[self.0.i] {
                b' ' | b'\t' | b'\n' | b'\r' => self.0.i += 1,
                _ => return,
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    unsafe fn scan_ident_tail_avx2(&mut self) {
        // Short-circuit if we're already at the end or the first char is not an identifier continuation.
        while self.0.i < self.0.n && CoreLexer::is_ident_cont(self.0.s[self.0.i]) {
            if self.0.n - self.0.i >= 32 {
                use std::arch::x86_64::*;
                // Phase 1: SIMD - process 32 bytes at a time until we find a non-identifier character or run out of input.
                while self.0.i + 32 <= self.0.n {
                    let p = self.0.s.as_ptr().add(self.0.i) as *const __m256i;
                    let x = _mm256_loadu_si256(p);

                    let ge = |lo: u8| _mm256_cmpeq_epi8(_mm256_max_epu8(x, _mm256_set1_epi8(lo as i8)), x);
                    let le = |hi: u8| _mm256_cmpeq_epi8(_mm256_min_epu8(x, _mm256_set1_epi8(hi as i8)), x);

                    let is_az = _mm256_and_si256(ge(b'a'), le(b'z'));
                    let is_az_upper = _mm256_and_si256(ge(b'A'), le(b'Z'));
                    let is_09 = _mm256_and_si256(ge(b'0'), le(b'9'));
                    let is_us = _mm256_cmpeq_epi8(x, _mm256_set1_epi8(b'_' as i8));

                    let ok = _mm256_or_si256(
                        _mm256_or_si256(is_az, is_az_upper),
                        _mm256_or_si256(is_09, is_us),
                    );

                    let mask_ok = _mm256_movemask_epi8(ok) as u32;

                    if mask_ok == 0xFFFF_FFFF {
                        self.0.i += 32;
                    } else {
                        self.0.i += (!mask_ok).trailing_zeros() as usize;
                        return;
                    }
                }
            }
            // Phase 2: Scalar - handle the remaining bytes one by one.
            self.0.i += 1;
        }
    }
}

impl<'a> CommonLexer<'a> for Avx2Lexer<'a> {
    #[inline(always)] fn bytes(&self) -> &'a[u8] { self.0.s }
    #[inline(always)] fn pos(&self) -> usize { self.0.i }
    #[inline(always)] fn set_pos(&mut self, v: usize) { self.0.i = v; }

    #[inline(always)]
    fn skip_ws(&mut self) {
        #[cfg(target_arch = "x86_64")]
        unsafe { self.skip_ws_avx2(); }
    }

    #[inline(always)]
    fn scan_ident_tail(&mut self) {
        // Short-circuit for small remaining input to avoid SIMD overhead.
        if self.0.n - self.0.i < 32 {
            while self.0.i < self.0.n && CoreLexer::is_ident_cont(self.0.s[self.0.i]) {
                self.0.i += 1;
            }
            return;
        }
        #[cfg(target_arch = "x86_64")]
        unsafe { self.scan_ident_tail_avx2(); }
    }
}

// --- SSE2 ---
impl<'a> Sse2Lexer<'a> {
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sse2")]
    unsafe fn skip_ws_sse2(&mut self) {
        use std::arch::x86_64::*;
        // Phase 1: SIMD - process 16 bytes at a time until we find a non-whitespace character or run out of input.
        while self.0.i + 16 <= self.0.n {
            let p = self.0.s.as_ptr().add(self.0.i) as *const __m128i;
            let chunk = _mm_loadu_si128(p);

            let sp = _mm_cmpeq_epi8(chunk, _mm_set1_epi8(b' ' as i8));
            let tb = _mm_cmpeq_epi8(chunk, _mm_set1_epi8(b'\t' as i8));
            let nl = _mm_cmpeq_epi8(chunk, _mm_set1_epi8(b'\n' as i8));
            let cr = _mm_cmpeq_epi8(chunk, _mm_set1_epi8(b'\r' as i8));

            let ws = _mm_or_si128(_mm_or_si128(sp, tb), _mm_or_si128(nl, cr));
            let mask = _mm_movemask_epi8(ws) as u32;

            if (mask & 0xFFFF) == 0xFFFF {
                self.0.i += 16;
            } else {
                let not_ws = (!mask) & 0xFFFF;
                self.0.i += not_ws.trailing_zeros() as usize;
                return;
            }
        }
        // Phase 2: Scalar - handle the remaining bytes one by one.
        while self.0.i < self.0.n {
            match self.0.s[self.0.i] {
                b' ' | b'\t' | b'\n' | b'\r' => self.0.i += 1,
                _ => return,
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "sse2")]
    unsafe fn scan_ident_tail_sse2(&mut self) {
        use std::arch::x86_64::*;
        while self.0.i < self.0.n {
            if !CoreLexer::is_ident_cont(self.0.s[self.0.i]) {
                return;
            }

            let rem = self.0.n - self.0.i;
            if rem < 16 {
                while self.0.i < self.0.n && CoreLexer::is_ident_cont(self.0.s[self.0.i]) {
                    self.0.i += 1;
                }
                return;
            }

            let p = self.0.s.as_ptr().add(self.0.i) as *const __m128i;
            let x = _mm_loadu_si128(p);

            let ge = |lo: u8| _mm_cmpeq_epi8(_mm_max_epu8(x, _mm_set1_epi8(lo as i8)), x);
            let le = |hi: u8| _mm_cmpeq_epi8(_mm_min_epu8(x, _mm_set1_epi8(hi as i8)), x);

            let is_az = _mm_and_si128(ge(b'a'), le(b'z'));
            let is_az_upper = _mm_and_si128(ge(b'A'), le(b'Z'));
            let is_09 = _mm_and_si128(ge(b'0'), le(b'9'));
            let is_us = _mm_cmpeq_epi8(x, _mm_set1_epi8(b'_' as i8));

            let ok = _mm_or_si128(_mm_or_si128(is_az, is_az_upper), _mm_or_si128(is_09, is_us));
            let mask_ok = _mm_movemask_epi8(ok) as u32;

            if (mask_ok & 0xFFFF) == 0xFFFF {
                self.0.i += 16;
            } else {
                let not_ok = (!mask_ok) & 0xFFFF;
                let adv = not_ok.trailing_zeros() as usize;
                self.0.i += adv;
                return;
            }
        }
    }
}

impl<'a> CommonLexer<'a> for Sse2Lexer<'a> {
    #[inline(always)] fn bytes(&self) -> &'a[u8] { self.0.s }
    #[inline(always)] fn pos(&self) -> usize { self.0.i }
    #[inline(always)] fn set_pos(&mut self, v: usize) { self.0.i = v; }

    #[inline(always)]
    fn skip_ws(&mut self) {
        #[cfg(target_arch = "x86_64")]
        unsafe { self.skip_ws_sse2(); }
    }

    #[inline(always)]
    fn scan_ident_tail(&mut self) {
        // Short-circuit for small remaining input to avoid SIMD overhead.
        if self.0.n - self.0.i < 16 {
            while self.0.i < self.0.n && CoreLexer::is_ident_cont(self.0.s[self.0.i]) {
                self.0.i += 1;
            }
            return;
        }
        #[cfg(target_arch = "x86_64")]
        unsafe { self.scan_ident_tail_sse2(); }
    }
}

// --- SCALAR FALLBACK ---
impl<'a> CommonLexer<'a> for ScalarLexer<'a> {
    #[inline(always)] fn bytes(&self) -> &'a[u8] { self.0.s }
    #[inline(always)] fn pos(&self) -> usize { self.0.i }
    #[inline(always)] fn set_pos(&mut self, v: usize) { self.0.i = v; }

    #[inline(always)]
    fn skip_ws(&mut self) {
        while self.0.i < self.0.n {
            match self.0.s[self.0.i] {
                b' ' | b'\t' | b'\n' | b'\r' => self.0.i += 1,
                _ => break,
            }
        }
    }

    #[inline(always)]
    fn scan_ident_tail(&mut self) {
        while self.0.i < self.0.n && CoreLexer::is_ident_cont(self.0.s[self.0.i]) {
            self.0.i += 1;
        }
    }
}