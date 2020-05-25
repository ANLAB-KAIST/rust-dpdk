extern crate bindgen;
extern crate cc;
extern crate clang;
extern crate etrace;
extern crate num_cpus;
extern crate regex;

use etrace::some_or;
use regex::Regex;
use std::cmp::Ordering;
use std::env;
use std::fs::*;
use std::io::*;
use std::path::*;
use std::process::Command;

/// We make static functions to have linkable symbols.
/// To avoid collision, we add a magic number to all symbols.
static STATIC_PREFIX: &str = "static_8a9f682d_";

/// Some DPDK headers are not intended to be directly included.
///
/// Heuristically check whether a header allows it to be directly included.
fn check_direct_include(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let file = File::open(path).unwrap();
    let reader = BufReader::new(&file);
    for (_, line) in reader.lines().enumerate() {
        let line_str = line.ok().unwrap().trim().to_lowercase();
        if line_str.starts_with("#error")
            && line_str.find("do not").is_some()
            && line_str.find("include").is_some()
            && line_str.find("directly").is_some()
        {
            return false;
        }
    }
    true
}

/// Information needed to generate DPDK binding.
///
/// Each information is filled at different build stages.
#[derive(Debug)]
struct State {
    /// Location of this crate.
    project_path: PathBuf,

    /// Location of generated files.
    out_path: PathBuf,

    /// Essential link path for C standard library.
    system_include_path: Vec<String>,

    /// DPDK include folder.
    include_path: Option<PathBuf>,

    /// DPDK lib folder.
    library_path: Option<PathBuf>,

    /// List of DPDK header files.
    dpdk_headers: Vec<PathBuf>,

    /// List of DPDK lib files.
    dpdk_links: Vec<PathBuf>,

    /// DPDK config file (will be included as a predefined macro file).
    dpdk_config: Option<PathBuf>,

    /// Names of static functions.
    static_functions: Vec<String>,
}

