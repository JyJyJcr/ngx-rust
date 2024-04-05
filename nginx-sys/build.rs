extern crate bindgen;
extern crate duct;

use std::collections::HashMap;
use std::error::Error as StdError;
use std::ffi::OsString;
use std::fs::{read_to_string, File, Permissions};
use std::io::ErrorKind::NotFound;
use std::io::{Error as IoError, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::{env, fs, thread};

use duct::cmd;
use flate2::read::GzDecoder;
use tar::Archive;
use url::Url;
use which::which;

const UBUNTU_KEYSERVER: &str = "hkps://keyserver.ubuntu.com";
/// The default version of zlib to use if the `ZLIB_VERSION` environment variable is not present
const ZLIB_DEFAULT_VERSION: &str = "1.3.1";
/// Key 1: Mark Adler's public key. For zlib 1.3.1 and earlier
const ZLIB_GPG_SERVER_AND_KEY_ID: (&str, &str) = (UBUNTU_KEYSERVER, "5ED46A6721D365587791E2AA783FCD8E58BCAFBA");
const ZLIB_DOWNLOAD_URL_PREFIX: &str = "https://github.com/madler/zlib/releases/download";
/// The default version of pcre to use if the `PCRE2_VERSION` environment variable is not present
const PCRE1_DEFAULT_VERSION: &str = "8.45";
const PCRE2_DEFAULT_VERSION: &str = "10.42";
/// Key 1: Phillip Hazel's public key. For PCRE2 10.42 and earlier
const PCRE2_GPG_SERVER_AND_KEY_ID: (&str, &str) = (UBUNTU_KEYSERVER, "45F68D54BBE23FB3039B46E59766E084FB0F43D8");
const PCRE1_DOWNLOAD_URL_PREFIX: &str = "https://sourceforge.net/projects/pcre/files/pcre";
const PCRE2_DOWNLOAD_URL_PREFIX: &str = "https://github.com/PCRE2Project/pcre2/releases/download";
/// The default version of openssl to use if the `OPENSSL_VERSION` environment variable is not present
const OPENSSL1_DEFAULT_VERSION: &str = "1.1.1w";
const OPENSSL3_DEFAULT_VERSION: &str = "3.2.1";
const OPENSSL_GPG_SERVER_AND_KEY_IDS: (&str, &str) = (
    UBUNTU_KEYSERVER,
    "\
EFC0A467D613CB83C7ED6D30D894E2CE8B3D79F5 \
A21FAB74B0088AA361152586B8EF1A6BA9DA2D5C \
8657ABB260F056B1E5190839D9C4D26D0E604491 \
B7C1C14360F353A36862E4D5231C84CDDCC69C45 \
95A9908DDFA16830BE9FB9003D30A3A9FF1360DC \
7953AC1FBC3DC8B3B292393ED5E9E43F7DF9EE8C \
E5E52560DD91C556DDBDA5D02064C53641C25E5D \
C1F33DD8CE1D4CC613AF14DA9195C48241FBF7DD",
);
const OPENSSL_DOWNLOAD_URL_PREFIX: &str = "https://github.com/openssl/openssl/releases/download";
/// The default version of NGINX to use if the `NGX_VERSION` environment variable is not present
const NGX_DEFAULT_VERSION: &str = "1.24.0";

/// Key 1: Konstantin Pavlov's public key. For Nginx 1.25.3 and earlier
/// Key 2: Sergey Kandaurov's public key. For Nginx 1.25.4
/// Key 3: Maxim Dounin's public key. At least used for Nginx 1.18.0
const NGX_GPG_SERVER_AND_KEY_IDS: (&str, &str) = (
    UBUNTU_KEYSERVER,
    "\
13C82A63B603576156E30A4EA0EA981B66B0D967 \
D6786CE303D9A9022998DC6CC8464D549AF75C0A \
B0F4253373F8F6F510D42178520A9993A1C052F8",
);

const NGX_DOWNLOAD_URL_PREFIX: &str = "https://nginx.org/download";

/// If you are adding another dependency, you will need to add the server/public key tuple below.
const ALL_SERVERS_AND_PUBLIC_KEY_IDS: [(&str, &str); 4] = [
    ZLIB_GPG_SERVER_AND_KEY_ID,
    PCRE2_GPG_SERVER_AND_KEY_ID,
    OPENSSL_GPG_SERVER_AND_KEY_IDS,
    NGX_GPG_SERVER_AND_KEY_IDS,
];

/// List of configure switches specifying the modules to build nginx with
const NGX_BASE_MODULES: [&str; 20] = [
    "--with-compat",
    "--with-http_addition_module",
    "--with-http_auth_request_module",
    "--with-http_flv_module",
    "--with-http_gunzip_module",
    "--with-http_gzip_static_module",
    "--with-http_random_index_module",
    "--with-http_realip_module",
    "--with-http_secure_link_module",
    "--with-http_slice_module",
    "--with-http_slice_module",
    "--with-http_ssl_module",
    "--with-http_stub_status_module",
    "--with-http_sub_module",
    "--with-http_v2_module",
    "--with-stream_realip_module",
    "--with-stream_ssl_module",
    "--with-stream_ssl_preread_module",
    "--with-stream",
    "--with-threads",
];
/// Additional configuration flags to use when building on Linux.
const NGX_LINUX_ADDITIONAL_OPTS: [&str; 3] = [
    "--with-file-aio",
    "--with-cc-opt=-g -fstack-protector-strong -Wformat -Werror=format-security -Wp,-D_FORTIFY_SOURCE=2 -fPIC",
    "--with-ld-opt=-Wl,-Bsymbolic-functions -Wl,-z,relro -Wl,-z,now -Wl,--as-needed -pie",
];
const ENV_VARS_TRIGGERING_RECOMPILE: [&str; 12] = [
    "NGX_DEBUG",
    "OUT_DIR",
    "TARGET",
    "ZLIB_VERSION",
    "PCRE2_VERSION",
    "OPENSSL_VERSION",
    "NGX_VERSION",
    //"CARGO_CFG_TARGET_OS",
    "CARGO_MANIFEST_DIR",
    "CARGO_TARGET_TMPDIR",
    "CACHE_DIR",
    "NGX_INSTALL_ROOT_DIR",
    "NGX_INSTALL_DIR",
];

struct BuildConfig {
    out_dir: PathBuf,
    cache_dir: PathBuf,
    ngx_install_dir: PathBuf,
    src_root_dir: PathBuf,
    ngx_version: String,
    zlib_version: String,
    pcre_version: String,
    openssl_version: String,
    target: String,
    os: String,
    ngx_debug: bool,
}

impl BuildConfig {
    fn from_env() -> Self {
        // ENVs cargo provides (MUST): cargo info

        fn var_or_fail(name: &str) -> String {
            env::var(name).expect(format!("The required environment variable {} is not set", name).as_str())
        }
        let out_dir = PathBuf::from(var_or_fail("OUT_DIR"));
        let target = var_or_fail("TARGET");
        let os = var_or_fail("CARGO_CFG_TARGET_OS");

        // ENVs user provides (SHOULD): version specification

        let ngx_version = env::var("NGX_VERSION").unwrap_or(NGX_DEFAULT_VERSION.into());
        let zlib_version = env::var("ZLIB_VERSION").unwrap_or(ZLIB_DEFAULT_VERSION.into());
        // While Nginx 1.22.0 and later support pcre2 and openssl3, earlier ones only support pcre1 and openssl1. Here provides the appropriate (and as latest as possible) versions of these two dependencies as default, switching `***[major_version]_DEFAULT_VERSION` based on `is_after_1_22`. This facilitates to compile backport versions targeted for Nginx ealier than 1.22.0, which are still used in LTS releases of major Linux distributions.
        let ngx_version_vec: Vec<i16> = ngx_version.split('.').map(|s| s.parse().unwrap_or(-1)).collect();
        let ngx_after_1_22_0 = (ngx_version_vec.len() >= 2)
            && (ngx_version_vec[0] > 1 || (ngx_version_vec[0] == 1 && ngx_version_vec[1] >= 22));
        // leave env name `PCRE2_VERSION` for compat
        let pcre_version = env::var("PCRE_VERSION").or(env::var("PCRE2_VERSION")).unwrap_or(
            if ngx_after_1_22_0 {
                PCRE2_DEFAULT_VERSION
            } else {
                PCRE1_DEFAULT_VERSION
            }
            .into(),
        );
        let openssl_version = env::var("OPENSSL_VERSION").unwrap_or(
            if ngx_after_1_22_0 {
                OPENSSL3_DEFAULT_VERSION
            } else {
                OPENSSL1_DEFAULT_VERSION
            }
            .into(),
        );

        // ENVs user provides (MAY): directory path

        // prepare cache dir
        // Choose `.cache` relative to the manifest directory (nginx-sys) as the default cache directory
        // Environment variable `CACHE_DIR` overrides this
        // Recommendation: set env "CACHE_DIR = { value = ".cache", relative = true }" in `.cargo/config.toml` in your project
        let cache_dir = env::var("CACHE_DIR").map_or(
            env::var("CARGO_MANIFEST_DIR")
                .map(PathBuf::from)
                .unwrap_or(env::current_dir().expect("Failed to get current directory"))
                .join(".cache"),
            PathBuf::from,
        );
        // the below is ideal, but in this step we just refactor the code, so don't change this.
        //let cache_dir = env::var("CACHE_DIR").map_or(out_dir.join(".cache"), PathBuf::from);
        make_dir_or_fail(&cache_dir);

        // prepare nginx install dir
        let ngx_install_root_dir = env::var("NGX_INSTALL_ROOT_DIR").map_or(cache_dir.join("nginx"), PathBuf::from);
        let ngx_install_dir =
            env::var("NGX_INSTALL_DIR").map_or(ngx_install_root_dir.join(&ngx_version).join(&target), PathBuf::from);
        make_dir_or_fail(&ngx_install_dir);

        // prepare source root dir
        let src_root_dir = env::var("CARGO_TARGET_TMPDIR").map_or(cache_dir.join("src").join(&target), PathBuf::from);
        make_dir_or_fail(&src_root_dir);

        // ENVs user provides (MAY): debug mode

        let ngx_debug = env::var("NGX_DEBUG").map_or(false, |s| s == "true");

        Self {
            out_dir: out_dir,
            cache_dir: cache_dir,
            ngx_install_dir: ngx_install_dir,
            src_root_dir: src_root_dir,
            ngx_version: ngx_version,
            zlib_version: zlib_version,
            pcre_version: pcre_version,
            openssl_version: openssl_version,
            target: target,
            os: os,
            ngx_debug: ngx_debug,
        }
    }
    fn hint(&self) {
        // Hint cargo to rebuild if any of the these environment variables values change
        // because they will trigger a recompilation of NGINX with different parameters
        for var in ENV_VARS_TRIGGERING_RECOMPILE {
            println!("cargo::rerun-if-env-changed={var}");
        }
        println!("cargo::rerun-if-changed=build.rs");
        println!("cargo::rerun-if-changed=wrapper.h");
        println!("cargo::rustc-env=NGINX_SYS_TARGET={}", self.target);
        println!(
            "cargo::rustc-env=NGINX_SYS_NGX_INSTALL_DIR={}",
            self.ngx_install_dir.display()
        );
    }
}

struct GPGManager {
    bin: Option<PathBuf>,
    dir: PathBuf,
}
impl GPGManager {
    fn new(dir: PathBuf) -> Self {
        // prepare gnupg dir
        make_dir_or_fail(&dir);

        Self {
            bin: which::which("gpg").ok(),
            dir: dir,
        }
    }
    /// Imports all the required GPG keys into the `[cache dir]/.gnupg` directory in order to
    /// validate the integrity of the downloaded tarballs.
    fn import_gpg_keys(&self) -> Result<(), Box<dyn StdError>> {
        if let Some(bin) = &self.bin {
            // We do not want to mess with the default gpg data for the running user,
            // so we store all gpg data with our cache directory.

            self.ensure_dir_permissions()?;
            for (server, key_ids) in keys_indexed_by_key_server() {
                println!("Importing {} GPG keys for key server: {}", key_ids.len(), server);
                let dir_str = self.dir.to_string_lossy().to_string();
                let base_args = vec![
                    "--homedir",
                    dir_str.as_str(),
                    "--keyserver",
                    server.as_str(),
                    "--recv-keys",
                ];
                let key_ids_str = key_ids.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
                let args = [base_args, key_ids_str].concat();
                let cmd = duct::cmd(bin, &args);

                let output = cmd.stderr_to_stdout().stderr_capture().unchecked().run()?;

                if !output.status.success() {
                    eprintln!("{}", String::from_utf8_lossy(&output.stdout));
                    return Err(format!(
                        "Command: {:?}\n\
                    Failed to import GPG keys: {}",
                        cmd,
                        key_ids.join(" ")
                    )
                    .into());
                }
            }
            self.ensure_dir_permissions()?;
        }
        Ok(())
    }
    /// Ensure the correct permissions are applied to the local gnupg directory
    fn ensure_dir_permissions(&self) -> Result<(), Box<dyn StdError>> {
        fn change_permissions_recursively(path: &Path, dir_mode: u32, file_mode: u32) -> std::io::Result<()> {
            if path.is_dir() {
                // Set directory permissions to 700
                fs::set_permissions(path, Permissions::from_mode(dir_mode))?;

                for entry in fs::read_dir(path)? {
                    let entry = entry?;
                    let path = entry.path();

                    change_permissions_recursively(&path, dir_mode, file_mode)?;
                }
            } else {
                // Set file permissions to 600
                fs::set_permissions(path, Permissions::from_mode(file_mode))?;
            }

            Ok(())
        }
        change_permissions_recursively(&self.dir, 0o700, 0o600).map_err(|e| Box::new(e) as Box<dyn StdError>)
    }
    /// Validates that a file is a valid GPG signature file.
    fn verify_signature_file(&self, signature_path: &Path) -> Result<(), Box<dyn StdError>> {
        if !signature_path.exists() {
            return Err(Box::new(std::io::Error::new(
                NotFound,
                format!("GPG signature file not found: {}", signature_path.display()),
            )));
        }
        if let Some(bin) = &self.bin {
            let cmd = cmd!(bin, "--homedir", &self.dir, "--list-packets", signature_path);
            let output = cmd.stderr_to_stdout().stdout_capture().unchecked().run()?;

            if !output.status.success() {
                eprintln!("{}", String::from_utf8_lossy(&output.stdout));
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!(
                        "Command: {:?} \nGPG signature file verification failed for signature: {}",
                        cmd,
                        signature_path.display()
                    ),
                )));
            }
        } else {
            println!("GPG not found, skipping signature file verification");
        }
        Ok(())
    }

    /// Validates the integrity of a tarball file against the cryptographic signature associated with
    /// the file.
    fn verify_archive_signature(&self, archive_path: &Path, signature_path: &Path) -> Result<(), Box<dyn StdError>> {
        if let Some(bin) = &self.bin {
            let cmd = cmd!(bin, "--homedir", &self.dir, "--verify", signature_path, archive_path);
            let output = cmd.stderr_to_stdout().stdout_capture().unchecked().run()?;
            if !output.status.success() {
                eprintln!("{}", String::from_utf8_lossy(&output.stdout));
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!(
                        "Command: {:?}\nGPG signature verification failed of archive failed [{}]",
                        cmd,
                        archive_path.display()
                    ),
                )));
            }
        } else {
            println!("GPG not found, skipping signature verification");
        }
        Ok(())
    }
}

