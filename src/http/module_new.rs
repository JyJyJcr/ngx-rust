use nginx_sys::{
    ngx_array_push, ngx_conf_t, ngx_http_core_module, ngx_http_handler_pt, ngx_http_phases,
    ngx_http_phases_NGX_HTTP_ACCESS_PHASE, ngx_http_phases_NGX_HTTP_CONTENT_PHASE,
    ngx_http_phases_NGX_HTTP_FIND_CONFIG_PHASE, ngx_http_phases_NGX_HTTP_LOG_PHASE,
    ngx_http_phases_NGX_HTTP_POST_ACCESS_PHASE, ngx_http_phases_NGX_HTTP_POST_READ_PHASE,
    ngx_http_phases_NGX_HTTP_POST_REWRITE_PHASE, ngx_http_phases_NGX_HTTP_PREACCESS_PHASE,
    ngx_http_phases_NGX_HTTP_PRECONTENT_PHASE, ngx_http_phases_NGX_HTTP_REWRITE_PHASE,
    ngx_http_phases_NGX_HTTP_SERVER_REWRITE_PHASE, ngx_http_request_t, ngx_int_t, ngx_uint_t,
    NGX_RS_HTTP_LOC_CONF_OFFSET, NGX_RS_HTTP_MAIN_CONF_OFFSET, NGX_RS_HTTP_SRV_CONF_OFFSET,
};

use crate::core::{Pool, Status, NGX_CONF_ERROR};
use crate::ffi::{ngx_http_module_t, NGX_HTTP_MODULE};
use crate::module::{
    CommandCallRule, CommandCallRuleBy, CommandOffset, CycleDelegate, Module, ModuleSignature, NgxModule,
    NgxModuleCommands, NgxModuleCommandsRefMut, NgxModuleCtx, PreCycleDelegate,
};
use crate::util::StaticRefMut;
use ::core::marker::PhantomData;
use std::ffi::{c_char, c_void, CStr};
use std::ptr::{addr_of, null_mut};

use super::{Merge, MergeConfigError, Request};

/// Wrapper of `HttpModule` implementing `Module`.
pub struct HttpModuleSkel<M: HttpModule>(PhantomData<M>);
impl<M: HttpModule> Module for HttpModuleSkel<M> {
    const SELF: StaticRefMut<NgxModule<Self>> = unsafe { StaticRefMut::from_mut(&mut M::SELF.to_mut().0) };
    const NAME: &'static CStr = M::NAME;
    const TYPE: ModuleSignature = unsafe { ModuleSignature::from_ngx_uint(NGX_HTTP_MODULE as ngx_uint_t) };
    type Ctx = ngx_http_module_t;
    const CTX: StaticRefMut<NgxModuleCtx<Self>> = unsafe { StaticRefMut::from_mut(&mut M::SELF.to_mut().1) };
    const COMMANDS: NgxModuleCommandsRefMut<Self> = M::COMMANDS;

    type MasterInitializer = M::MasterInitializer;
    type ModuleDelegate = M::ModuleDelegate;
    type ProcessDelegate = M::ProcessDelegate;
    type ThreadDelegate = M::ThreadDelegate;
}

/// Type safe wrapper of `ngx_module_t` and `ngx_http_module_t` by specifying `Module`.
pub struct NgxHttpModule<M: HttpModule>(NgxModule<HttpModuleSkel<M>>, NgxModuleCtx<HttpModuleSkel<M>>);
impl<M: HttpModule> Default for NgxHttpModule<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: HttpModule> NgxHttpModule<M> {
    /// Construct this type.
    pub const fn new() -> Self {
        Self(NgxModule::new(), unsafe {
            NgxModuleCtx::from_raw(ngx_http_module_t {
                preconfiguration: M::PreConfiguration::CONFIGURATION,
                postconfiguration: M::PostConfiguration::CONFIGURATION,
                create_main_conf: M::MainConfSetting::CREATE,
                init_main_conf: M::MainConfSetting::INIT,
                create_srv_conf: M::SrvConfSetting::CREATE,
                merge_srv_conf: M::SrvConfSetting::MERGE,
                create_loc_conf: M::LocConfSetting::CREATE,
                merge_loc_conf: M::LocConfSetting::MERGE,
            })
        })
    }
}
/// Type Alias of `NgxModuleCommands` for `HttpModule`.
pub type NgxHttpModuleCommands<M, const N: usize> = NgxModuleCommands<HttpModuleSkel<M>, N>;
/// Type Alias of `NgxModuleCommandsRefMut` for `HttpModule`.
pub type NgxHttpModuleCommandsRefMut<M> = NgxModuleCommandsRefMut<HttpModuleSkel<M>>;

