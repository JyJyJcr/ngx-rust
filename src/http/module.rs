use std::ffi::c_char;
use std::ffi::c_void;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::ptr;

use crate::core::NGX_CONF_ERROR;
use crate::core::*;
use crate::ffi::*;
use crate::module::CycleDelegate;
use crate::module::PreCycleDelegate;
use crate::util::StaticRefMut;

use super::ConfigurationDelegate;
use super::HttpModule;
use super::InitConfSetting;
use super::MergeConfSetting;
use super::NgxHttpModule;
use super::NgxHttpModuleCommandsRefMut;

/// MergeConfigError - configuration cannot be merged with levels above.
#[derive(Debug)]
pub enum MergeConfigError {
    /// No value provided for configuration argument
    NoValue,
}

impl std::error::Error for MergeConfigError {}

impl std::fmt::Display for MergeConfigError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MergeConfigError::NoValue => "no value".fmt(fmt),
        }
    }
}

/// The `Merge` trait provides a method for merging configuration down through each level.
///
/// A module configuration should implement this trait for setting its configuration throughout
/// each level.
pub trait Merge {
    /// Module merge function.
    ///
    /// # Returns
    /// Result, Ok on success or MergeConfigError on failure.
    fn merge(&mut self, prev: &Self) -> Result<(), MergeConfigError>;
}

impl Merge for () {
    fn merge(&mut self, _prev: &Self) -> Result<(), MergeConfigError> {
        Ok(())
    }
}

