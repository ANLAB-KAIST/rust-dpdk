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
use std::fs::*;
use std::io::*;
use std::path::*;
use std::process::Command;

/// We make additional wrapper functions for existing bindings.
/// To avoid collision, we add a magic prefix for each.
static PREFIX: &str = "prefix_8a9f682d_";

/// Convert `/**` comments into `///` comments
fn strip_comments(comment: String) -> String {
    comment
        .split('\n')
        .map(|line| {
            line.trim_matches(|c| c == ' ' || c == '/' || c == '*')
                .replace("\t", "    ")
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
    dpdk_headers: Vec<PathBuf>,

    /// List of DPDK lib files.
    dpdk_links: Vec<PathBuf>,

    /// DPDK config file (will be included as a predefined macro file).
    dpdk_config: Option<PathBuf>,

    /// Use definitions for automatically found EAL APIs.
    eal_function_use_defs: Vec<String>,

    /// Names of `static inline` functions found in DPDK headers.
    static_functions: Vec<String>,

    /// Names of linkable (non-static) PMD-specific functions. We use them to create explicit
    /// symbolic dependencies to PMDs.
    ///
    /// Currently, DPDK's conditional build is incomplete. For example, declaration of
    /// `rte_pmd_ixgbe_bypass_wd_reset` is controlled by `RTE_LIBRTE_IXGBE_BYPASS`, but its
    /// definition is not.  Thus, we fallback to use explicit whitelist rather than automatically
    /// detect non-static symbols.
    linkable_pmd_functions: Vec<String>,
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
            static_functions: Default::default(),
            linkable_pmd_functions: Default::default(),
        }
    }

    /// Create clang trans unit from given header file.
    /// This function will fill options from current `State`.
    fn trans_unit_from_header<'a>(
        &self,
        index: &'a clang::Index,
        header_path: PathBuf,
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
    /// After 20.11 update, build system is integrated into meson.
    /// Thus, it is difficult to obtain build path manually.
    /// Currently, one must install DPDK to one's system.
    /// This function validates whether DPDK is installed.
    fn find_dpdk(&mut self) {
        // To find correct lib path of this platform.
        let output = Command::new("cc")
            .args(&["-dumpmachine"])
            .output()
            .expect("failed obtain current machine");
        let machine_string = String::from(String::from_utf8(output.stdout).unwrap().trim());
        let config_header = PathBuf::from("/usr/local/include/rte_config.h");
        if config_header.exists() {
            self.include_path = Some(PathBuf::from("/usr/local/include"));
            self.library_path = Some(PathBuf::from(format!("/usr/local/lib/{}", machine_string)));
        } else {
            panic!("DPDK is not installed on your system! (Cannot find /usr/local/include/dpdk/rte_config.h)")
        }
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
        let blacklist = vec!["rte_pmd_dlb", "rte_pmd_dlb2"];
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
                headers.push(path);
            } else {
                continue;
            }
        }
        headers.sort();
        headers.dedup();
        assert!(!headers.is_empty());

        // Heuristically remove platform-specific headers
        let platform_set = vec!["x86", "x86_64", "x64", "arm", "arm32", "arm64", "amd64"];
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
            for platform in &platform_set {
                if file_name.ends_with(&format!("_{}", platform)) {
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
                Ordering::Equal => left_str.cmp(right_str),
                Ordering::Less => Ordering::Less,
                Ordering::Greater => Ordering::Greater,
            }
        });
        new_vec.dedup();
        headers = new_vec;

        // Generate all-in-one dpdk header (`dpdk.h`).
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
        let headers_whitelist = vec![
            // From librte_eal/include/generic
            "rte_atomic.h",
            "rte_byteorder.h",
            "rte_cpuflags.h",
            "rte_cycles.h",
            "rte_io.h",
            "rte_mcslock.h",
            "rte_memcpy.h",
            "rte_pause.h",
            "rte_prefetch.h",
            "rte_rwlock.h",
            "rte_spinlock.h",
            "rte_ticketlock.h",
            "rte_vect.h",
            // From librte_eal/include
            // "rte_alarm.h",
            // "rte_bitmap.h",
            // "rte_branch_prediction.h",
            // "rte_bus.h",
            // "rte_class.h",
            "rte_common.h",
            // "rte_compat.h",
            // "rte_debug.h",
            // "rte_dev.h",
            // "rte_devargs.h",
            // "rte_eal_interrupts.h",
            // "rte_eal_memconfig.h",
            // "rte_eal.h",
            // "rte_errno.h",
            // "rte_fbarray.h",
            // "rte_function_versioning.h",
            // "rte_hexdump.h",
            // "rte_hypervisor.h",
            // "rte_interrupts.h",
            // "rte_keepalive.h",
            // "rte_launch.h",
            // "rte_lcore.h",
            // "rte_log.h",
            // "rte_malloc.h",
            // "rte_memory.h",
            // "rte_memzone.h",
            // "rte_option.h",
            // "rte_pci_dev_feature_defs.h",
            // "rte_pci_dev_features.h",
            // "rte_per_lcore.h",
            "rte_random.h",
            // "rte_reciprocal.h",
            // "rte_service_component.h",
            // "rte_service.h",
            // "rte_string_fns.h",
            // "rte_tailq.h",
            // "rte_test.h",
            "rte_time.h",
            "rte_uuid.h",
            "rte_version.h",
            // "rte_vfio.h",
        ];

        // Set of function definition strings (Rust), coupled with function names.
        // This will prevent duplicated function definitions.
        let mut use_def_map = HashMap::new();

        for header_name in &headers_whitelist {
            let header_path = self.include_path.as_ref().unwrap().join(header_name);
            if !header_path.exists() {
                // In case where our whitelist is outdated.
                println!("cargo:warning=EAL header whitelist is outdated. Contact maintainers.");
                continue;
            }
            let clang = clang::Clang::new().unwrap();
            let index = clang::Index::new(&clang, true, true);
            let trans_unit = self.trans_unit_from_header(&index, header_path);

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
                let is_decl = f.is_definition();
                let comment = strip_comments(some_or!(f.get_comment(), continue));
                if use_def_map.contains_key(&name) {
                    // Skip duplicate
                    continue;
                }
                if name.starts_with('_') {
                    // Skip hidden implementations
                    continue;
                }
                if !(storage == clang::StorageClass::None && !is_decl
                    || is_decl && storage == clang::StorageClass::Static)
                {
                    // We only accept if a function definition is found, or a `static inline`
                    // function declaration is found.
                    continue;
                }

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
            }
        }
        self.eal_function_use_defs = use_def_map.values().cloned().collect();
    }

    /// Generate wrappers for static functions and create explicit links for PMDs.
    fn generate_static_impls_and_link_pmds(&mut self) {
        let header_path = self.out_path.join("dpdk.h");
        let clang = clang::Clang::new().unwrap();
        let index = clang::Index::new(&clang, true, true);
        let trans_unit = self.trans_unit_from_header(&index, header_path);

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
                    return format!("{} {}", type_.get_display_name(), name);
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
            let is_decl = f.is_definition();

            if storage == clang::StorageClass::None && !is_decl && name.starts_with("rte_pmd_") {
                // non-static function definition for a PMD is found.
                self.linkable_pmd_functions.push(name);
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

        let header_path = self.out_path.join("static.h");
        let header_template = self.project_path.join("gen/static.h.template");
        let source_path = self.out_path.join("static.c");
        let source_template = self.project_path.join("gen/static.c.template");

        let header_defs = static_def_list
            .iter()
            .map(|def_| format!("{};", def_))
            .join("\n");
        let static_impls = Iterator::zip(static_def_list.iter(), static_impl_list.iter())
            .map(|(def_, decl_)| format!("{}{}", def_, decl_))
            .join("\n");

        // List of manually enabled DPDK PMDs
        let mut linkable_whitelist: Vec<_> = vec![
            "rte_pmd_ixgbe_set_all_queues_drop_en", // ixgbe
            "rte_pmd_i40e_ping_vfs",                // i40e
            "e1000_igb_init_log",                   // e1000
            "ice_release_vsi",                      // ice
            "vmxnet3_dev_tx_queue_release",         // vmxnet3
            "virtio_dev_pause",                     // virtio
            "softnic_thread_free",                  // softnic
            // "ipn3ke_hw_tm_init", // ipn3ke (currently not enabled)
            // "mlx4_fd_set_non_blocking", // mlx4 (currently not enabled)
            // "mlx5_set_cksum_table", // mlx5 (currently not enabled)
            "iavf_prep_pkts",                    // iavf
            "fm10k_get_pcie_msix_count_generic", // fm10k
        ]
        .iter()
        .map(|name| (*name).to_string())
        .collect();

        // List of non-static PMD-specific functions used to create symbolic dependencies to PMDs.
        let mut linkable_extern_def_list: Vec<_> = vec![
            "void e1000_igb_init_log(void)",                           // e1000
            "int ice_release_vsi(struct ice_vsi *vsi)",                // ice
            "void vmxnet3_dev_tx_queue_release(void *txq)",            // vmxnet3
            "int virtio_dev_pause(struct rte_eth_dev *dev)",           // virtio
            "void softnic_thread_free(struct pmd_internals *softnic)", // softnic
            // "int ipn3ke_hw_tm_init(struct ipn3ke_hw *hw)", // ipn3ke (currently not enabled)
            // "int mlx4_fd_set_non_blocking(int fd)", // mlx4 (currently not enabled)
            // "void mlx5_set_cksum_table(void)", // mlx5 (currently not enabled)
            "uint16_t iavf_prep_pkts(void *tx_queue, struct rte_mbuf **tx_pkts, uint16_t nb_pkts)", // iavf
            "uint16_t fm10k_get_pcie_msix_count_generic(struct fm10k_hw *hw)", // fm10k
        ]
        .iter()
        .map(|name| (*name).to_string())
        .collect();

        // If non-default net drivers are enabled (ex. MLX5), add their PMD to the list.
        for link in &self.dpdk_links {
            let libname = link.file_name().unwrap().to_str().unwrap();

            if libname == "librte_pmd_mlx5.a" {
                linkable_whitelist.push("mlx5_set_cksum_table".to_string());
                linkable_extern_def_list.push("void mlx5_set_cksum_table(void)".to_string());
                break;
            }
        }

        // Currently, we use whitelist instead of extracted function list from DPDK library.  See
        // `linkable_pmd_functions` field of `State` for more information.
        self.linkable_pmd_functions = linkable_whitelist;

        // Create `extern` definition for each symbol.
        let linkable_extern_defs = linkable_extern_def_list
            .iter()
            .map(|name| format!("extern {name};", name = name))
            .join("\n");

        // Create explicit symbolic links to PMDs from `rust-dpdk-sys` rust library.  We will
        // normalize each function symbol to return its address.
        let perlist_links = self
            .linkable_pmd_functions
            .iter()
            .map(|name| {
                format!(
                    "void* {prefix}{name}() {{\n\treturn {name};\n}}",
                    prefix = PREFIX,
                    name = name
                )
            })
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
        let formatted_string =
            formatted_string.replace("%linkable_extern_defs%", &linkable_extern_defs);
        let formatted_string = formatted_string.replace("%explicit_pmd_links%", &perlist_links);
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
            .opaque_type("rte_avp.*")
            .opaque_type("vmbus.*")
            .blocklist_type("rte_arp_hdr")
            .blocklist_type("rte_arp_ipv4")
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
                    "pub use dpdk::{prefix}{name} as {name};",
                    prefix = PREFIX,
                    name = name
                )
            })
            .join("\n");

        let explicit_use_string = self
            .linkable_pmd_functions
            .iter()
            .map(|name| {
                format!(
                    "\tpub fn {prefix}{name}() -> *mut ::std::os::raw::c_void;",
                    prefix = PREFIX,
                    name = name
                )
            })
            .join("\n");
        let explicit_invoke_string = self
            .linkable_pmd_functions
            .iter()
            .map(|name| format!("\t\t{prefix}{name}();", prefix = PREFIX, name = name))
            .join("\n");

        let mut template = File::open(template_path).unwrap();
        let mut template_string = String::new();
        template.read_to_string(&mut template_string).ok();

        let formatted_string = template_string.replace("%static_use_defs%", &static_use_string);
        let formatted_string =
            formatted_string.replace("%explicit_use_defs%", &explicit_use_string);
        let formatted_string =
            formatted_string.replace("%explicit_invokes%", &explicit_invoke_string);
        let formatted_string = formatted_string.replace(
            "%static_eal_functions%",
            &self
                .eal_function_use_defs
                .iter()
                .map(|item| item.replace("\n", "\n\t"))
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
            .flag(&format!("-L{}", lib_path.to_str().unwrap()))
            .flag("-ldpdk")
            .compile("lib_static_wrapper.a");

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
                } else if link_name == "rte_pmd_mlx5" {
                    // MLX5 PMD requires additional liniking of two libraries
                    println!("cargo:rustc-link-lib=ibverbs");
                    println!("cargo:rustc-link-lib=mlx5");
                }
                println!("cargo:rustc-link-lib=static={}", link_name);
            }
        }
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
