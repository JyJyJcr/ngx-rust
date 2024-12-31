mod const_array;
#[cfg(static_ref_mut)]
mod static_ref;

pub use const_array::*;
#[cfg(static_ref_mut)]
pub use static_ref::*;