/// Downloads a tarball from the specified URL into the `.cache` directory.
fn download(url: &Url, file_path: &Path) -> Result<(), Box<dyn StdError>> {
    if !file_path.exists() || file_path.metadata().map_or(false, |m| m.len() < 1) {
        println!("Downloading: {} -> {}", url, file_path.display());
        let mut reader = ureq::get(url.as_str()).call()?.into_reader();
        let mut file = File::create(&file_path)?;
        std::io::copy(&mut reader, &mut file)?;
    }
    if !file_path.exists() {
        return Err(format!("Downloaded file was not written to the expected location: {}", url).into());
    }
    Ok(())
}

struct ArchiveLine {
    tar_url: Url,
    sig_url: Url,
    tar_path: PathBuf,
    sig_path: PathBuf,
    src_dir: PathBuf,
}
impl ArchiveLine {
    fn new(tar_url_str: &str, sig_url_str: &str, tar_path: PathBuf, sig_path: PathBuf, src_dir: PathBuf) -> Self {
        let tar_url = Url::parse(tar_url_str).expect(format!("Failed to parse URL {}", tar_url_str).as_str());
        let sig_url = Url::parse(sig_url_str).expect(format!("Failed to parse URL {}", sig_url_str).as_str());
        Self {
            tar_url: tar_url,
            sig_url: sig_url,
            tar_path: tar_path,
            sig_path: sig_path,
            src_dir: src_dir,
        }
    }
    fn new_with_cache_dir(tar_url_str: &str, sig_url_str: &str, cache_dir: &Path, src_dir: PathBuf) -> Self {
        Self::new(
            tar_url_str,
            sig_url_str,
            cache_dir.join(tar_url_str.split('/').last().unwrap()),
            cache_dir.join(sig_url_str.split('/').last().unwrap()),
            src_dir,
        )
    }
    fn new_with_cache_dir_and_sig_ext(tar_url_str: &str, ext: &str, cache_dir: &Path, src_dir: PathBuf) -> Self {
        Self::new_with_cache_dir(
            tar_url_str,
            format!("{}.{}", tar_url_str, ext).as_str(),
            cache_dir,
            src_dir,
        )
    }
    fn download(&self) -> Result<(), Box<dyn StdError>> {
        download(&self.tar_url, &self.tar_path)?;
        download(&self.sig_url, &self.sig_path)?;
        Ok(())
    }
    fn validate(&self, gpgm: &GPGManager) -> Result<(), Box<dyn StdError>> {
        if let Err(e) = gpgm.verify_signature_file(&self.sig_path) {
            fs::remove_file(&self.sig_path)?;
            return Err(e);
        }
        match gpgm.verify_archive_signature(&self.tar_path, &self.sig_path) {
            Ok(_) => Ok(()),
            Err(e) => {
                fs::remove_file(&self.tar_path)?;
                Err(e)
            }
        }
    }
    /// Extract a tarball into a subdirectory based on the tarball's name under the source base directory.
    fn extract(&self) -> Result<(), Box<dyn StdError>> {
        let archive_file = File::open(&self.tar_path)
            .expect(format!("Unable to open archive file: {}", self.tar_path.display()).as_str());
        if !self.src_dir.exists() {
            Archive::new(GzDecoder::new(archive_file))
                .entries()?
                .filter_map(|e| e.ok())
                .for_each(|mut entry| {
                    let path = entry.path().unwrap();
                    let stripped_path = path.components().skip(1).collect::<PathBuf>();
                    entry.unpack(&self.src_dir.join(stripped_path)).unwrap();
                });
        } else {
            println!(
                "Archive {} already extracted to directory: {}",
                self.tar_path.display(),
                self.src_dir.display()
            );
        }

        Ok(())
    }
}