/// Type safe interface expressing unique Nginx Http module.
pub trait HttpModule: Sized + 'static {
    /// Wrapper of static mutable `NgxModule` and `NgxModuleCtx` object expressing this module.
    const SELF: StaticRefMut<NgxHttpModule<Self>>;
    /// CStr module name expression.
    const NAME: &'static CStr;
    ///  Wrapper of static mutable `NgxHttpModuleCommands` object bound to this module.
    const COMMANDS: NgxHttpModuleCommandsRefMut<Self>;

    /// Type deligating `init_master` (not called now).
    type MasterInitializer: PreCycleDelegate;
    /// Type deligating `init_module` and `exit_master`.
    type ModuleDelegate: CycleDelegate;
    /// Type deligating `init_process` and `exit_process`.
    type ProcessDelegate: CycleDelegate;
    /// Type deligating `init_thread` and `exit_thread` (not called now).
    type ThreadDelegate: CycleDelegate;

    /// Type deligating `preconfiguration`.
    type PreConfiguration: ConfigurationDelegate;
    /// Type deligating `postconfiguration`.
    type PostConfiguration: ConfigurationDelegate;

    /// Type deligating `create_main_conf` and `init_main_conf`, and specifying MainConf type.
    type MainConfSetting: InitConfSetting;
    /// Type deligating `create_srv_conf` and `merge_srv_conf`, and specifying SrvConf type.
    type SrvConfSetting: MergeConfSetting;
    /// Type deligating `create_loc_conf` and `merge_loc_conf`, and specifying LocConf type.
    type LocConfSetting: MergeConfSetting;
    /// Context type of Http request bound with this module.
    type Ctx;
}

/// Delegete type of module level configuration.
pub trait ConfigurationDelegate {
    /// Module level configuration.
    fn configuration(cf: &mut ngx_conf_t) -> Result<(), Status>;
    /// Unsafe `configuration` wrapper for pointer usage.
    ///
    /// # Safety
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn configuration_unsafe(cf: *mut ngx_conf_t) -> ngx_int_t {
        Self::configuration(&mut *cf).err().unwrap_or(Status::NGX_OK).into()
    }
    /// Nullable configuration function pointer actually called.
    const CONFIGURATION: Option<unsafe extern "C" fn(*mut ngx_conf_t) -> ngx_int_t> = Some(Self::configuration_unsafe);
}

impl ConfigurationDelegate for () {
    fn configuration(_cf: &mut ngx_conf_t) -> Result<(), Status> {
        unimplemented!()
    }
    const CONFIGURATION: Option<unsafe extern "C" fn(*mut ngx_conf_t) -> ngx_int_t> = None;
}

/// Error for creating conf.
#[derive(Debug)]
pub struct ConfCreateError;
/// Error for initing conf.
#[derive(Debug)]
pub struct ConfInitError;
/// Error for merging conf.
#[derive(Debug)]
pub struct ConfMergeError;
impl From<MergeConfigError> for ConfMergeError {
    fn from(value: MergeConfigError) -> Self {
        match value {
            MergeConfigError::NoValue => ConfMergeError,
        }
    }
}

/// Delegete type of conf object level configuration with init.
pub trait InitConfSetting {
    /// Conf type.
    type Conf;
    /// create conf object.
    fn create(cf: &mut ngx_conf_t) -> Result<Self::Conf, ConfCreateError>;
    /// Unsafe `create` wrapper for pointer usage.
    ///
    /// # Safety
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn create_unsafe(cf: *mut ngx_conf_t) -> *mut c_void {
        let mut pool = Pool::from_ngx_pool((*cf).pool);
        if let Ok(conf) = Self::create(&mut *cf) {
            return pool.allocate(conf) as *mut c_void;
        }
        null_mut()
    }
    /// init conf object.
    fn init(cf: &mut ngx_conf_t, conf: &mut Self::Conf) -> Result<(), ConfInitError>;
    /// Unsafe `init` wrapper for pointer usage.
    ///
    /// # Safety
    /// Callers should provide valid non-null `ngx_conf_t` and `c_void` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn init_unsafe(cf: *mut ngx_conf_t, conf: *mut c_void) -> *mut c_char {
        if Self::init(&mut *cf, &mut *(conf as *mut _)).is_ok() {
            return null_mut();
        }
        NGX_CONF_ERROR as _
    }

    /// Nullable create function pointer actually called.
    const CREATE: Option<unsafe extern "C" fn(*mut ngx_conf_t) -> *mut c_void> = Some(Self::create_unsafe);
    /// Nullable init function pointer actually called.
    const INIT: Option<unsafe extern "C" fn(*mut ngx_conf_t, *mut c_void) -> *mut c_char> = Some(Self::init_unsafe);
}

