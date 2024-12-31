mod const_array;
#[cfg(feature = "static_ref")]
mod static_ref;

pub use const_array::*;
#[cfg(feature = "static_ref")]
pub use static_ref::*;
