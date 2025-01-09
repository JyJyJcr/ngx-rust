use std::mem::{ManuallyDrop, MaybeUninit};

/// Array builder available in compile time.
pub struct ConstArrayBuilder<T, const N: usize> {
    len: usize,
    inner: [MaybeUninit<T>; N],
}
impl<T, const N: usize> ConstArrayBuilder<T, N> {
    /// Constructs a new, empty builder with capacity size = N.
    pub const fn new() -> Self {
        Self {
            len: 0,
            inner: [const { MaybeUninit::uninit() }; N],
        }
    }
    /// Appends a PreArray to the back of the array.
    pub const fn push(mut self, elements: T) -> Self {
        if self.len >= N {
            panic!("size inconsistency: you should specify correct array size")
        }
        self.inner[self.len] = MaybeUninit::new(elements);
        self.len += 1;
        self
    }

    /// Builds normal array.
    pub const fn build(self) -> [T; N] {
        if self.len != N {
            panic!("size inconsistency: you should specify correct array size")
        }
        union UnwrapInner<T, const N: usize> {
            from: ManuallyDrop<[MaybeUninit<T>; N]>,
            to: ManuallyDrop<[T; N]>,
        }
        ManuallyDrop::into_inner(unsafe {
            UnwrapInner {
                from: ManuallyDrop::new(self.inner),
            }
            .to
        })
    }
}

#[cfg(test)]
mod test {
    use super::ConstArrayBuilder;

    const ARR: [u32; 3] = ConstArrayBuilder::new().push(0).push(1).push(2).build();

    #[test]
    fn arr_test() {
        assert_eq!(ARR, [0, 1, 2,]);
    }
}