struct Downloader {
    ngx: ArchiveLine,
    openssl: ArchiveLine,
    pcre: ArchiveLine,
    zlib: ArchiveLine,
}

impl Downloader {
    /// Returns a list of tuples containing the URL to a tarball archive and the GPG signature used to validate the integrity of the tarball.
    fn from_conf(conf: &BuildConfig) -> Self {
        let unique_src_dir =
            |name: &str, version: &str| -> PathBuf { conf.src_root_dir.join(format!("{}-{}", name, version)) };

        Self {
            ngx: ArchiveLine::new_with_cache_dir_and_sig_ext(
                &ngx_archive_url(&conf.ngx_version),
                "asc",
                &conf.cache_dir,
                unique_src_dir("nginx", &conf.ngx_version),
            ),
            openssl: ArchiveLine::new_with_cache_dir_and_sig_ext(
                &openssl_archive_url(&conf.openssl_version),
                "asc",
                &conf.cache_dir,
                unique_src_dir("openssl", &conf.openssl_version),
            ),
            pcre: ArchiveLine::new_with_cache_dir_and_sig_ext(
                &pcre_archive_url(&conf.pcre_version),
                "sig",
                &conf.cache_dir,
                unique_src_dir("pcre", &conf.pcre_version),
            ),
            zlib: ArchiveLine::new_with_cache_dir_and_sig_ext(
                &zlib_archive_url(&conf.zlib_version),
                "asc",
                &conf.cache_dir,
                unique_src_dir("zlib", &conf.zlib_version),
            ),
        }
    }
    fn load(&self, gpgm: &GPGManager) -> Result<(), Box<dyn StdError>> {
        let dwn_and_val = |al: &ArchiveLine| -> Result<(), Box<dyn StdError>> {
            al.download()?;
            al.validate(gpgm)?;
            al.extract()?;
            Ok(())
        };
        dwn_and_val(&self.ngx)?;
        dwn_and_val(&self.openssl)?;
        dwn_and_val(&self.pcre)?;
        dwn_and_val(&self.zlib)?;
        Ok(())
    }
}

