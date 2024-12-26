//! Bindings to NGINX
//! This project provides Rust SDK interfaces to the [NGINX](https://nginx.com) proxy allowing the creation of NGINX
//! dynamic modules completely in Rust.
//!
//! ## Build
//!
//! NGINX modules can be built against a particular version of NGINX. The following environment variables can be used
//! to specify a particular version of NGINX or an NGINX dependency:
//!
//! * `ZLIB_VERSION` (default 1.3.1) - zlib version
//! * `PCRE2_VERSION` (default 10.42 for NGINX 1.22.0 and later, or 8.45 for earlier) - PCRE1 or PCRE2 version
//! * `OPENSSL_VERSION` (default 3.2.1 for NGINX 1.22.0 and later, or 1.1.1w for earlier) - OpenSSL version
//! * `NGX_VERSION` (default 1.26.1) - NGINX OSS version
//! * `NGX_DEBUG` (default to false) -  if set to true, then will compile NGINX `--with-debug` option
//!
//! For example, this is how you would compile the [examples](https://github.com/nginxinc/ngx-rust/tree/master/examples) using a specific version of NGINX and enabling
//! debugging: `NGX_DEBUG=true NGX_VERSION=1.23.0 cargo build --package=examples --examples --release`
//!
//! To build Linux-only modules, use the "linux" feature: `cargo build --package=examples --examples --features=linux --release`
//!
//! After compilation, the modules can be found in the path `target/release/examples/` ( with the `.so` file extension for
//! Linux or `.dylib` for MacOS).
//!
//! Additionally, the folder  `.cache/nginx/{NGX_VERSION}/{OS}/` will contain the compiled version of NGINX used to build
//! the SDK. You can start NGINX directly from this directory if you want to test the module or add it to `$PATH`
//! ```not_rust
//! $ export NGX_VERSION=1.23.3
//! $ cargo build --package=examples --examples --features=linux --release
//! $ export PATH=$PATH:`pwd`/.cache/nginx/$NGX_VERSION/macos-x86_64/sbin
//! $ nginx -V
//! $ ls -la ./target/release/examples/
//! # now you can use dynamic modules with the NGINX
//! ```

// support both std and no_std
#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
/// The core module.
///
/// This module provides fundamental utilities needed to interface with many NGINX primitives.
/// String conversions, the pool (memory interface) object, and buffer APIs are covered here. These
/// utilities will generally align with the NGINX 'core' files and APIs.
pub mod core;

/// The ffi module.
///
/// This module provides scoped FFI bindings for NGINX symbols.
pub mod ffi;

/// The http module.
///
/// This modules provides wrappers and utilities to NGINX http APIs, such as requests,
/// configuration access, and statuses.
pub mod http;

/// The log module.
///
/// This module provides an interface into the NGINX logger framework.
pub mod log;

/// Define modules exported by this library.
///
/// These are normally generated by the Nginx module system, but need to be
/// defined when building modules outside of it.
#[macro_export]
macro_rules! ngx_modules {
    ($( $mod:ident ),+) => {
        #[no_mangle]
        pub static mut ngx_modules: [*const $crate::ffi::ngx_module_t; $crate::count!($( $mod, )+) + 1] = [
            $( unsafe { &$mod } as *const $crate::ffi::ngx_module_t, )+
            ::core::ptr::null()
        ];

        #[no_mangle]
        pub static mut ngx_module_names: [*const ::core::ffi::c_char; $crate::count!($( $mod, )+) + 1] = [
            $( concat!(stringify!($mod), "\0").as_ptr() as *const ::core::ffi::c_char, )+
            ::core::ptr::null()
        ];

        #[no_mangle]
        pub static mut ngx_module_order: [*const ::core::ffi::c_char; 1] = [
            ::core::ptr::null()
        ];
    };
}

/// Count number of arguments
#[macro_export]
macro_rules! count {
    () => { 0usize };
    ($x:tt, $( $xs:tt ),*) => { 1usize + $crate::count!($( $xs, )*) };
}
