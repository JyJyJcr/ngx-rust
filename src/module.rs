use crate::{
    core::NGX_CONF_ERROR,
    ffi::{
        nginx_version, ngx_command_t, ngx_conf_t, ngx_cycle_t, ngx_int_t, ngx_log_t, ngx_module_t, ngx_str_t,
        ngx_uint_t, NGX_RS_MODULE_SIGNATURE,
    },
    ngx_null_command,
    util::{ConstArrayBuilder, StaticRefMut},
};
use ::core::{
    ffi::{c_char, c_void, CStr},
    marker::PhantomData,
    ptr::null_mut,
};

/// default (unimplemented) `ngx_module_t` template.
pub const NGX_MODULE_EMPTY: ngx_module_t = ngx_module_t {
    ctx_index: ngx_uint_t::MAX,
    index: ngx_uint_t::MAX,
    name: null_mut(),
    spare0: 0,
    spare1: 0,
    version: nginx_version as ngx_uint_t,
    signature: NGX_RS_MODULE_SIGNATURE.as_ptr() as *const _,

    ctx: null_mut(),
    commands: null_mut(),
    type_: 0,

    init_master: None,
    init_module: None,
    init_process: None,
    init_thread: None,
    exit_thread: None,
    exit_process: None,
    exit_master: None,

    spare_hook0: 0,
    spare_hook1: 0,
    spare_hook2: 0,
    spare_hook3: 0,
    spare_hook4: 0,
    spare_hook5: 0,
    spare_hook6: 0,
    spare_hook7: 0,
};

/// Type safe interface expressing unique Nginx module.
pub trait Module: Sized + 'static {
    /// Wrapper of static mutable `NgxModule` object expressing this module.
    const SELF: StaticRefMut<NgxModule<Self>>;
    /// CStr module name expression.
    const NAME: &'static CStr;
    /// Module sigunature.
    const TYPE: ModuleSignature;
    /// Module context type.
    type Ctx: 'static;
    ///  Wrapper of static mutable `NgxModuleCtx` object bound to this module as context object.
    const CTX: StaticRefMut<NgxModuleCtx<Self>>;
    ///  Wrapper of static mutable `NgxModuleCommands` object bound to this module.
    const COMMANDS: NgxModuleCommandsRefMut<Self>;

    /// Type deligating `init_master` (not called now).
    type MasterInitializer: PreCycleDelegate;
    /// Type deligating `init_module` and `exit_master`.
    type ModuleDelegate: CycleDelegate;
    /// Type deligating `init_process` and `exit_process`.
    type ProcessDelegate: CycleDelegate;
    /// Type deligating `init_thread` and `exit_thread` (not called now).
    type ThreadDelegate: CycleDelegate;
}

/// Type safe wrapper of `ngx_module_t` by specifying `Module`.
pub struct NgxModule<M: Module>(ngx_module_t, PhantomData<M>);
impl<M: Module> NgxModule<M> {
    /// Construct this type.
    pub const fn new() -> Self {
        Self(
            ngx_module_t {
                ctx: M::CTX.to_mut_ptr() as *mut _,
                commands: &raw mut unsafe { &mut M::COMMANDS.1.to_mut() }[0],
                type_: M::TYPE.to_ngx_uint(),

                init_master: M::MasterInitializer::INIT,
                init_module: M::ModuleDelegate::INIT,
                init_process: M::ProcessDelegate::INIT,
                init_thread: M::ThreadDelegate::INIT,
                exit_thread: M::ThreadDelegate::EXIT,
                exit_process: M::ProcessDelegate::EXIT,
                exit_master: M::ModuleDelegate::EXIT,

                ..NGX_MODULE_EMPTY
            },
            PhantomData,
        )
    }
}

/// Module type signature.
#[derive(Debug, Clone, Copy)]
pub struct ModuleSignature(ngx_uint_t);
impl ModuleSignature {
    /// Constructs this type from raw ngx_uint_t signature.
    ///
    /// # Safety
    /// Caller must provide a valid module type signature.
    pub const unsafe fn from_ngx_uint(signature: ngx_uint_t) -> Self {
        Self(signature)
    }
    const fn to_ngx_uint(self) -> ngx_uint_t {
        self.0
    }
}