/// Function invoked when `cargo build` is executed.
/// This function will download NGINX and all supporting dependencies, verify their integrity,
/// extract them, execute autoconf `configure` for NGINX, compile NGINX and finally install
/// NGINX in a subdirectory with the project.
fn main() -> Result<(), Box<dyn StdError>> {
    println!("Building NGINX");

    // Create BuildConfig
    let conf = BuildConfig::from_env();
    println!("Cache directory created");
    // Create GPGManager
    let gpgm = GPGManager::new(conf.cache_dir.join(".gnupg"));
    // Import GPG keys used to verify dependency tarballs, if gpg is available
    gpgm.import_gpg_keys()?;
    println!("GPG keys imported");
    // Create Downloader
    let dwner = Downloader::from_conf(&conf);
    // download, validate and extract tarballs
    dwner.load(&gpgm)?;

    // Configure and Compile NGINX
    let ngx_src_dir = compile_nginx(&conf, &dwner)?;
    conf.hint();
    // Read autoconf generated makefile for NGINX and generate Rust bindings based on its includes
    generate_binding(&conf.out_dir, &ngx_src_dir);
    Ok(())
}

/// Generates Rust bindings for NGINX
fn generate_binding(out_dir: &Path, nginx_source_dir: &Path) {
    let autoconf_makefile_path = nginx_source_dir.join("objs").join("Makefile");
    let clang_args: Vec<String> = parse_includes_from_makefile(&autoconf_makefile_path)
        .into_iter()
        .map(|path| format!("-I{}", path.to_string_lossy()))
        .collect();

    let bindings = bindgen::Builder::default()
        // Bindings will not compile on Linux without block listing this item
        // It is worth investigating why this is
        .blocklist_item("IPPORT_RESERVED")
        // The input header we would like to generate bindings for.
        .header("wrapper.h")
        .clang_args(clang_args)
        .layout_tests(false)
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}

