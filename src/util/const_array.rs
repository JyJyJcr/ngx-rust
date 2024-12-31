use std::{marker::PhantomData, mem::ManuallyDrop};

/// The `PreArray` trait provides the size of the array if the value is flattened into the array.
pub trait PreArray<T> {
    /// the size of the array if flattened
    const N: usize;
}
impl<T> PreArray<T> for T {
    const N: usize = 1;
}
impl<T, const N: usize> PreArray<T> for [T; N] {
    const N: usize = N;
}

/// Concatenation of two PreArray.
#[repr(C)]
pub struct ConcatPreArray<T, PA1: PreArray<T>, PA2: PreArray<T>> {
    first: PA1,
    second: PA2,
    __: PhantomData<T>,
}
impl<T, Ne1: PreArray<T>, Ne2: PreArray<T>> PreArray<T> for ConcatPreArray<T, Ne1, Ne2> {
    const N: usize = Ne1::N + Ne2::N;
}

/// Array builder available in compile time.
#[repr(C)]
pub struct ConstArrayBuilder<T, PA: PreArray<T>> {
    inner: PA,
    __: PhantomData<T>,
}
impl<T> ConstArrayBuilder<T, [T; 0]> {
    /// Constructs a new, empty builder.
    pub const fn new() -> Self {
        Self {
            inner: [],
            __: PhantomData,
        }
    }
}

union ToPreArray<T, PA: PreArray<T>> {
    from: ManuallyDrop<ConstArrayBuilder<T, PA>>,
    to: ManuallyDrop<PA>,
}
union ToArray<T, PA: PreArray<T>, const N: usize> {
    from: ManuallyDrop<PA>,
    to: ManuallyDrop<[T; N]>,
}

impl<T, PA: PreArray<T>> ConstArrayBuilder<T, PA> {
    const fn to_pre_array(self) -> PA {
        ManuallyDrop::<PA>::into_inner(unsafe {
            ToPreArray::<T, PA> {
                from: ManuallyDrop::new(self),
            }
            .to
        })
    }

    /// Appends a PreArray to the back of the array.
    pub const fn push<PA2: PreArray<T>>(self, elements: PA2) -> ConstArrayBuilder<T, ConcatPreArray<T, PA, PA2>> {
        let pre_array = self.to_pre_array();
        ConstArrayBuilder::<T, ConcatPreArray<T, PA, PA2>> {
            inner: ConcatPreArray::<T, PA, PA2> {
                first: pre_array,
                second: elements,
                __: PhantomData,
            },
            __: PhantomData,
        }
    }

    /// Builds normal array.
    pub const fn build<const N: usize>(self) -> [T; N] {
        let pre_array = self.to_pre_array();
        if PA::N != N || size_of::<PA>() != size_of::<[T; N]>() {
            panic!("size inconsistency: you should specify correct array size")
        }

        ManuallyDrop::<[T; N]>::into_inner(unsafe {
            ToArray::<T, PA, N> {
                from: ManuallyDrop::new(pre_array),
            }
            .to
        })
    }
}

union ToBuilder<T, PA: PreArray<T>, B> {
    from: ManuallyDrop<B>,
    to: ManuallyDrop<ConstArrayBuilder<T, PA>>,
}

/// Convert builder wrapper to get raw builder.
/// This is the workaround for overstrict drop check in const fn (see example in https://github.com/rust-lang/rust/issues/115403)
pub(crate) const unsafe fn unwrap_builder<T, PA: PreArray<T>, B>(builder: B) -> ConstArrayBuilder<T, PA> {
    if size_of::<B>() != size_of::<ConstArrayBuilder<T, PA>>() {
        panic!("size inconsistency: you should specify correct array size")
    }
    ManuallyDrop::<ConstArrayBuilder<T, PA>>::into_inner(
        ToBuilder::<T, PA, B> {
            from: ManuallyDrop::new(builder),
        }
        .to,
    )
}

#[cfg(test)]
mod test {
    use super::ConstArrayBuilder;

    const ARR: [u32; 6] = ConstArrayBuilder::new().push(0).push([1, 2]).push([3, 4, 5]).build();

    #[test]
    fn arr_test() {
        assert_eq!(ARR, [0, 1, 2, 3, 4, 5]);
    }
}