/// Type safe wrapper of ctx by specifying `Module`.
pub struct NgxModuleCtx<M: Module>(M::Ctx);
impl<M: Module> NgxModuleCtx<M> {
    /// Construct this type from raw ctx value.
    pub const unsafe fn from_raw(inner: M::Ctx) -> Self {
        Self(inner)
    }
}

/// Reference to static mutable `NgxModuleCommands` object ignoring the length.
pub struct NgxModuleCommandsRefMut<M: Module>(PhantomData<M>, StaticRefMut<[ngx_command_t]>);
impl<M: Module> NgxModuleCommandsRefMut<M> {
    /// Wrap a static mutable reference to `NgxModuleCommands` into this type.
    ///
    /// # Safety
    /// Caller must ensure that the provided reference is to static mutable.
    pub const unsafe fn from_mut<const N: usize>(
        r: &'static mut NgxModuleCommands<M, N>,
    ) -> NgxModuleCommandsRefMut<M> {
        NgxModuleCommandsRefMut(PhantomData, StaticRefMut::from_mut(&mut r.0))
    }
}

/// Type safe wrapper of \[ngx_command_t\] by specifying `Module`.
pub struct NgxModuleCommands<M: Module, const N: usize>([ngx_command_t; N], PhantomData<M>);

/// `NgxModuleCommands` builder.
pub struct NgxModuleCommandsBuilder<M: Module, const N: usize>(ConstArrayBuilder<ngx_command_t, N>, PhantomData<M>);
impl<M: Module, const N: usize> NgxModuleCommandsBuilder<M, N> {
    /// Constructs new, empty builder with capacity size = N..
    pub const fn new() -> Self {
        Self(ConstArrayBuilder::new(), PhantomData)
    }
    /// Add type implementing `Command` which is consistent with `M:Module` into array.
    pub const fn add<C: Command>(mut self) -> Self
    where
        C::CallRule: CommandCallRuleBy<M>,
    {
        self.0 = self.0.push(command::<M, C>());
        self
    }
    /// Build `NgxModuleCommands`.
    pub const fn build(self) -> NgxModuleCommands<M, N> {
        NgxModuleCommands(self.0.push(ngx_null_command!()).build(), PhantomData)
    }
}

/// Type safe command interface.
pub trait Command {
    /// Type expressing call rule.
    type CallRule: CommandCallRule;
    /// Command name.
    const NAME: ngx_str_t;
    /// Context Flags.
    const CONTEXT_FLAG: CommandContextFlagSet;
    /// Arg Flags.
    const ARG_FLAG: CommandArgFlagSet;
    /// handle command directive
    fn handler(cf: &mut ngx_conf_t, conf: &mut <Self::CallRule as CommandCallRule>::Conf) -> Result<(), ()>;
}

/// Command call interface containing information for proper call.
pub trait CommandCallRule {
    /// Conf type.
    type Conf;
}

/// Command call interface containing information for proper call by M:`Module`.
pub trait CommandCallRuleBy<M: Module>: CommandCallRule {
    /// Offset.
    const OFFSET: CommandOffset;
}

/// Offset of `Command::Conf` in Nginx configuration struct.
pub struct CommandOffset(ngx_uint_t);
impl CommandOffset {
    /// Constructs this type from raw ngx_uint_t offset.
    ///
    /// # Safety
    /// Caller must provide a valid offset.
    pub const unsafe fn from_ngx_uint(offset: ngx_uint_t) -> Self {
        Self(offset)
    }
    const fn to_ngx_uint(self) -> ngx_uint_t {
        self.0
    }
}

use crate::ffi::{NGX_CONF_TAKE1, NGX_CONF_TAKE2, NGX_HTTP_LOC_CONF, NGX_HTTP_MAIN_CONF, NGX_HTTP_SRV_CONF};