/*
###########################################################################
# NGINX Build Functions - Everything below here is for building NGINX     #
###########################################################################

In order to build Rust bindings for NGINX using the bindgen crate, we need
to do the following:

 1. Download NGINX source code and all dependencies (zlib, pcre2, openssl)
 2. Verify the integrity of the downloaded files using GPG signatures
 3. Extract the downloaded files
 4. Run autoconf `configure` for NGINX
 5. Compile NGINX
 6. Install NGINX in a subdirectory of the project
 7. Read the autoconf generated makefile for NGINX and configure bindgen
    to generate Rust bindings based on the includes in the makefile.

Additionally, we want to provide the following features as part of the
build process:
 * Allow the user to specify the version of NGINX to build
 * Allow the user to specify the version of each dependency to build
 * Only reconfigure and recompile NGINX if any of the above versions
   change or the configuration flags change (like enabling or disabling
   the debug mode)
 * Not rely on the user having NGINX dependencies installed on their
   system (zlib, pcre2, openssl)
 * Keep source code and binaries confined to a subdirectory of the
   project to avoid having to track files outside of the project
 * If GPG is not installed, the build will still continue. However, the
   integrity of the downloaded files will not be verified.
*/

fn zlib_archive_url(version: &str) -> String {
    format!("{ZLIB_DOWNLOAD_URL_PREFIX}/v{version}/zlib-{version}.tar.gz")
}