impl State {
    fn new() -> Self {
        let project_path = PathBuf::from(".").canonicalize().unwrap();
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap())
            .canonicalize()
            .unwrap();
        Self {
            project_path,
            out_path,
            system_include_path: Default::default(),
            include_path: Default::default(),
            library_path: Default::default(),
            dpdk_headers: Default::default(),
            dpdk_links: Default::default(),
            dpdk_config: Default::default(),
            static_functions: Default::default(),
        }
    }

    /// Check current OS.
    ///
    /// Currently, we only accept linux.
    fn check_os(&self) {
        #[cfg(not(unix))]
        panic!("Currently, only xnix OS is supported.");
    }

    /// Check compiler and retrieve link path for C standard libs.
    fn check_compiler(&mut self) {
        let output = Command::new("bash")
            .args(&[
                "-c",
                "clang -march=native -Wp,-v -x c - -fsyntax-only < /dev/null 2>&1 | sed -e '/^#include <...>/,/^End of search/{ //!b };d'",
            ])
            .output()
            .expect("failed to extract cc include path");
        let message = String::from_utf8(output.stdout).unwrap();
        self.system_include_path
            .extend(message.lines().map(|x| String::from(x.trim())));
    }

    /// Find DPDK install path.
    ///
    /// 1. If `RTE_SDK` is set, find its installed directory.
    /// 2. If `RTE_TARGET` is set, DPDK is at `RTE_SDK/RTE_TARGET`.
    /// 3. If not set, default `RTE_TARGET` is `build`.
    /// 4. If DPDK is installed at `/usr/local/include/dpdk`, use it.
    /// 5. If none is found, download from `https://github.com/DPDK/dpdk.git`.
    fn find_dpdk(&mut self) {
        if let Ok(path_string) = env::var("RTE_SDK") {
            let mut dpdk_path = PathBuf::from(path_string);
            if let Ok(target_string) = env::var("RTE_TARGET") {
                dpdk_path = dpdk_path.join(target_string);
            } else {
                dpdk_path = dpdk_path.join("build");
            }
            self.include_path = Some(dpdk_path.join("include"));
            self.library_path = Some(dpdk_path.join("lib"));
        } else if Path::new("/usr/local/include/dpdk/rte_config.h").exists() {
            self.include_path = Some(PathBuf::from("/usr/local/include/dpdk"));
            self.library_path = Some(PathBuf::from("/usr/local/lib"));
        } else {
            // Automatic download
            let dir_path = Path::new(&self.out_path).join("3rdparty");
            if !dir_path.exists() {
                create_dir(&dir_path).ok();
            }
            assert!(dir_path.exists());
            let git_path = dir_path.join("dpdk");
            if !git_path.exists() {
                Command::new("git")
                    .args(&[
                        "clone",
                        "-b",
                        "v20.02",
                        "https://github.com/DPDK/dpdk.git",
                        git_path.to_str().unwrap(),
                    ])
                    .output()
                    .expect("failed to run git command");
            }
            Command::new("make")
                .args(&["-C", git_path.to_str().unwrap(), "defconfig"])
                .output()
                .expect("failed to run make command");
            Command::new("make")
                .env("EXTRA_CFLAGS", " -fPIC ")
                .args(&[
                    "-C",
                    git_path.to_str().unwrap(),
                    &format!("-j{}", num_cpus::get()),
                ])
                .output()
                .expect("failed to run make command");

            self.include_path = Some(git_path.join("build").join("include"));
            self.library_path = Some(git_path.join("build").join("lib"));
        }
        assert!(self.include_path.as_ref().unwrap().exists());
        assert!(self.library_path.as_ref().unwrap().exists());
        let config_header = self.include_path.as_ref().unwrap().join("rte_config.h");
        assert!(config_header.exists());
        println!(
            "cargo:rerun-if-changed={}",
            self.include_path.as_ref().unwrap().to_str().unwrap()
        );
        println!(
            "cargo:rerun-if-changed={}",
            self.library_path.as_ref().unwrap().to_str().unwrap()
        );
        for entry in self
            .project_path
            .join("gen")
            .read_dir()
            .expect("read_dir failed")
        {
            if let Ok(entry) = entry {
                let path = entry.path();

                if let Some(ext) = path.extension() {
                    if ext == "template" {
                        println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
                    }
                }
            }
        }

        println!("cargo:rerun-if-env-changed=RTE_SDK");
        println!("cargo:rerun-if-env-changed=RTE_TARGET");

        self.dpdk_config = Some(config_header);
    }

    /// Search through DPDK's link dir and extract library names.
    fn find_link_libs(&mut self) {
        let lib_dir = self.library_path.as_ref().unwrap();

        let mut libs = vec![];
        for entry in lib_dir.read_dir().expect("read_dir failed") {
            if let Ok(entry) = entry {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                if let Some(ext) = path.extension() {
                    if ext != "a" {
                        //if ext != "so" {
                        continue;
                    }
                } else {
                    continue;
                }

                if let Some(file_stem) = path.file_stem() {
                    let string = file_stem.to_str().unwrap();
                    if !string.starts_with("librte_") {
                        continue;
                    }
                    libs.push(path.clone());
                } else {
                    continue;
                }
            } else {
                continue;
            }
        }
        if libs.is_empty() {
            //panic!("Cannot find any shared libraries. Check if DPDK is built with CONFIG_RTE_BUILD_SHARED_LIB=y option.");
            panic!("Cannot find any libraries.");
        }
        libs.sort();
        libs.dedup();
        self.dpdk_links = libs;
    }
    /// Prepare a header file which contains all available DPDK headers.
    fn make_all_in_one_header(&mut self) {
        let include_dir = self.include_path.as_ref().unwrap();
        let dpdk_config = self.dpdk_config.as_ref().unwrap();
        let blacklist = vec!["rte_function_versioning"];
        let mut headers = vec![];
        for entry in include_dir.read_dir().expect("read_dir failed") {
            if let Ok(entry) = entry {
                let path = entry.path();

                if !path.is_file() {
                    continue;
                }
                if let Some(stem) = path.file_stem() {
                    if blacklist.contains(&stem.to_str().unwrap()) {
                        continue;
                    }
                }

                if let Some(ext) = path.extension() {
                    if ext != "h" {
                        continue;
                    }
                } else {
                    continue;
                }
                if path == *dpdk_config {
                    continue;
                }
                if !check_direct_include(&path) {
                    continue;
                }
                headers.push(path);
            } else {
                continue;
            }
        }
        headers.sort();
        headers.dedup();
        assert!(!headers.is_empty());

        // Heuristically remove platform-specific headers
        let mut name_set = vec![];
        for file in &headers {
            let file_name = String::from(file.file_stem().unwrap().to_str().unwrap());
            name_set.push(file_name);
        }
        let mut new_vec = vec![];
        'outer: for file in &headers {
            let file_name = file.file_stem().unwrap().to_str().unwrap();
            for prev_name in &name_set {
                if file_name.starts_with(&format!("{}_", prev_name)) {
                    continue 'outer;
                }
            }
            new_vec.push(file.clone());
        }

        new_vec.sort_by(|left, right| {
            let left_str = left.file_stem().unwrap().to_str().unwrap();
            let right_str = right.file_stem().unwrap().to_str().unwrap();
            let left_count = left_str.split('_').count();
            let right_count = right_str.split('_').count();
            match left_count.cmp(&right_count) {
                Ordering::Equal => left_str.cmp(&right_str),
                Ordering::Less => Ordering::Less,
                Ordering::Greater => Ordering::Greater,
            }
        });
        new_vec.dedup();
        headers = new_vec;

        self.dpdk_headers = headers;
        let template_path = self.project_path.join("gen/dpdk.h.template");
        let target_path = self.out_path.join("dpdk.h");
        let mut template = File::open(template_path).unwrap();
        let mut target = File::create(target_path).unwrap();

        let mut template_string = String::new();
        template.read_to_string(&mut template_string).ok();

        let mut headers_string = String::new();
        for header in &self.dpdk_headers {
            headers_string += &format!(
                "#include \"{}\"\n",
                header.file_name().unwrap().to_str().unwrap()
            );
        }
        let formatted_string = template_string.replace("%header_list%", &headers_string);

        target.write_fmt(format_args!("{}", formatted_string)).ok();
    }

    /// Generate wrappers for static functions and create persistent link for PMDs.
    fn generate_static_impls_and_link_pmds(&mut self) {
        let clang = clang::Clang::new().unwrap();

        let index = clang::Index::new(&clang, true, true);

        let header_path = self.out_path.join("dpdk.h");

        let mut argument = vec![
            "-march=native".into(),
            format!(
                "-I{}",
                self.include_path.as_ref().unwrap().to_str().unwrap()
            ),
            //.to_string(),
            format!("-I{}", self.out_path.to_str().unwrap()), //.to_string(),
            "-imacros".into(),
            self.dpdk_config
                .as_ref()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        ];

        for path in self.system_include_path.iter() {
            argument.push(format!("-I{}", path).to_string());
        }

        let trans_unit = index
            .parser(header_path)
            .arguments(&argument)
            .parse()
            .unwrap();
        let diagnostics = trans_unit.get_diagnostics();
        let mut fatal_count = 0;
        for diagnostic in diagnostics {
            if let clang::diagnostic::Severity::Fatal = diagnostic.get_severity() {
                fatal_count += 1;
            }
        }
        if fatal_count > 0 {
            panic!(format!("Encountering {} fatal parse errors", fatal_count));
        }

        let mut persist_link_list = vec![];
        let mut static_def_list = vec![];
        let mut static_impl_list = vec![];

        fn format_arg(type_: clang::Type, name: String) -> String {
            match type_.get_kind() {
                clang::TypeKind::DependentSizedArray | clang::TypeKind::VariableArray => {
                    panic!("Not supported (DependentSizedArray");
                }
                clang::TypeKind::ConstantArray => {
                    let elem_type = type_.get_element_type().unwrap();
                    let array_size = type_.get_size().unwrap();
                    let name = name + &format!("[{}]", array_size);
                    format_arg(elem_type, name)
                }
                clang::TypeKind::IncompleteArray => {
                    let elem_type = type_.get_element_type().unwrap();
                    let name = name + "[]";
                    format_arg(elem_type, name)
                }
                _ => {
                    return format!("{} {}", type_.get_display_name(), name);
                }
            }
        }

        for f in trans_unit
            .get_entity()
            .get_children()
            .into_iter()
            .filter(|e| e.get_kind() == clang::EntityKind::FunctionDecl)
        {
            let name = some_or!(f.get_name(), continue);
            let storage = some_or!(f.get_storage_class(), continue);
            let return_type = some_or!(f.get_result_type(), continue);
            let is_decl = f.is_definition();

            if storage == clang::StorageClass::None && !is_decl && name.starts_with("rte_pmd_") {
                // non-static function definition for a PMD is found.
                persist_link_list.push(name);
            } else if storage == clang::StorageClass::Static && is_decl && !name.starts_with('_') {
                // Declaration of static function is found (skip if function name starts with _).
                let mut arg_strings = Vec::new();
                let mut param_strings = Vec::new();
                let return_type_string = return_type.get_display_name();
                if let Some(args) = f.get_arguments() {
                    for (counter, arg) in args.iter().enumerate() {
                        let arg_name = arg
                            .get_display_name()
                            .unwrap_or_else(|| format!("_unnamed_arg{}", counter));
                        let type_ = arg.get_type().unwrap();
                        arg_strings.push(format_arg(type_, arg_name.clone()));
                        param_strings.push(arg_name);
                    }
                }
                let arg_string = arg_strings.join(", ");
                let param_string = param_strings.join(", ");
                static_def_list.push(format!(
                    "{ret} {prefix}{name} ({args})",
                    ret = return_type_string,
                    prefix = STATIC_PREFIX,
                    name = name,
                    args = arg_string
                ));
                static_impl_list.push(format!(
                    "{{ return {name}({params}); }}",
                    name = name,
                    params = param_string
                ));
                self.static_functions.push(name.clone());
            }
        }

        let header_path = self.out_path.join("static.h");
        let header_template = self.project_path.join("gen/static.h.template");
        let source_path = self.out_path.join("static.c");
        let source_template = self.project_path.join("gen/static.c.template");

        let mut header_defs = String::new();
        let mut static_impls = String::new();
        for (def_, impl_) in Iterator::zip(static_def_list.iter(), static_impl_list.iter()) {
            header_defs += &format!("{};\n", def_);
            static_impls += &format!("{}{}\n", def_, impl_);
        }
        let mut perlist_links = String::new();
        for name in &persist_link_list {
            perlist_links += &format!("void* persist_{}() {{\n", name);
            perlist_links += &format!("\treturn {};\n", name);
            perlist_links += &"}}\n";
        }

        let mut template = File::open(header_template).unwrap();
        let mut template_string = String::new();
        template.read_to_string(&mut template_string).ok();
        let formatted_string = template_string.replace("%header_defs%", &header_defs);
        let mut target = File::create(header_path).unwrap();
        target.write_fmt(format_args!("{}", &formatted_string)).ok();

        let mut template = File::open(source_template).unwrap();
        let mut template_string = String::new();
        template.read_to_string(&mut template_string).ok();
        let formatted_string = template_string.replace("%static_impls%", &static_impls);
        let formatted_string = formatted_string.replace("%persist_pmds%", &perlist_links);
        let mut target = File::create(source_path).unwrap();
        target.write_fmt(format_args!("{}", &formatted_string)).ok();
    }

    /// Generate Rust bindings from DPDK source.
    fn generate_rust_def(&mut self) {
        let dpdk_include_path = self.include_path.as_ref().unwrap();
        let dpdk_config_path = self.dpdk_config.as_ref().unwrap();

        let header_path = self.out_path.join("static.h");
        let target_path = self.out_path.join("dpdk.rs");
        bindgen::builder()
            .header(header_path.to_str().unwrap())
            .clang_arg(format!("-I{}", dpdk_include_path.to_str().unwrap()))
            .clang_arg(format!("-I{}", self.out_path.to_str().unwrap()))
            .clang_arg("-imacros")
            .clang_arg(dpdk_config_path.to_str().unwrap())
            .clang_arg("-march=native")
            .clang_arg("-Wno-everything")
            .rustfmt_bindings(true)
            .opaque_type("max_align_t")
            .opaque_type("rte_event.*")
            .generate()
            .unwrap()
            .write_to_file(target_path)
            .ok();
    }

    /// Generate Rust source files.
    fn generate_lib_rs(&mut self) {
        let template_path = self.project_path.join("gen/lib.rs.template");
        let target_path = self.out_path.join("lib.rs");

        let format = Regex::new(r"rte_pmd_(\w+)").unwrap();

        let pmds: Vec<_> = self
            .dpdk_links
            .iter()
            .filter_map(|link| {
                let link_name = link.file_stem().unwrap().to_str().unwrap();
                format
                    .captures(link_name)
                    .map(|capture| format!("\"{}\"", &capture[1]))
            })
            .collect();

        let pmds_string = pmds.join(",\n");

        let static_use_string = self
            .static_functions
            .iter()
            .map(|name| {
                format!(
                    "pub use dpdk::{prefix}{name} as {name};",
                    prefix = STATIC_PREFIX,
                    name = name
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let mut template = File::open(template_path).unwrap();
        let mut template_string = String::new();
        template.read_to_string(&mut template_string).ok();

        let formatted_string = template_string.replace("%pmd_list%", &pmds_string);
        let formatted_string = formatted_string.replace("%static_use_defs%", &static_use_string);

        let mut target = File::create(target_path).unwrap();
        target.write_fmt(format_args!("{}", formatted_string)).ok();
    }

    /// Do compile.
    fn compile(&mut self) {
        let dpdk_include_path = self.include_path.as_ref().unwrap();
        let dpdk_config = self.dpdk_config.as_ref().unwrap();
        let source_path = self.out_path.join("static.c");
        let lib_path = self.library_path.as_ref().unwrap();

        println!(
            "cargo:rustc-link-search=native={}",
            lib_path.to_str().unwrap()
        );

        // Legacy mode: Rust cargo cannot recognize library groups (libdpdk.a).
        let format = Regex::new(r"lib(.*)\.(a)").unwrap();
        for link in &self.dpdk_links {
            let lib_name = link.file_name().unwrap().to_str().unwrap();

            if let Some(capture) = format.captures(lib_name) {
                let link_name = &capture[1];
                if link_name == "dpdk" {
                    continue;
                }
                println!("cargo:rustc-link-lib=static={}", link_name);
            }
        }
        println!("cargo:rustc-link-lib=numa");

        cc::Build::new()
            .file(source_path)
            .static_flag(true)
            .shared_flag(false)
            .opt_level(3)
            .include(dpdk_include_path)
            .include(&self.out_path)
            .flag("-w") // hide warnings
            .flag("-march=native")
            .flag("-imacros")
            .flag(dpdk_config.to_str().unwrap())
            .compile("lib_static_wrapper.a");
    }
}

fn main() {
    let mut state = State::new();
    state.check_os();
    state.check_compiler();
    state.find_dpdk();
    state.find_link_libs();
    state.make_all_in_one_header();
    state.generate_static_impls_and_link_pmds();
    state.generate_rust_def();
    state.generate_lib_rs();
    state.compile();
}
