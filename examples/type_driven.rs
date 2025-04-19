use std::ffi::CStr;

use ngx::ffi::{ngx_conf_t, ngx_str_t};
use ngx::http::HttpMainConf;
use ngx::module::{
    CommandArgFlag, CommandArgFlagSet, CommandCallRule, CommandContextFlag, CommandContextFlagSet, CommandError,
};
use ngx::{arg_flags, context_flags, exhibit_modules, ngx_string};
use ngx::{
    http::{DefaultInit, DefaultMerge, HttpModule, NgxHttpModule, NgxHttpModuleCommands},
    module::{Command, NgxModuleCommandsBuilder},
};

#[cfg(static_ref_mut)]
use ngx::{exhibit_modules, http::NgxHttpModuleCommandsRefMut, util::StaticRefMut};
#[cfg(not(static_ref_mut))]
use std::ptr::addr_of_mut;

#[cfg(not(static_ref_mut))]
use ngx::{
    http::HttpModuleSkel,
    module::{NgxModule, NgxModuleCtx},
    ngx_modules,
};
#[cfg(not(static_ref_mut))]
use std::ptr::addr_of;

#[cfg(all(static_ref_mut, feature = "export-modules"))]
exhibit_modules!(HttpModuleSkel<FooBarHttpModule>);

#[cfg(all(not(static_ref_mut), feature = "export-modules"))]
exhibit_modules!(HttpModuleSkel<FooBarHttpModule> => &mut FOO_BAR_HTTP_MODULE);

#[cfg(not(static_ref_mut))]
static mut FOO_BAR_HTTP_MODULE: NgxModule<HttpModuleSkel<FooBarHttpModule>> = unsafe {
    NgxModule::new_from_ptr(
        addr_of!(FOO_BAR_HTTP_MODULE_CTX),
        addr_of!(FOO_BAR_HTTP_MODULE_COMMANDS),
    )
};
#[cfg(not(static_ref_mut))]
static mut FOO_BAR_HTTP_MODULE_CTX: NgxModuleCtx<HttpModuleSkel<FooBarHttpModule>> =
    unsafe { NgxModuleCtx::from_raw() };

#[cfg(not(static_ref_mut))]
static mut FOO_BAR_HTTP_MODULE_COMMANDS: NgxHttpModuleCommands<FooBarHttpModule, { 1 + 1 }> =
    NgxModuleCommandsBuilder::new().add::<FooBarCommand>().build();

struct FooBarHttpModule;
impl HttpModule for FooBarHttpModule {
    #[cfg(static_ref_mut)]
    const SELF: StaticRefMut<NgxHttpModule<Self>> = {
        static mut FOO_BAR_HTTP_MODULE: NgxHttpModule<FooBarHttpModule> = NgxHttpModule::new();
        unsafe { StaticRefMut::from_mut(&mut *addr_of_mut!(FOO_BAR_HTTP_MODULE)) }
    };

    const NAME: &'static CStr = c"foo_bar_module";

    #[cfg(static_ref_mut)]
    const COMMANDS: NgxHttpModuleCommandsRefMut<Self> = {
        static mut FOO_BAR_HTTP_MODULE_COMMANDS: NgxHttpModuleCommands<FooBarHttpModule, { 1 + 1 }> =
            NgxModuleCommandsBuilder::new().add::<FooBarCommand>().build();
        unsafe { NgxHttpModuleCommandsRefMut::from_mut(&mut *addr_of_mut!(FOO_BAR_HTTP_MODULE_COMMANDS)) }
    };

    type MasterInitializer = ();
    type ModuleDelegate = ();
    type ProcessDelegate = ();
    type ThreadDelegate = ();

    type MainConfSetting = DefaultInit<u32>;

    type SrvConfSetting = DefaultMerge<()>;

    type LocConfSetting = DefaultMerge<()>;

    type Ctx = ();

    type PreConfiguration = ();
    type PostConfiguration = ();
}

struct FooBarCommand;
impl Command for FooBarCommand {
    type CallRule = HttpMainConf<u32>;

    const NAME: ngx_str_t = ngx_string!("foo_bar");

    const CONTEXT_FLAG: CommandContextFlagSet = context_flags!(
        CommandContextFlag::HttpMain,
        CommandContextFlag::HttpSrv,
        CommandContextFlag::HttpLoc
    );

    const ARG_FLAG: CommandArgFlagSet = arg_flags!(CommandArgFlag::Take1, CommandArgFlag::Take2);

    fn handler(_cf: &mut ngx_conf_t, conf: &mut <Self::CallRule as CommandCallRule>::Conf) -> Result<(), CommandError> {
        *conf += 1;
        Ok(())
    }
}