fn pcre_archive_url(version: &str) -> String {
    // We can distinguish pcre1/pcre2 by checking whether the second character is '.', because the final version of pcre1 is 8.45 and the first one of pcre2 is 10.00.
    if version.chars().nth(1).is_some_and(|c| c == '.') {
        format!("{PCRE1_DOWNLOAD_URL_PREFIX}/{version}/pcre-{version}.tar.gz")
    } else {
        format!("{PCRE2_DOWNLOAD_URL_PREFIX}/pcre2-{version}/pcre2-{version}.tar.gz")
    }
}

fn openssl_archive_url(version: &str) -> String {
    if version.starts_with("1.1.1") {
        let version_hyphened = version.replace('.', "_");
        format!("{OPENSSL_DOWNLOAD_URL_PREFIX}/OpenSSL_{version_hyphened}/openssl-{version}.tar.gz")
    } else {
        format!("{OPENSSL_DOWNLOAD_URL_PREFIX}/openssl-{version}/openssl-{version}.tar.gz")
    }
}

fn ngx_archive_url(version: &str) -> String {
    format!("{NGX_DOWNLOAD_URL_PREFIX}/nginx-{version}.tar.gz")
}

/// Iterates through the tuples in `ALL_SERVERS_AND_PUBLIC_KEY_IDS` and returns a map of
/// key servers to public key IDs.
fn keys_indexed_by_key_server() -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    for tuple in ALL_SERVERS_AND_PUBLIC_KEY_IDS {
        let key = tuple.0.to_string();
        let value: Vec<String> = tuple.1.split_whitespace().map(|s| s.to_string()).collect();
        match map.get_mut(&key) {
            Some(keys) => keys.extend(value),
            None => {
                map.insert(key, value);
            }
        }
    }

    map
}