/// Flag expressing positions the command directive can appear.
pub enum CommandContextFlag {
    /// Http main block.
    HttpMain,
    /// Http server block.
    HttpSrv,
    /// Http location block.
    HttpLoc,
}

/// Set of `CommandContextFlag`.
#[derive(Debug, Clone, Copy)]
pub struct CommandContextFlagSet(ngx_uint_t);
impl CommandContextFlagSet {
    /// Constructs empty flag set.
    pub const fn empty() -> Self {
        Self(0)
    }
    /// Add (bitor) a flag into this set.
    pub const fn union(self, f: CommandContextFlag) -> Self {
        use CommandContextFlag::*;
        let b: ngx_uint_t = match f {
            HttpMain => NGX_HTTP_MAIN_CONF as ngx_uint_t,
            HttpSrv => NGX_HTTP_SRV_CONF as ngx_uint_t,
            HttpLoc => NGX_HTTP_LOC_CONF as ngx_uint_t,
        };
        Self(self.0 | b)
    }
    const fn to_ngx_uint(self) -> ngx_uint_t {
        self.0
    }
}

/// Flag expressing argument format rule the command directive can take.
pub enum CommandArgFlag {
    /// Take 1 argument.
    Take1,
    /// Take 2 arguments.
    Take2,
}

/// Set of `CommandArgFlag`.
#[derive(Debug, Clone, Copy)]
pub struct CommandArgFlagSet(ngx_uint_t);
impl CommandArgFlagSet {
    /// Constructs empty flag set.
    pub const fn empty() -> Self {
        Self(0)
    }
    /// Add (bitor) a flag into this set.
    pub const fn union(self, f: CommandArgFlag) -> Self {
        use CommandArgFlag::*;
        let b: ngx_uint_t = match f {
            Take1 => NGX_CONF_TAKE1 as ngx_uint_t,
            Take2 => NGX_CONF_TAKE2 as ngx_uint_t,
        };
        Self(self.0 | b)
    }
    const fn to_ngx_uint(self) -> ngx_uint_t {
        self.0
    }
}

/// Construct `ContextFlag` set.
#[macro_export]
macro_rules! context_flags {
    ($($flag:expr),*) => {
        $crate::module::CommandContextFlagSet::empty()$(.union($flag))*
    };
}

/// Construct `ArgFlag` set.
#[macro_export]
macro_rules! arg_flags {
    ($($flag:expr ),*) => {
        $crate::module::CommandArgFlagSet::empty()$(.union($flag))*
    };
}

const fn command<M: Module, C: Command>() -> ngx_command_t
where
    C::CallRule: CommandCallRuleBy<M>,
{
    ngx_command_t {
        name: C::NAME,
        type_: C::CONTEXT_FLAG.to_ngx_uint() | C::ARG_FLAG.to_ngx_uint(),
        set: Some(command_handler::<M, C>),
        conf: <C::CallRule as CommandCallRuleBy<M>>::OFFSET.to_ngx_uint(),
        offset: 0,
        post: null_mut(),
    }
}

unsafe extern "C" fn command_handler<M: Module, C: Command>(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    if C::handler(&mut *cf, &mut *(conf as *mut _)).is_ok() {
        // NGX_CONF_OK not impled yet, but nullptr = 0 is same as NGX_CONF_OK
        return null_mut();
    }
    NGX_CONF_ERROR as *mut c_char
}

/// Delegete type of pre-cycle delegate.
pub trait PreCycleDelegate {
    /// initialize in pre-cycle.
    fn init(log: &mut ngx_log_t) -> ngx_int_t;
    /// Unsafe `init`` wrapper for pointer usage
    unsafe extern "C" fn init_unsafe(log: *mut ngx_log_t) -> ngx_int_t {
        Self::init(&mut *log)
    }
    /// Nullable init function pointer actually called.
    const INIT: Option<unsafe extern "C" fn(*mut ngx_log_t) -> ngx_int_t> = Some(Self::init_unsafe);
}
impl PreCycleDelegate for () {
    fn init(_log: &mut ngx_log_t) -> ngx_int_t {
        unimplemented!()
    }
    const INIT: Option<unsafe extern "C" fn(*mut ngx_log_t) -> ngx_int_t> = None;
}

