use core::ptr::NonNull;

#[derive(Debug, Clone, Copy)]
/// Static immutable reference wrapper.
pub struct StaticRef<E: 'static + ?Sized>(NonNull<E>);
impl<E: 'static + ?Sized> StaticRef<E> {
    /// Wrap a static immutable reference into this type.
    ///
    /// # Safety
    /// Caller must ensure that the provided reference is to static immutable.
    pub const unsafe fn from_ref(e: &'static E) -> Self {
        Self(unsafe { NonNull::new_unchecked(e as *const _ as *mut _) })
    }
    /// Converts this type into a static immutable reference.
    pub const fn to_ref(self) -> &'static E {
        unsafe { self.0.as_ref() }
    }
    /// Converts this type into an immutable raw pointer.
    pub const fn to_ptr(self) -> *const E {
        self.0.as_ptr()
    }
}
#[derive(Debug, Clone, Copy)]
/// Static mutable reference wrapper.
pub struct StaticRefMut<E: 'static + ?Sized>(NonNull<E>);
impl<E: 'static + ?Sized> StaticRefMut<E> {
    /// Wrap a static mutable reference into this type.
    ///
    /// # Safety
    /// Caller must ensure that the provided reference is to static mutable.
    pub const unsafe fn from_mut(e: &'static mut E) -> Self {
        Self(unsafe { NonNull::new_unchecked(e) })
    }
    /// Converts this type into a static immutable reference.
    ///
    /// # Safety
    /// This sunction is marked unsafe because provide a reference to static mutable
    pub const unsafe fn to_ref(self) -> &'static E {
        unsafe { self.0.as_ref() }
    }
    /// Converts this type into a static mutable reference.
    ///
    /// # Safety
    /// This sunction is marked unsafe because provide a reference to static mutable
    pub const unsafe fn to_mut(mut self) -> &'static mut E {
        self.0.as_mut()
    }
    /// Converts this type into an immutable raw pointer.
    pub const fn to_ptr(self) -> *const E {
        self.0.as_ptr()
    }
    /// Converts this type into a mutable raw pointer.
    pub const fn to_mut_ptr(self) -> *mut E {
        self.0.as_ptr()
    }
}

// pub trait Tied<E: 'static + ?Sized> {
//     const ENTITY: StaticRef<E>;
// }
// pub const unsafe fn entity_ref<T: Tied<E>, E: 'static + ?Sized>() -> &'static E {
//     <T as Tied<_>>::ENTITY.as_ref()
// }
// pub const fn entity_ptr<T: Tied<E>, E: 'static + ?Sized>() -> *const E {
//     <T as Tied<_>>::ENTITY.as_ptr()
// }

// pub trait TiedMut<E: 'static + ?Sized> {
//     const ENTITY: StaticRefMut<E>;
// }
// pub const unsafe fn mut_entity_ref<T: TiedMut<E>, E: 'static + ?Sized>() -> &'static E {
//     <T as TiedMut<_>>::ENTITY.as_ref()
// }
// pub const unsafe fn mut_entity_mut<T: TiedMut<E>, E: 'static + ?Sized>() -> &'static mut E {
//     <T as TiedMut<_>>::ENTITY.as_mut()
// }
// pub const fn mut_entity_ptr<T: TiedMut<E>, E: 'static + ?Sized>() -> *const E {
//     <T as TiedMut<_>>::ENTITY.as_ptr()
// }
// pub const fn mut_entity_mut_ptr<T: TiedMut<E>, E: 'static + ?Sized>() -> *mut E {
//     <T as TiedMut<_>>::ENTITY.as_mut_ptr()
// }

#[cfg(test)]
mod test {
    use std::ptr::{addr_of, addr_of_mut};

    use super::{StaticRef, StaticRefMut};

    // const X: i32 = 30;
    // const REF_X: StaticRef<i32> = unsafe { StaticRef::wrap(&mut *core::ptr::addr_of_mut!(X)) }; //compile error
    // const REF_X_MUT: StaticRefMut<i32> = unsafe { StaticRefMut::wrap(&mut *core::ptr::addr_of_mut!(X)) }; //compile error

    static A: i32 = 20;
    const A_REF: StaticRef<i32> = unsafe { StaticRef::from_ref(&*addr_of!(A)) };
    //const REF_B_MUT: StaticRefMut<i32> = unsafe { StaticRefMut::wrap(&mut *core::ptr::addr_of_mut!(B)) }; //compile error

    static mut B: i32 = 10;
    const B_REF: StaticRef<i32> = unsafe { StaticRef::from_ref(&mut *addr_of_mut!(B)) };
    const B_MUT: StaticRefMut<i32> = unsafe { StaticRefMut::from_mut(&mut *addr_of_mut!(B)) };

    #[allow(unused_variables)]
    #[allow(dead_code)]
    fn static_ref_compile_test() {
        // immutable static access is safe.
        let a_value = A;
        let a_ref: &_ = &A;
        // StaticRef follow the rule.
        let a_ref_from_ref: &_ = A_REF.to_ref();

        // mutable static access is always unsafe.
        let a_value = unsafe { B }; // actually this is Copy trait
        let a_ref: &_ = unsafe { &*addr_of!(B) };
        let a_mut: &mut _ = unsafe { &mut *addr_of_mut!(B) };
        // StaticRefMut follow the rule.
        let a_ref_from_mut: &_ = unsafe { B_MUT.to_ref() };
        let a_mut_from_mut: &mut _ = unsafe { B_MUT.to_mut() };
        // but B_REF provide reference in safe function!
        let a_ref_from_ref: &_ = B_REF.to_ref();
        // that's why from_ref() is made unsafe. caller must provide correct ptr;

        // get ptr is safe, since ptr itself is unsafe object.
        let a_ptr_const: *const _ = &raw const A;
        let b_ptr_const: *const _ = &raw const B;
        let b_ptr_mut: *mut _ = &raw mut B;
        // StaticRef,StaticRefMut follow the rule.
        let a_ptr_const_from_ref: *const _ = A_REF.to_ptr();
        let b_ptr_const_from_mut: *const _ = B_MUT.to_ptr();
        let b_ptr_mut_from_mut: *mut _ = B_MUT.to_mut_ptr();
    }

    // struct TiedI32;
    // impl Tied<i32> for TiedI32 {
    //     const ENTITY: StaticRef<i32> = A_REF;
    // }
    // struct TiedMutI32;
    // impl TiedMut<i32> for TiedMutI32 {
    //     const ENTITY: StaticRefMut<i32> = B_MUT;
    // }

    // #[test]
    // fn tied_test() {
    //     let a_ref = TiedI32::ENTITY.as_ref();
    //     assert_eq!(*a_ref, A);
    //     let b_mut = unsafe { TiedMutI32::ENTITY.as_mut() };
    //     let b_old = *b_mut;
    //     *b_mut += 1;
    //     let b_new = *b_mut;
    //     assert_eq!(b_old + 1, b_new);
    // }
}
