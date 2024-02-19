extern crate bindgen;
extern crate cc;
extern crate clang;
extern crate etrace;
extern crate itertools;
extern crate num_cpus;
extern crate regex;

use etrace::some_or;
use itertools::Itertools;
use regex::Regex;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::fs::*;
use std::io::*;
use std::path::*;
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// We make additional wrapper functions for existing bindings.
/// To avoid collision, we add a magic prefix for each.
static PREFIX: &str = "prefix_8a9f682d_";

/// Convert `/**` comments into `///` comments
fn strip_comments(comment: String) -> String {
    comment
        .split('\n')
        .map(|line| {
            line.trim_matches(|c| c == ' ' || c == '/' || c == '*')
                .replace('\t', "    ")
        })
        .map(|line| format!("/// {}", line))
        .join("\n")
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
    dpdk_headers: Vec<String>,

    /// List of DPDK lib files.
    dpdk_links: Vec<PathBuf>,

    /// DPDK config file (will be included as a predefined macro file).
    dpdk_config: Option<PathBuf>,

    /// Use definitions for automatically found EAL APIs.
    eal_function_use_defs: Vec<String>,

    /// Use definitions for automatically found EAL APIs (Global).
    global_eal_function_use_defs: Vec<String>,

    /// Names of `static inline` functions found in DPDK headers.
    static_functions: Vec<String>,

    /// Macro constants are not expanded when it uses other macro functions.
    static_constants: String,
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
            eal_function_use_defs: Default::default(),
            global_eal_function_use_defs: Default::default(),
            static_functions: Default::default(),
            static_constants: Default::default(),
        }
    }

    /// Create clang trans unit from given header file.
    /// This function will fill options from current `State`.
    fn trans_unit_from_header<'a>(
        &self,
        index: &'a clang::Index,
        header_path: PathBuf,
        do_macro: bool,
    ) -> clang::TranslationUnit<'a> {
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
            .detailed_preprocessing_record(do_macro)
            .arguments(&argument)
            .parse()
            .unwrap();
        let fatal_diagnostics = trans_unit
            .get_diagnostics()
            .iter()
            .filter(|diagnostic| clang::diagnostic::Severity::Fatal == diagnostic.get_severity())
            .count();
        if fatal_diagnostics > 0 {
            panic!("Encountering {} fatal parse error(s)", fatal_diagnostics);
        }
        trans_unit
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
            .args([
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
    /// After 20.11 update, build system is integrated into meson.
    /// Thus, it is difficult to obtain build path manually.
    /// Currently, one must install DPDK to one's system.
    /// This function validates whether DPDK is installed.
    fn find_dpdk(&mut self) {
        // To find correct lib path of this platform.
        let output = Command::new("cc")
            .args(["-dumpmachine"])
            .output()
            .expect("failed obtain current machine");
        let machine_string = String::from(String::from_utf8(output.stdout).unwrap().trim());
        let config_header = PathBuf::from("/usr/local/include/rte_config.h");
        let build_config_header = PathBuf::from("/usr/local/include/rte_build_config.h");

        if config_header.exists() && build_config_header.exists() {
            self.include_path = Some(PathBuf::from("/usr/local/include"));
            self.library_path = Some(PathBuf::from(format!("/usr/local/lib/{}", machine_string)));
        } else {
            panic!(
                "DPDK is not installed on your system! (Cannot find {} nor {})",
                config_header.to_str().unwrap(),
                build_config_header.to_str().unwrap()
            );
        }
        println!("cargo:rerun-if-changed={}", config_header.to_str().unwrap());
        println!(
            "cargo:rerun-if-changed={}",
            build_config_header.to_str().unwrap()
        );
        for entry in self
            .project_path
            .join("gen")
            .read_dir()
            .expect("read_dir failed")
            .flatten()
        {
            let path = entry.path();

            if let Some(ext) = path.extension() {
                if ext == "template" {
                    println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
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
        // dlb drivers have duplicated enum definitions.
        let mut headers = vec![];
        for entry in include_dir.read_dir().expect("read_dir failed") {
            if let Ok(entry) = entry {
                let path = entry.path();

                if !path.is_file() {
                    continue;
                }
                if let Some(stem) = path.file_stem() {
                    if stem.to_str().unwrap().starts_with("rte_pmd_") {
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
                headers.push(path);
            } else {
                continue;
            }
        }
        headers.sort();
        headers.dedup();
        assert!(!headers.is_empty());

        // Heuristically remove platform-specific headers
        let platform_set = vec![
            "x86", "x86_64", "x64", "arm", "arm32", "arm64", "amd64", "generic", "gfni", "32", "64",
        ];

        // Remove blacklist headers
        let blacklist_prefix = vec!["rte_acc_"];
        let mut name_set: Vec<String> = vec![];
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
            for platform in &platform_set {
                if file_name.ends_with(&format!("_{}", platform)) {
                    continue 'outer;
                }
            }
            for black in &blacklist_prefix {
                if file_name.starts_with(black) {
                    continue 'outer;
                }
            }
            // println!("cargo:warning=header-name: {}", file_name);
            new_vec.push(file.clone());
        }
        new_vec.sort_by(|left, right| {
            let left_str = left.file_stem().unwrap().to_str().unwrap();
            let right_str = right.file_stem().unwrap().to_str().unwrap();
            let left_count = left_str.split('_').count();
            let right_count = right_str.split('_').count();
            match left_count.cmp(&right_count) {
                Ordering::Equal => left_str.cmp(right_str),
                Ordering::Less => Ordering::Less,
                Ordering::Greater => Ordering::Greater,
            }
        });

        let mut header_names: Vec<_> = new_vec
            .into_iter()
            .map(|header| header.file_name().unwrap().to_str().unwrap().to_string())
            .filter(|x| x != "rte_config.h" || x != "rte_common.h")
            .collect();
        header_names.sort();
        header_names.dedup();
        header_names.insert(0, "rte_config.h".into());
        header_names.insert(0, "rte_common.h".into());

        // Generate all-in-one dpdk header (`dpdk.h`).
        self.dpdk_headers = header_names;
        let template_path = self.project_path.join("gen/dpdk.h.template");
        let target_path = self.out_path.join("dpdk.h");
        let mut template = File::open(template_path).unwrap();
        let mut target = File::create(target_path).unwrap();
        let mut template_string = String::new();
        template.read_to_string(&mut template_string).ok();
        let mut headers_string = String::new();
        for header in &self.dpdk_headers {
            headers_string += &format!("#include \"{}\"\n", header);
        }
        let formatted_string = template_string.replace("%header_list%", &headers_string);
        target.write_fmt(format_args!("{}", formatted_string)).ok();
    }

    /// Extract trivial EAL APIs whose paramter types are all primitive (e.g. `uint8_t`).
    ///
    /// This function does followings:
    /// 1. List up all headers in `librte_eal/include/generic`
    /// 1. Also list up some selected headers in `librte_eal/include`
    /// 1. Extract all function in the listed headers.
    /// 1. Filter out "trivial" FFI implementations. For instance, a function whose arguments and
    ///    return type are primitive types.
    /// 1. Generate a trait which trivially invokes the selected foriegn functions.
    /// 1. Remove `rte_` prefix of them.
    fn extract_eal_apis(&mut self) {
        // List of acceptable primitive types.
        let arg_type_whitelist: HashMap<_, _> = vec![
            ("void", "()"),
            ("int", "i32"),
            ("unsigned int", "u32"),
            ("size_t", "usize"),
            ("ssize_t", "isize"),
            ("uint8_t", "u8"),
            ("uint16_t", "u16"),
            ("uint32_t", "u32"),
            ("uint64_t", "u64"),
            ("int8_t", "i8"),
            ("int16_t", "i16"),
            ("int32_t", "i32"),
            ("int64_t", "i64"),
        ]
        .iter()
        .map(|(c_type, rust_type)| (String::from(*c_type), String::from(*rust_type)))
        .collect();

        // Set of function definition strings (Rust), coupled with function names.
        // This will prevent duplicated function definitions.
        let mut use_def_map = HashMap::new();
        let mut global_use_def_map = HashMap::new();
        let target_path = self.out_path.join("dpdk.h");
        {
            let clang = clang::Clang::new().unwrap();
            let index = clang::Index::new(&clang, true, true);
            let trans_unit = self.trans_unit_from_header(&index, target_path, false);

            // Iterate through each EAL header files and extract function definitions.
            'each_function: for f in trans_unit
                .get_entity()
                .get_children()
                .into_iter()
                .filter(|e| e.get_kind() == clang::EntityKind::FunctionDecl)
            {
                let name = some_or!(f.get_name(), continue);
                let storage = some_or!(f.get_storage_class(), continue);
                let return_type = some_or!(f.get_result_type(), continue);
                let is_decl = f.is_declaration();
                let is_inline_fn = f.is_inline_function();

                let comment = f
                    .get_comment()
                    .map(strip_comments)
                    .unwrap_or_else(|| "".to_string());

                if use_def_map.contains_key(&name) {
                    // Skip duplicate
                    continue;
                }
                if name.starts_with('_') {
                    // Skip hidden implementations
                    continue;
                }
                if clang::StorageClass::Static != storage || !(is_decl && is_inline_fn) {
                    continue;
                }
                // println!("cargo:warning={} {} {} {:?}", name, is_decl, f.is_inline_function(), storage);

                // Extract type names in C and Rust.
                let c_return_type_string = return_type.get_display_name();
                let rust_return_type_string =
                    some_or!(arg_type_whitelist.get(&c_return_type_string), {
                        continue;
                    });

                let args = f.get_arguments().unwrap_or_default();
                let mut arg_names = Vec::new();
                let mut rust_arg_names = Vec::new();
                // Format arguments
                for (counter, arg) in args.iter().enumerate() {
                    let arg_name = arg
                        .get_display_name()
                        .unwrap_or_else(|| format!("_unnamed_arg{}", counter));
                    let c_type_name = arg.get_type().unwrap().get_display_name();
                    let rust_type_name = some_or!(arg_type_whitelist.get(&c_type_name), {
                        // If the given C type is not supported as primitive Rust types. Skip
                        // processing this function.
                        continue 'each_function;
                    });
                    rust_arg_names.push(format!("{}: {}", arg_name, rust_type_name));
                    arg_names.push(arg_name);
                }
                // Returning void (`-> ()`) triggers clippy error, skip.
                let ret = if rust_return_type_string == "()" {
                    String::new()
                } else {
                    format!(" -> {}", rust_return_type_string)
                };
                /*
                Following code generates trait function definitions like this:

                /// Comment from C
                #[inline(always)]
                fn function_name ( &self, arg: u8 ) -> u8 {
                    unsafe { crate::rte_function_name(arg) }
                }
                */
                use_def_map.insert(name.clone(), format!("\n{comment}\n#[inline(always)]\nfn {func_name} ( &self, {rust_args} ){ret} {{\n\tunsafe {{ crate::{name}({c_arg}) }}\n}}", comment=comment, func_name=name.trim_start_matches("rte_"), name=name, rust_args=rust_arg_names.join(", "), ret=ret, c_arg=arg_names.join(", ")));
                /*
                Following code generates trait function definitions like this:

                /// Comment from C
                #[inline(always)]
                fn function_name ( arg: u8 ) -> u8 {
                    unsafe { crate::rte_function_name(arg) }
                }
                */
                global_use_def_map.insert(name.clone(), format!("\n{comment}\n#[inline(always)]\nfn {func_name} ( {rust_args} ){ret} {{\n\tunsafe {{ crate::{name}({c_arg}) }}\n}}", comment=comment, func_name=name.trim_start_matches("rte_"), name=name, rust_args=rust_arg_names.join(", "), ret=ret, c_arg=arg_names.join(", ")));
            }
        }
        self.eal_function_use_defs = use_def_map.values().cloned().collect();
        self.global_eal_function_use_defs = global_use_def_map.values().cloned().collect();
    }

    /// Generate wrappers for static functions and create explicit links for PMDs.
    fn generate_static_impls_and_link_pmds(&mut self) {
        let header_path = self.out_path.join("dpdk.h");
        let clang = clang::Clang::new().unwrap();
        let index = clang::Index::new(&clang, true, true);
        let trans_unit = self.trans_unit_from_header(&index, header_path.clone(), false);

        // List of `static inline` functions (definitions).
        let mut static_def_list = vec![];
        // List of `static inline` functions (declarations).
        let mut static_impl_list = vec![];

        // Generate C code from given each static function's information.
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
                    format!("{} {}", type_.get_display_name(), name)
                }
            }
        }

        // Iterate through the `dpdk.h` header file.
        for f in trans_unit
            .get_entity()
            .get_children()
            .into_iter()
            .filter(|e| e.get_kind() == clang::EntityKind::FunctionDecl)
        {
            let name = some_or!(f.get_name(), continue);
            let storage = some_or!(f.get_storage_class(), continue);
            let return_type = some_or!(f.get_result_type(), continue);
            let is_inline = f.is_inline_function();
            let is_decl = f.is_declaration();

            if storage == clang::StorageClass::Static
                && is_decl
                && is_inline
                && !name.starts_with('_')
            {
                // Declaration of static function is found (skip if function name starts with _).
                if self.static_functions.contains(&name) {
                    continue;
                }
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
                    prefix = PREFIX,
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

        if env::var("CARGO_FEATURE_CONSTANTS_CACHE").is_ok() {
            println!("cargo:warning=Using cached constants data");
            let cache_path = self.project_path.join("gen/constants.rs.cache");
            let mut cache_file = File::open(cache_path).unwrap();
            let mut cache_string = String::new();
            cache_file.read_to_string(&mut cache_string).ok();
            self.static_constants = cache_string;
        }
        // Check macro
        else {
            let mut static_constants_vec: Vec<(String, String, u64)> = Vec::new();
            let trans_unit = self.trans_unit_from_header(&index, header_path, true);
            let mut macro_candidates = Vec::new();

            let macro_const_fmt = Regex::new(r"[A-Z][A-Z0-9]*(_[A-Z][A-Z0-9]*)*").unwrap();
            for f in trans_unit
                .get_entity()
                .get_children()
                .into_iter()
                .filter(|e| e.get_kind() == clang::EntityKind::MacroDefinition)
            {
                let name = some_or!(f.get_name(), continue);
                if f.is_builtin_macro() {
                    continue;
                }
                if f.is_function_like_macro() {
                    continue;
                }
                if !macro_const_fmt.is_match(name.as_str()) {
                    continue;
                }
                macro_candidates.push(name.trim().to_string());
            }
            macro_candidates.sort();
            macro_candidates.dedup();
            // macro_candidates.drain(100..);

            let test_template = self.project_path.join("gen/int_test.c");
            let builder = cc::Build::new();
            let compiler = builder.get_compiler();
            let cc_name = compiler.path().to_str().unwrap().to_string();

            let dpdk_include_path = self.include_path.as_ref().unwrap();
            let dpdk_config_path = self.dpdk_config.as_ref().unwrap();

            let cpus = num_cpus::get();

            let workqueue = Arc::new(crossbeam_queue::SegQueue::<String>::new());
            for name in macro_candidates {
                workqueue.push(name);
            }
            let dpdk_include = dpdk_include_path.to_str().unwrap();
            let output_include = self.out_path.to_str().unwrap();

            let compile_start_at = Instant::now();
            let mut wait_list = Vec::new();
            let start_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();
            for _idx in 0..cpus {
                let test_template = test_template.clone();

                let queue = workqueue.clone();
                let cc_name = cc_name.clone();
                let dpdk_include = dpdk_include.to_string();
                let output_include = output_include.to_string();
                let dpdk_config_path = dpdk_config_path.clone();
                let out_path = self.out_path.clone();
                let task = move || {
                    let mut results = Vec::new();
                    while let Some(name) = queue.pop() {
                        let target_bin_path =
                            out_path.join(format!("int_test_{}_{}", start_time, name));

                        let mut return_value = None;
                        let try_args = vec![
                            ("U64_FMT", "u64"),
                            ("ULL_FMT", "u64"), // ("ULL_FMT", "u128")
                            ("U32_FMT", "u32"),
                        ];
                        for (fmt_name, type_name) in try_args {
                            if target_bin_path.exists() {
                                fs::remove_file(target_bin_path.clone()).unwrap();
                            }
                            let ret = Command::new(cc_name.clone())
                                .arg("-Wall")
                                .arg("-Wextra")
                                .arg("-Werror")
                                .arg("-std=c99")
                                .arg(format!("-I{}", dpdk_include))
                                .arg(format!("-I{}", output_include))
                                .arg("-imacros")
                                .arg(dpdk_config_path.to_str().unwrap())
                                .arg("-march=native")
                                .arg(format!("-D__CHECK_FMT={}", fmt_name))
                                .arg(format!("-D__CHECK_VAL={}", name))
                                .arg("-o")
                                .arg(target_bin_path.clone())
                                .arg(test_template.clone())
                                .arg("-lrte_eal")
                                .output();
                            if let Ok(ret) = ret {
                                if ret.status.success() {
                                    let ret =
                                        Command::new(target_bin_path.clone()).output().unwrap();
                                    let str = String::from_utf8(ret.stdout).unwrap();
                                    let val: u64 = str.trim().parse().unwrap(); // See ULL_FMT to use which integer. u64 or u128.
                                    return_value = Some((
                                        name.clone().to_ascii_uppercase(),
                                        type_name.into(),
                                        val,
                                    ));
                                }
                            }
                            if return_value.is_some() {
                                break;
                            }
                        }

                        // println!("cargo:warning=compile thread task done {}, {:?}", idx, return_value);
                        results.push(return_value);
                        if target_bin_path.exists() {
                            fs::remove_file(target_bin_path.clone()).unwrap();
                        }
                    }
                    // println!("cargo:warning=compile thread terminated {}", idx);
                    results
                };
                let handle = std::thread::spawn(task);
                wait_list.push(handle);
                //pool.execute(task);
                // task();
            }
            let mut all_results = Vec::new();
            for handle in wait_list {
                let results = handle.join().unwrap();
                all_results.extend(results);
            }

            let mut some_count = 0;
            let mut none_count = 0;
            for val in all_results {
                if let Some((name, int_type, val)) = val {
                    // println!("cargo:warning=macro {}: {} = {}", name, int_type, val);
                    static_constants_vec.push((name, int_type, val));
                    some_count += 1;
                } else {
                    none_count += 1;
                }
            }
            let compile_end_at = Instant::now();
            println!(
                "cargo:warning=compile time: {:02}s, {}/{} macros processed",
                (compile_end_at - compile_start_at).as_secs_f64(),
                some_count,
                some_count + none_count,
            );

            let mut zero_prefix_list = Vec::new();
            for (name, int_type, val) in static_constants_vec.iter() {
                if *val == 0 && int_type == "u32" {
                    let segs = name.split('_').collect::<Vec<_>>();
                    if segs.len() <= 3 {
                        // 이름이 너무 짧은 경우
                        continue;
                    }
                    let prefix = segs[..segs.len() - 1].join("_") + "_";
                    zero_prefix_list.push((name.clone(), prefix));
                }
            }
            zero_prefix_list.sort();
            zero_prefix_list.dedup();
            let mut change_list = Vec::new();
            for (other_name, other_int_type, _) in static_constants_vec.iter() {
                for (name, prefix) in zero_prefix_list.iter() {
                    if *other_name != *name
                        && other_name.starts_with(prefix)
                        && *other_int_type == "u64"
                    {
                        change_list.push((name.clone(), other_int_type.clone()));
                        break;
                    }
                }
            }
            change_list.sort();
            change_list.dedup();
            for (name, int_type, val) in static_constants_vec.iter_mut() {
                for (change_name, change_int_type) in change_list.iter() {
                    if *name == *change_name {
                        println!(
                            "cargo:warning=macro {}:{}, {} -> {}",
                            name, val, int_type, change_int_type
                        );
                        *int_type = change_int_type.clone();
                    }
                }
            }

            let mut total_string = String::new();
            total_string += "pub mod constants {\n";
            total_string += &static_constants_vec
                .iter()
                .map(|(name, int_type, val): &(String, String, u64)| {
                    format!("pub const {}: {} = {};", name, int_type, val)
                })
                .join("\n");
            total_string += "\n}\n";
            self.static_constants = total_string;
        }
        // gcc -S test.c -Wall -Wextra -std=c99 -Werror

        let header_path: PathBuf = self.out_path.join("static.h");
        let header_template = self.project_path.join("gen/static.h.template");
        let source_path: PathBuf = self.out_path.join("static.c");
        let source_template = self.project_path.join("gen/static.c.template");

        let header_defs = static_def_list
            .iter()
            .map(|def_| format!("{};", def_))
            .join("\n");
        let static_impls = Iterator::zip(static_def_list.iter(), static_impl_list.iter())
            .map(|(def_, decl_)| format!("{}{}", def_, decl_))
            .join("\n");

        // Generate header file from template
        let mut template = File::open(header_template).unwrap();
        let mut template_string = String::new();
        template.read_to_string(&mut template_string).ok();
        let formatted_string = template_string.replace("%header_defs%", &header_defs);
        let mut target = File::create(header_path).unwrap();
        target.write_fmt(format_args!("{}", &formatted_string)).ok();

        // Generate source file from template
        let mut template = File::open(source_template).unwrap();
        let mut template_string = String::new();
        template.read_to_string(&mut template_string).ok();
        let formatted_string = template_string.replace("%static_impls%", &static_impls);
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
            .clang_arg("-DALLOW_INTERNAL_API") // We will not use internal API, but it is necessary to generate bindings.
            .opaque_type("vmbus_bufring")
            .opaque_type("rte_avp_desc")
            .opaque_type("rte_.*_hdr")
            .opaque_type("rte_arp_ipv4")
            .opaque_type("__*")
            .generate()
            .unwrap()
            .write_to_file(target_path)
            .ok();
    }

    /// Generate Rust source files.
    fn generate_lib_rs(&mut self) {
        let template_path = self.project_path.join("gen/lib.rs.template");
        let target_path = self.out_path.join("lib.rs");

        let static_use_string = self
            .static_functions
            .iter()
            .map(|name| {
                format!(
                    "pub use crate::dpdk::{prefix}{name} as {name};",
                    prefix = PREFIX,
                    name = name
                )
            })
            .join("\n");

        let mut template = File::open(template_path).unwrap();
        let mut template_string = String::new();
        template.read_to_string(&mut template_string).ok();

        let formatted_string = template_string.replace("%static_use_defs%", &static_use_string);

        let formatted_string =
            formatted_string.replace("%static_constants%", &self.static_constants);

        let formatted_string = formatted_string.replace(
            "%static_eal_functions%",
            &self
                .eal_function_use_defs
                .iter()
                .map(|item| item.replace('\n', "\n\t"))
                .join("\n"),
        );

        let formatted_string = formatted_string.replace(
            "%global_static_eal_functions%",
            &self
                .global_eal_function_use_defs
                .iter()
                .map(|item| item.replace('\n', "\n\t"))
                .join("\n"),
        );

        let mut target = File::create(target_path).unwrap();
        target.write_fmt(format_args!("{}", formatted_string)).ok();
    }

    /// Do compile.
    fn compile(&mut self) {
        let dpdk_include_path = self.include_path.as_ref().unwrap();
        let dpdk_config = self.dpdk_config.as_ref().unwrap();
        let source_path = self.out_path.join("static.c");
        let lib_path = self.library_path.as_ref().unwrap();

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
            // .flag(&format!("-L{}", lib_path.to_str().unwrap()))
            // .flag("-ldpdk")
            .compile("lib_static_wrapper.a");

        println!(
            "cargo:rustc-link-search=native={}",
            lib_path.to_str().unwrap()
        );

        let pmd_whitelist_candidate = vec![
            ("rte_net_af_packet", vec![]),
            ("rte_net_af_xdp", vec!["xdp", "bpf"]),
            ("rte_net_ark", vec![]),
            ("rte_net_avp", vec![]),
            ("rte_net_axgbe", vec![]),
            ("rte_net_bnx2x", vec!["z"]),
            ("rte_net_bnxt", vec![]),
            ("rte_net_bonding", vec![]),
            ("rte_net_cxgbe", vec![]),
            ("rte_net_e1000", vec![]),
            ("rte_net_ena", vec![]),
            ("rte_net_enetfec", vec![]),
            ("rte_net_enic", vec![]),
            ("rte_net_failsafe", vec![]),
            ("rte_net_fm10k", vec![]),
            ("rte_net_gve", vec![]),
            ("rte_net_hinic", vec![]),
            ("rte_net_hns3", vec![]),
            ("rte_net_i40e", vec![]),
            ("rte_net_ionic", vec![]),
            ("rte_net_ixgbe", vec![]),
            // ("rte_net_kni", vec![]), // Kni causes crash (Cannot init trace).
            ("rte_net_liquidio", vec![]),
            ("rte_net_memif", vec![]),
            ("rte_net_mlx4", vec!["mlx4", "ibverbs"]),
            ("rte_net_mlx5", vec!["mlx5", "ibverbs"]),
            ("rte_net_mvneta", vec!["musdk"]),
            ("rte_net_mvpp2", vec!["musdk"]),
            ("rte_net_netvsc", vec![]),
            ("rte_net_nfp", vec![]),
            ("rte_net_ngbe", vec![]),
            ("rte_net_null", vec![]),
            ("rte_net_pcap", vec!["pcap"]),
            ("rte_net_octeon_ep", vec![]),
            ("rte_net_octeontx", vec![]),
            ("rte_net_ring", vec![]),
            ("rte_net_sfc", vec!["atomic"]),
            ("rte_net_softnic", vec![]),
            ("rte_net_tap", vec![]),
            ("rte_net_thunderx", vec![]),
            ("rte_net_txgbe", vec![]),
            ("rte_net_vhost", vec![]),
            ("rte_net_virtio", vec![]),
            ("rte_net_vmxnet3", vec![]),
        ];
        let mut pmd_whitelist = Vec::new();

        let test_template = self.project_path.join("gen/link_test.c");
        let builder = cc::Build::new();
        let compiler = builder.get_compiler();
        let cc_name = compiler.path().to_str().unwrap().to_string();

        for (name, deps) in pmd_whitelist_candidate.into_iter() {
            let mut skip_due_to = Vec::new();
            for dep in &deps {
                let ret = Command::new(cc_name.clone())
                    .arg("-o")
                    .arg("/dev/null")
                    .arg(test_template.clone())
                    .arg(format!("-l{}", dep))
                    .output();
                if let Ok(ret) = ret {
                    if !ret.status.success() {
                        skip_due_to.push(dep);
                    }
                }
            }
            if !skip_due_to.is_empty() {
                println!(
                    "cargo:warning=Skip linking {} for missing deps {:?}",
                    name, skip_due_to
                );
                continue;
            }
            pmd_whitelist.push((name, deps));
        }
        let mut rte_libs: Vec<_> = Vec::new();
        let mut additional_libs: Vec<&'static str> = vec![];

        // Legacy mode: Rust cargo cannot recognize library groups (libdpdk.a).
        let lib_name_format = Regex::new(r"lib(.*)\.(a)").unwrap();
        'outer: for link in &self.dpdk_links {
            let lib_name = link.file_name().unwrap().to_str().unwrap();

            if let Some(capture) = lib_name_format.captures(lib_name) {
                let link_name = &capture[1];
                if link_name == "dpdk" {
                    continue;
                }
                for (name, deps) in pmd_whitelist.iter() {
                    if *name == link_name {
                        additional_libs.extend(deps.iter());
                        println!(
                            "cargo:rustc-link-lib=static:+whole-archive,-bundle={}",
                            link_name
                        );
                        continue 'outer;
                    }
                }
                if link_name.starts_with("rte_net_") {
                    continue;
                }
                rte_libs.push(link_name.to_string());
            }
        }
        rte_libs.sort();
        rte_libs.dedup();
        for rte_dep in rte_libs {
            println!("cargo:rustc-link-lib={}", rte_dep);
        }
        additional_libs.sort();
        additional_libs.dedup();
        for dep in additional_libs {
            println!("cargo:rustc-link-lib={}", dep);
        }
        println!("cargo:rustc-link-lib=bsd");
        println!("cargo:rustc-link-lib=numa");
    }
}

fn main() {
    let mut state = State::new();
    state.check_os();
    state.check_compiler();
    state.find_dpdk();
    state.find_link_libs();
    state.make_all_in_one_header();
    state.extract_eal_apis();
    state.generate_static_impls_and_link_pmds();
    state.generate_rust_def();
    state.generate_lib_rs();
    state.compile();
}