fn make_dir(dir: &Path) -> Result<(), Box<dyn StdError>> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
        Ok(())
    } else if !dir.is_dir() {
        Err(std::io::Error::other(format!("The entry {} is not a directory.", dir.display())).into())
    } else {
        Ok(())
    }
}
fn make_dir_or_fail(dir: &Path) {
    make_dir(dir).expect(format!("Failed to create the directory {}", dir.display()).as_str());
}

/// Invoke external processes to run autoconf `configure` to generate a makefile for NGINX and
/// then run `make install`.
fn compile_nginx(conf: &BuildConfig, dwner: &Downloader) -> Result<PathBuf, Box<dyn StdError>> {
    let zlib_src_dir = &dwner.zlib.src_dir;
    let openssl_src_dir = &dwner.openssl.src_dir;
    let pcre_src_dir = &dwner.pcre.src_dir;
    let ngx_src_dir = &dwner.ngx.src_dir;
    let ngx_configure_flags = nginx_configure_flags(&conf, zlib_src_dir, openssl_src_dir, pcre_src_dir);
    let nginx_binary_exists = conf.ngx_install_dir.join("sbin").join("nginx").exists();
    let autoconf_makefile_exists = ngx_src_dir.join("Makefile").exists();
    // We find out how NGINX was configured last time, so that we can compare it to what
    // we are going to configure it to this time. If there are no changes, then we can assume
    // that we do not need to reconfigure and rebuild NGINX.
    let build_info_path = ngx_src_dir.join("last-build-info");
    let current_build_info = build_info(&ngx_configure_flags);
    let build_info_no_change = if build_info_path.exists() {
        read_to_string(&build_info_path).map_or(false, |s| s == current_build_info)
    } else {
        false
    };

    println!("NGINX already installed: {nginx_binary_exists}");
    println!("NGINX autoconf makefile already created: {autoconf_makefile_exists}");
    println!("NGINX build info changed: {}", !build_info_no_change);

    if !nginx_binary_exists || !autoconf_makefile_exists || !build_info_no_change {
        fs::create_dir_all(&conf.ngx_install_dir)?;
        configure(ngx_configure_flags, ngx_src_dir)?;
        make(ngx_src_dir, "install")?;
        let mut output = File::create(build_info_path)?;
        // Store the configure flags of the last successful build
        output.write_all(current_build_info.as_bytes())?;
    }
    Ok(ngx_src_dir.to_owned())
}

/// Returns the options in which NGINX was built with
fn build_info(nginx_configure_flags: &[String]) -> String {
    // Flags should contain strings pointing to OS/platform as well as dependency versions,
    // so if any of that changes, it can trigger a rebuild
    nginx_configure_flags.join(" ")
}

/// Generate the flags to use with autoconf `configure` for NGINX based on the downloaded
/// dependencies' paths. Note: the paths differ based on cargo targets because they may be
/// configured differently for different os/platform targets.
fn nginx_configure_flags(
    conf: &BuildConfig,
    zlib_src_dir: &Path,
    openssl_src_dir: &Path,
    pcre_src_dir: &Path,
) -> Vec<String> {
    fn format_source_path(flag: &str, path: &Path) -> String {
        format!(
            "{}={}",
            flag,
            path.as_os_str().to_str().expect("Unable to read source path as string")
        )
    }
    let modules = || -> Vec<String> {
        let mut modules = vec![
            format_source_path("--with-zlib", zlib_src_dir),
            format_source_path("--with-pcre", pcre_src_dir),
            format_source_path("--with-openssl", openssl_src_dir),
        ];
        for module in NGX_BASE_MODULES {
            modules.push(module.to_string());
        }
        modules
    };
    let mut nginx_opts = vec![format_source_path("--prefix", &conf.ngx_install_dir)];
    if conf.ngx_debug {
        println!("Enabling --with-debug");
        nginx_opts.push("--with-debug".to_string());
    }
    if conf.os == "linux" {
        for flag in NGX_LINUX_ADDITIONAL_OPTS {
            nginx_opts.push(flag.to_string());
        }
    }
    for flag in modules() {
        nginx_opts.push(flag);
    }

    nginx_opts
}