/// Default implementer of `InitConfSetting` for `Conf: Default`.
pub struct DefaultInit<C: Default>(PhantomData<C>);
impl<C: Default> InitConfSetting for DefaultInit<C> {
    type Conf = C;

    fn create(_cf: &mut ngx_conf_t) -> Result<Self::Conf, ConfCreateError> {
        Ok(Default::default())
    }

    fn init(_cf: &mut ngx_conf_t, _conf: &mut Self::Conf) -> Result<(), ConfInitError> {
        Ok(())
    }
}

/// Delegete type of conf object level configuration with merge.
pub trait MergeConfSetting {
    /// Conf type.
    type Conf;
    /// create conf object.
    fn create(cf: &mut ngx_conf_t) -> Result<Self::Conf, ConfCreateError>;
    /// Unsafe `create` wrapper for pointer usage.
    ///
    /// # Safety
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn create_unsafe(cf: *mut ngx_conf_t) -> *mut c_void {
        let mut pool = Pool::from_ngx_pool((*cf).pool);
        if let Ok(conf) = Self::create(&mut *cf) {
            return pool.allocate(conf) as *mut c_void;
        }
        null_mut()
    }
    /// merge conf objects.
    fn merge(cf: &mut ngx_conf_t, prev: &mut Self::Conf, conf: &mut Self::Conf) -> Result<(), ConfMergeError>;
    /// Unsafe `merge` wrapper for pointer usage.
    ///
    /// # Safety
    /// Callers should provide valid non-null `ngx_conf_t` and `c_void` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn merge_unsafe(cf: *mut ngx_conf_t, prev: *mut c_void, conf: *mut c_void) -> *mut c_char {
        if Self::merge(&mut *cf, &mut *(prev as *mut _), &mut *(conf as *mut _)).is_ok() {
            return null_mut();
        }
        NGX_CONF_ERROR as _
    }
    /// Nullable create function pointer actually called.
    const CREATE: Option<unsafe extern "C" fn(*mut ngx_conf_t) -> *mut c_void> = Some(Self::create_unsafe);
    /// Nullable merge function pointer actually called.
    const MERGE: Option<unsafe extern "C" fn(*mut ngx_conf_t, *mut c_void, *mut c_void) -> *mut c_char> =
        Some(Self::merge_unsafe);
}

/// Default implementer of `InitConfSetting` for `Conf: Default + Merge`.
pub struct DefaultMerge<C: Default + Merge>(PhantomData<C>);
impl<C: Default + Merge> MergeConfSetting for DefaultMerge<C> {
    type Conf = C;

    fn create(_cf: &mut ngx_conf_t) -> Result<Self::Conf, ConfCreateError> {
        Ok(Default::default())
    }

    fn merge(_cf: &mut ngx_conf_t, prev: &mut Self::Conf, conf: &mut Self::Conf) -> Result<(), ConfMergeError> {
        conf.merge(prev).map_err(|e| e.into())
    }
}

/// `CommandCallRule` implementer for `Command` configuring Http Main Conf
pub struct HttpMainConf<C>(PhantomData<C>);
impl<C> CommandCallRule for HttpMainConf<C> {
    type Conf = C;
}

impl<M: HttpModule> CommandCallRuleBy<HttpModuleSkel<M>>
    for HttpMainConf<<M::MainConfSetting as InitConfSetting>::Conf>
{
    const OFFSET: CommandOffset = unsafe { CommandOffset::from_ngx_uint(NGX_RS_HTTP_MAIN_CONF_OFFSET) };
}

/// `CommandCallRule` implementer for `Command` configuring Http Main Conf
pub struct HttpSrvConf<C>(PhantomData<C>);
impl<C> CommandCallRule for HttpSrvConf<C> {
    type Conf = C;
}