/// The `HTTPModule` trait provides the NGINX configuration stage interface.
///
/// These functions allocate structures, initialize them, and merge through the configuration
/// layers.
///
/// See <https://nginx.org/en/docs/dev/development_guide.html#adding_new_modules> for details.
pub trait HTTPModule {
    /// Configuration in the `http` block.
    type MainConf: Merge + Default;
    /// Configuration in a `server` block within the `http` block.
    type SrvConf: Merge + Default;
    /// Configuration in a `location` block within the `http` block.
    type LocConf: Merge + Default;

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn preconfiguration(_cf: *mut ngx_conf_t) -> ngx_int_t {
        Status::NGX_OK.into()
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn postconfiguration(_cf: *mut ngx_conf_t) -> ngx_int_t {
        Status::NGX_OK.into()
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn create_main_conf(cf: *mut ngx_conf_t) -> *mut c_void {
        let mut pool = Pool::from_ngx_pool((*cf).pool);
        pool.allocate::<Self::MainConf>(Default::default()) as *mut c_void
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn init_main_conf(_cf: *mut ngx_conf_t, _conf: *mut c_void) -> *mut c_char {
        ptr::null_mut()
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn create_srv_conf(cf: *mut ngx_conf_t) -> *mut c_void {
        let mut pool = Pool::from_ngx_pool((*cf).pool);
        pool.allocate::<Self::SrvConf>(Default::default()) as *mut c_void
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn merge_srv_conf(_cf: *mut ngx_conf_t, prev: *mut c_void, conf: *mut c_void) -> *mut c_char {
        let prev = &mut *(prev as *mut Self::SrvConf);
        let conf = &mut *(conf as *mut Self::SrvConf);
        match conf.merge(prev) {
            Ok(_) => ptr::null_mut(),
            Err(_) => NGX_CONF_ERROR as _,
        }
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn create_loc_conf(cf: *mut ngx_conf_t) -> *mut c_void {
        let mut pool = Pool::from_ngx_pool((*cf).pool);
        pool.allocate::<Self::LocConf>(Default::default()) as *mut c_void
    }

    /// # Safety
    ///
    /// Callers should provide valid non-null `ngx_conf_t` arguments. Implementers must
    /// guard against null inputs or risk runtime errors.
    unsafe extern "C" fn merge_loc_conf(_cf: *mut ngx_conf_t, prev: *mut c_void, conf: *mut c_void) -> *mut c_char {
        let prev = &mut *(prev as *mut Self::LocConf);
        let conf = &mut *(conf as *mut Self::LocConf);
        match conf.merge(prev) {
            Ok(_) => ptr::null_mut(),
            Err(_) => NGX_CONF_ERROR as _,
        }
    }
}

pub trait HTTPModuleSupplement<M: HTTPModule + 'static>: 'static + Sized {
    const SELF: StaticRefMut<NgxHttpModule<(M, Self)>>;
    const NAME: &'static CStr;
    const COMMANDS: NgxHttpModuleCommandsRefMut<(M, Self)>;

    /// exexutor type deligating `init_master` (not called now).
    type MasterInitializer: PreCycleDelegate;
    /// exexutor type deligating `init_module` and `exit_master`.
    type ModuleDelegate: CycleDelegate;
    /// exexutor type deligating `init_process` and `exit_process`.
    type ProcessDelegate: CycleDelegate;
    /// exexutor type deligating `init_thread` and `exit_thread` (not called now).
    type ThreadDelegate: CycleDelegate;

    type Ctx;
}

impl<M: HTTPModule + 'static, MS: HTTPModuleSupplement<M>> HttpModule for (M, MS) {
    const SELF: StaticRefMut<NgxHttpModule<(M, MS)>> = MS::SELF;
    const NAME: &'static CStr = MS::NAME;
    const COMMANDS: NgxHttpModuleCommandsRefMut<Self> = MS::COMMANDS;

    type MasterInitializer = MS::MasterInitializer;
    type ModuleDelegate = MS::ModuleDelegate;
    type ProcessDelegate = MS::ProcessDelegate;
    type ThreadDelegate = MS::ThreadDelegate;

    type PreConfiguration = HTTPModulePreConfiguration<M>;
    type PostConfiguration = HTTPModulePostConfiguration<M>;
    type MainConfSetting = HTTPModuleMainConfSetting<M>;
    type SrvConfSetting = HTTPModuleSrvConfSetting<M>;
    type LocConfSetting = HTTPModuleLocConfSetting<M>;

    type Ctx = MS::Ctx;
}

pub struct HTTPModulePreConfiguration<M: HTTPModule>(PhantomData<M>);
impl<M: HTTPModule> ConfigurationDelegate for HTTPModulePreConfiguration<M> {
    fn configuration(_cf: &mut ngx_conf_t) -> Result<(), Status> {
        unimplemented!()
    }
    unsafe extern "C" fn configuration_unsafe(cf: *mut ngx_conf_t) -> ngx_int_t {
        M::preconfiguration(cf)
    }
}
pub struct HTTPModulePostConfiguration<M: HTTPModule>(PhantomData<M>);
impl<M: HTTPModule> ConfigurationDelegate for HTTPModulePostConfiguration<M> {
    fn configuration(_cf: &mut ngx_conf_t) -> Result<(), Status> {
        unimplemented!()
    }
    unsafe extern "C" fn configuration_unsafe(cf: *mut ngx_conf_t) -> ngx_int_t {
        M::postconfiguration(cf)
    }
}

pub struct HTTPModuleMainConfSetting<M: HTTPModule>(PhantomData<M>);
impl<M: HTTPModule> InitConfSetting for HTTPModuleMainConfSetting<M> {
    type Conf = M::MainConf;

    fn create(cf: &mut ngx_conf_t) -> Result<Self::Conf, ()> {
        unimplemented!()
    }

    fn init(cf: &mut ngx_conf_t, conf: &mut Self::Conf) -> Result<(), ()> {
        unimplemented!()
    }
    unsafe extern "C" fn create_unsafe(cf: *mut ngx_conf_t) -> *mut c_void {
        M::create_main_conf(cf)
    }
    unsafe extern "C" fn init_unsafe(cf: *mut ngx_conf_t, conf: *mut c_void) -> *mut c_char {
        M::init_main_conf(cf, conf)
    }
}

pub struct HTTPModuleSrvConfSetting<M: HTTPModule>(PhantomData<M>);
impl<M: HTTPModule> MergeConfSetting for HTTPModuleSrvConfSetting<M> {
    type Conf = M::SrvConf;

    fn create(cf: &mut ngx_conf_t) -> Result<Self::Conf, ()> {
        unimplemented!()
    }
    fn merge(cf: &mut ngx_conf_t, prev: &mut Self::Conf, conf: &mut Self::Conf) -> Result<(), ()> {
        unimplemented!()
    }
    unsafe extern "C" fn create_unsafe(cf: *mut ngx_conf_t) -> *mut c_void {
        M::create_srv_conf(cf)
    }
    unsafe extern "C" fn merge_unsafe(cf: *mut ngx_conf_t, prev: *mut c_void, conf: *mut c_void) -> *mut c_char {
        M::merge_srv_conf(cf, prev, conf)
    }
}

pub struct HTTPModuleLocConfSetting<M: HTTPModule>(PhantomData<M>);
impl<M: HTTPModule> MergeConfSetting for HTTPModuleLocConfSetting<M> {
    type Conf = M::LocConf;

    fn create(cf: &mut ngx_conf_t) -> Result<Self::Conf, ()> {
        unimplemented!()
    }
    fn merge(cf: &mut ngx_conf_t, prev: &mut Self::Conf, conf: &mut Self::Conf) -> Result<(), ()> {
        unimplemented!()
    }
    unsafe extern "C" fn create_unsafe(cf: *mut ngx_conf_t) -> *mut c_void {
        M::create_loc_conf(cf)
    }
    unsafe extern "C" fn merge_unsafe(cf: *mut ngx_conf_t, prev: *mut c_void, conf: *mut c_void) -> *mut c_char {
        M::merge_loc_conf(cf, prev, conf)
    }
}
