//! Bounded scratch storage; exhaustion falls back to ordinary grammar parsing.
use super::Field;

const RESERVE: usize = 16 * 1024 * 1024;
const MAX_FIELDS: usize = RESERVE / std::mem::size_of::<Field<'static>>();

#[derive(Default)]
pub(super) struct FieldBuffer<'a>(Vec<Field<'a>>);

impl<'a> FieldBuffer<'a> {
    pub fn try_push(&mut self, field: Field<'a>) -> bool {
        if self.0.len() == MAX_FIELDS {
            return false;
        }
        self.0.push(field);
        true
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Field<'a>> {
        self.0.iter_mut()
    }
    #[cfg(test)]
    pub fn as_ptr(&self) -> *const Field<'a> {
        self.0.as_ptr()
    }
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scratch_boundaries_exhaustion_and_reuse_preserve_values() {
        let mut buffer = FieldBuffer::default();
        for i in 0..MAX_FIELDS {
            assert!(buffer.try_push(("n", Some(crate::ast::Value::Num(i as f64)))));
        }
        assert!(!buffer.try_push(("overflow", None)));
        for (i, (_, value)) in buffer.iter_mut().enumerate() {
            assert_eq!(*value, Some(crate::ast::Value::Num(i as f64)));
        }
        let pointer = buffer.as_ptr();
        buffer.clear();
        assert!(buffer.is_empty());
        assert!(buffer.try_push((
            "text",
            Some(crate::ast::Value::Str("owned".to_owned().into()))
        )));
        assert_eq!(buffer.as_ptr(), pointer);
        assert_eq!(buffer.iter_mut().next().unwrap().0, "text");
    }
}