impl<M: HttpModule> CommandCallRuleBy<HttpModuleSkel<M>>
    for HttpSrvConf<<M::SrvConfSetting as MergeConfSetting>::Conf>
{
    const OFFSET: CommandOffset = unsafe { CommandOffset::from_ngx_uint(NGX_RS_HTTP_SRV_CONF_OFFSET) };
}

/// `CommandCallRule` implementer for `Command` configuring Http Main Conf
pub struct HttpLocConf<C>(PhantomData<C>);
impl<C> CommandCallRule for HttpLocConf<C> {
    type Conf = C;
}
impl<M: HttpModule> CommandCallRuleBy<HttpModuleSkel<M>>
    for HttpLocConf<<M::LocConfSetting as MergeConfSetting>::Conf>
{
    const OFFSET: CommandOffset = unsafe { CommandOffset::from_ngx_uint(NGX_RS_HTTP_LOC_CONF_OFFSET) };
}

/// Http configuration phase.
pub enum Phase {
    /// NGX_HTTP_POST_READ_PHASE.
    PostRead,
    /// NGX_HTTP_SERVER_REWRITE_PHASE.
    ServerRewrite,
    /// NGX_HTTP_FIND_CONFIG_PHASE.
    FindConfig,
    /// NGX_HTTP_REWRITE_PHASE.
    Rewrite,
    /// NGX_HTTP_POST_REWRITE_PHASE.
    PostRewrite,
    /// NGX_HTTP_PREACCESS_PHASE.
    PreAccess,
    /// NGX_HTTP_ACCESS_PHASE.
    Access,
    /// NGX_HTTP_POST_ACCESS_PHASE.
    PostAccess,
    /// NGX_HTTP_PRECONTENT_PHASE.
    PreContent,
    /// NGX_HTTP_CONTENT_PHASE.
    Content,
    /// NGX_HTTP_LOG_PHASE.
    Log,
}
impl Phase {
    const fn into_ngx_http_phases(self) -> ngx_http_phases {
        use Phase::*;
        match self {
            PostRead => ngx_http_phases_NGX_HTTP_POST_READ_PHASE,
            ServerRewrite => ngx_http_phases_NGX_HTTP_SERVER_REWRITE_PHASE,
            FindConfig => ngx_http_phases_NGX_HTTP_FIND_CONFIG_PHASE,
            Rewrite => ngx_http_phases_NGX_HTTP_REWRITE_PHASE,
            PostRewrite => ngx_http_phases_NGX_HTTP_POST_REWRITE_PHASE,
            PreAccess => ngx_http_phases_NGX_HTTP_PREACCESS_PHASE,
            Access => ngx_http_phases_NGX_HTTP_ACCESS_PHASE,
            PostAccess => ngx_http_phases_NGX_HTTP_POST_ACCESS_PHASE,
            PreContent => ngx_http_phases_NGX_HTTP_PRECONTENT_PHASE,
            Content => ngx_http_phases_NGX_HTTP_CONTENT_PHASE,
            Log => ngx_http_phases_NGX_HTTP_LOG_PHASE,
        }
    }
}
/// Interface to set Http handler.
/// This trait should be removed when ngx_conf_t created.
pub trait SetHttpHandler {
    /// set http handler.
    fn set_handler<H: HttpHandler>(&mut self) -> Result<(), Status>;
}
impl SetHttpHandler for ngx_conf_t {
    fn set_handler<H: HttpHandler>(&mut self) -> Result<(), Status> {
        let conf =
            unsafe { crate::http::ngx_http_conf_get_module_main_conf(self, &*addr_of!(ngx_http_core_module)).as_mut() }
                .ok_or(Status::NGX_ERROR)?;
        let pointer = unsafe {
            (ngx_array_push(&mut conf.phases[H::PHASE.into_ngx_http_phases() as usize].handlers)
                as *mut ngx_http_handler_pt)
                .as_mut()
        }
        .ok_or(Status::NGX_ERROR)?;
        *pointer = Some(handle_func::<H>);
        Ok(())
    }
}

/// Type safe interface expressing unique Nginx Http handler.
pub trait HttpHandler {
    /// The phase the handler registered in.
    const PHASE: Phase;
    /// Handler implementation.
    fn handle(request: &mut Request) -> Status;
}

unsafe extern "C" fn handle_func<H: HttpHandler>(request: *mut ngx_http_request_t) -> ngx_int_t {
    let req = Request::from_ngx_http_request(request);
    H::handle(req).into()
}