/// Run external process invoking autoconf `configure` for NGINX.
fn configure(nginx_configure_flags: Vec<String>, nginx_src_dir: &Path) -> std::io::Result<Output> {
    let flags = nginx_configure_flags
        .iter()
        .map(OsString::from)
        .collect::<Vec<OsString>>();
    let configure_executable = nginx_src_dir.join("configure");
    if !configure_executable.exists() {
        panic!(
            "Unable to find NGINX configure script at: {}",
            configure_executable.to_string_lossy()
        );
    }
    println!(
        "Running NGINX configure script with flags: {:?}",
        nginx_configure_flags.join(" ")
    );
    duct::cmd(configure_executable, flags)
        .dir(nginx_src_dir)
        .stderr_to_stdout()
        .run()
}

/// Run `make` within the NGINX source directory as an external process.
fn make(nginx_src_dir: &Path, arg: &str) -> std::io::Result<Output> {
    // Give preference to the binary with the name of gmake if it exists because this is typically
    // the GNU 4+ on MacOS (if it is installed via homebrew).
    let make_bin_path = match (which("gmake"), which("make")) {
        (Ok(path), _) => Ok(path),
        (_, Ok(path)) => Ok(path),
        _ => Err(IoError::new(NotFound, "Unable to find make in path (gmake or make)")),
    }?;

    // Level of concurrency to use when building nginx - cargo nicely provides this information
    let num_jobs = match env::var("NUM_JOBS") {
        Ok(s) => s.parse::<usize>().ok(),
        Err(_) => thread::available_parallelism().ok().map(|n| n.get()),
    }
    .unwrap_or(1);

    /* Use the duct dependency here to merge the output of STDOUT and STDERR into a single stream,
    and to provide the combined output as a reader which can be iterated over line-by-line. We
    use duct to do this because it is a lot of work to implement this from scratch. */
    cmd!(make_bin_path, "-j", num_jobs.to_string(), arg)
        .dir(nginx_src_dir)
        .stderr_to_stdout()
        .run()
}

/// Reads through the makefile generated by autoconf and finds all of the includes
/// used to compile nginx. This is used to generate the correct bindings for the
/// nginx source code.
fn parse_includes_from_makefile(nginx_autoconf_makefile_path: &PathBuf) -> Vec<PathBuf> {
    fn extract_include_part(line: &str) -> &str {
        line.strip_suffix('\\').map_or(line, |s| s.trim())
    }
    /// Extracts the include path from a line of the autoconf generated makefile.
    fn extract_after_i_flag(line: &str) -> Option<&str> {
        let mut parts = line.split("-I ");
        match parts.next() {
            Some(_) => parts.next().map(extract_include_part),
            None => None,
        }
    }

    let mut includes = vec![];
    let makefile_contents = match read_to_string(nginx_autoconf_makefile_path) {
        Ok(path) => path,
        Err(e) => {
            panic!(
                "Unable to read makefile from path [{}]. Error: {}",
                nginx_autoconf_makefile_path.to_string_lossy(),
                e
            );
        }
    };

    let mut includes_lines = false;
    for line in makefile_contents.lines() {
        if !includes_lines {
            if let Some(stripped) = line.strip_prefix("ALL_INCS") {
                includes_lines = true;
                if let Some(part) = extract_after_i_flag(stripped) {
                    includes.push(part);
                }
                continue;
            }
        }

        if includes_lines {
            if let Some(part) = extract_after_i_flag(line) {
                includes.push(part);
            } else {
                break;
            }
        }
    }

    let makefile_dir = nginx_autoconf_makefile_path
        .parent()
        .expect("makefile path has no parent")
        .parent()
        .expect("objs dir has no parent")
        .to_path_buf()
        .canonicalize()
        .expect("Unable to canonicalize makefile path");

    includes
        .into_iter()
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                makefile_dir.join(path)
            }
        })
        .collect()
}