/// Delegete type of in-cycle delegate.
pub trait CycleDelegate {
    /// initialize in cycle start time.
    fn init(cycle: &mut ngx_cycle_t) -> ngx_int_t;
    /// finalize in cycle end time.
    fn exit(cycle: &mut ngx_cycle_t);
    /// Unsafe `init`` wrapper for pointer usage
    unsafe extern "C" fn init_unsafe(cycle: *mut ngx_cycle_t) -> ngx_int_t {
        Self::init(&mut *cycle)
    }
    /// Unsafe `exit`` wrapper for pointer usage
    unsafe extern "C" fn exit_unsafe(cycle: *mut ngx_cycle_t) {
        Self::exit(&mut *cycle)
    }
    /// Nullable init function pointer actually called.
    const INIT: Option<unsafe extern "C" fn(*mut ngx_cycle_t) -> ngx_int_t> = Some(Self::init_unsafe);
    /// Nullable exit function pointer actually called.
    const EXIT: Option<unsafe extern "C" fn(*mut ngx_cycle_t)> = Some(Self::exit_unsafe);
}

impl CycleDelegate for () {
    fn init(_cycle: &mut ngx_cycle_t) -> ngx_int_t {
        unimplemented!()
    }
    fn exit(_cycle: &mut ngx_cycle_t) {
        unimplemented!()
    }
    const INIT: Option<unsafe extern "C" fn(*mut ngx_cycle_t) -> ngx_int_t> = None;
    const EXIT: Option<unsafe extern "C" fn(*mut ngx_cycle_t)> = None;
}

/// Exhibit modules exported by this library.
///
/// These are normally generated by the Nginx module system, but need to be
/// defined when building modules outside of it.
#[macro_export]
macro_rules! exhibit_modules {
    ($( $mod:ty ),+) => {
        #[no_mangle]
        #[allow(non_upper_case_globals)]
        pub static mut ngx_modules: [*const $crate::ffi::ngx_module_t; $( {let _:[$mod;0]; 1}+ )+ 1] =
        $crate::module::__macro::NgxModulesBuilder::new()$(.add::<$mod>())+ .build();

        #[no_mangle]
        #[allow(non_upper_case_globals)]
        pub static mut ngx_module_names: [*const $crate::module::__macro::c_char; $( {let _:[$mod;0]; 1}+ ) + 1] =
        $crate::module::__macro::NgxModuleNamesBuilder::new()$(.add::<$mod>())+ .build();

        #[no_mangle]
        #[allow(non_upper_case_globals)]
        pub static mut ngx_module_order: [*const ::core::ffi::c_char; 1] = [
            $crate::module::__macro::null()
        ];
    };
}

#[doc(hidden)]
pub mod __macro {
    pub use core::{ffi::c_char, ptr::null};

    use crate::{ffi::ngx_module_t, util::ConstArrayBuilder};

    use super::Module;

    pub struct NgxModulesBuilder<const N: usize>(ConstArrayBuilder<*const ngx_module_t, N>);
    impl<const N: usize> NgxModulesBuilder<N> {
        pub const fn new() -> Self {
            Self(ConstArrayBuilder::new())
        }
        pub const fn add<M: Module>(mut self) -> Self {
            self.0 = self.0.push(unsafe { &raw const M::SELF.to_ref().0 });
            self
        }
        pub const fn build(self) -> [*const ngx_module_t; N] {
            self.0.push(null()).build()
        }
    }
    pub struct NgxModuleNamesBuilder<const N: usize>(ConstArrayBuilder<*const c_char, N>);
    impl<const N: usize> NgxModuleNamesBuilder<N> {
        pub const fn new() -> Self {
            Self(ConstArrayBuilder::new())
        }
        pub const fn add<M: Module>(mut self) -> Self {
            self.0 = self.0.push(M::NAME.as_ptr());
            self
        }
        pub const fn build(self) -> [*const c_char; N] {
            self.0.push(null()).build()
        }
    }
}
