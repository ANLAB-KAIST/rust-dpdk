extern crate bindgen;
extern crate cc;
extern crate clang;
extern crate num_cpus;
extern crate regex;

use regex::Regex;
use std::cmp::Ordering;
use std::env;
use std::fs::*;
use std::io::*;
use std::path::*;
use std::process::Command;

#[derive(Default)]
struct State {
    project_path: Option<PathBuf>,
    out_path: Option<PathBuf>,
    system_include_path: Vec<String>,
    include_path: Option<PathBuf>,
    library_path: Option<PathBuf>,
    dpdk_headers: Vec<PathBuf>,
    dpdk_links: Vec<PathBuf>,
    dpdk_config: Option<PathBuf>,
    static_functions: Vec<String>,
}

fn check_os(_: &mut State) {
    #[cfg(not(unix))]
    panic!("Currently, only xnix OS is supported.");
}

fn check_compiler(state: &mut State) {
    let output = Command::new("bash")
        .args(&[
            "-c",
            "clang -march=native -Wp,-v -x c - -fsyntax-only < /dev/null 2>&1 | sed -e '/^#include <...>/,/^End of search/{ //!b };d'",
        ])
        .output()
        .expect("failed to extract cc include path");
    let message = String::from_utf8(output.stdout).unwrap();
    state
        .system_include_path
        .extend(message.lines().map(|x| String::from(x.trim())));
}

static STATIC_PREFIX: &str = "static_8a9f682d_";

fn find_dpdk(state: &mut State) {
    if let Ok(path_string) = env::var("RTE_SDK") {
        let mut dpdk_path = PathBuf::from(path_string);
        if let Ok(target_string) = env::var("RTE_TARGET") {
            dpdk_path = dpdk_path.join(target_string);
        } else {
            dpdk_path = dpdk_path.join("build");
        }
        state.include_path = Some(dpdk_path.join("include"));
        state.library_path = Some(dpdk_path.join("lib"));
    } else if Path::new("/usr/local/include/dpdk/rte_config.h").exists() {
        state.include_path = Some(PathBuf::from("/usr/local/include/dpdk"));
        state.library_path = Some(PathBuf::from("/usr/local/lib"));
    } else {
        // Automatic download
        let dir_path = Path::new(state.out_path.as_ref().unwrap()).join("3rdparty");
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

        state.include_path = Some(git_path.join("build").join("include"));
        state.library_path = Some(git_path.join("build").join("lib"));
    }
    assert!(state.include_path.as_ref().unwrap().exists());
    assert!(state.library_path.as_ref().unwrap().exists());
    let config_header = state.include_path.as_ref().unwrap().join("rte_config.h");
    assert!(config_header.exists());
    println!(
        "cargo:rerun-if-changed={}",
        state.include_path.as_ref().unwrap().to_str().unwrap()
    );
    println!(
        "cargo:rerun-if-changed={}",
        state.library_path.as_ref().unwrap().to_str().unwrap()
    );
    for entry in state
        .project_path
        .as_ref()
        .unwrap()
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

    state.dpdk_config = Some(config_header);
}

fn find_link_libs(state: &mut State) {
    let lib_dir = state.library_path.as_ref().unwrap();

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
    state.dpdk_links = libs;
}

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

fn make_all_in_one_header(state: &mut State) {
    let include_dir = state.include_path.as_ref().unwrap();
    let dpdk_config = state.dpdk_config.as_ref().unwrap();
    let blacklist = vec!["rte_function_versioning"];
    let mut headers = vec![];
    for entry in include_dir.read_dir().expect("read_dir failed") {
        if let Ok(entry) = entry {
            let path = entry.path();

            if !path.is_file() {
                continue;
            }
            if let Some(stem) = path.file_stem() {
                if blacklist.contains(stem) {
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

    state.dpdk_headers = headers;
    let template_path = state
        .project_path
        .as_ref()
        .unwrap()
        .join("gen/dpdk.h.template");
    let target_path = state.out_path.as_ref().unwrap().join("dpdk.h");
    let mut template = File::open(template_path).unwrap();
    let mut target = File::create(target_path).unwrap();

    let mut template_string = String::new();
    template.read_to_string(&mut template_string).ok();

    let mut headers_string = String::new();
    for header in &state.dpdk_headers {
        headers_string += &format!(
            "#include \"{}\"\n",
            header.file_name().unwrap().to_str().unwrap()
        );
    }
    let formatted_string = template_string.replace("%header_list%", &headers_string);

    target.write_fmt(format_args!("{}", formatted_string)).ok();
}

fn generate_static_impl(state: &mut State) {
    let clang = clang::Clang::new().unwrap();

    let index = clang::Index::new(&clang, true, true);

    let header_path = state.out_path.as_ref().unwrap().join("dpdk.h");

    let mut argument = vec![
        "-march=native".into(),
        format!(
            "-I{}",
            state.include_path.as_ref().unwrap().to_str().unwrap()
        ),
        //.to_string(),
        format!("-I{}", state.out_path.as_ref().unwrap().to_str().unwrap()), //.to_string(),
        "-imacros".into(),
        state
            .dpdk_config
            .as_ref()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string(),
    ];

    for path in state.system_include_path.iter() {
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
        let name_ = f.get_name();
        let storage_ = f.get_storage_class();
        let return_type_ = f.get_result_type();
        let is_decl = f.is_definition();
        if let (Some(clang::StorageClass::Static), Some(return_type), Some(name), true) =
            (storage_, return_type_, name_, is_decl)
        {
            if name.starts_with('_') {
                // Skip low level intrinsics
                continue;
            }
            //println!("cargo:warning={:?}", name);
            let mut arg_string = String::from("");
            let mut param_string = String::from("");
            let return_type_string = return_type.get_display_name();
            if let Some(args) = f.get_arguments() {
                // let mut counter = 0;
                for (counter, arg) in args.iter().enumerate() {
                    let arg_name = arg
                        .get_display_name()
                        .unwrap_or_else(|| format!("_unnamed_arg{}", counter));
                    let type_ = arg.get_type().unwrap();
                    arg_string += &format!("{}, ", format_arg(type_, arg_name.clone()));
                    param_string += &format!("{}, ", arg_name);
                    //counter += 1;
                }
                arg_string = arg_string.trim_end_matches(", ").to_string();
                param_string = param_string.trim_end_matches(", ").to_string();
            }
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
            state.static_functions.push(name.clone());
        }
    }

    let header_path = state.out_path.as_ref().unwrap().join("static.h");
    let header_template = state
        .project_path
        .as_ref()
        .unwrap()
        .join("gen/static.h.template");
    let source_path = state.out_path.as_ref().unwrap().join("static.c");
    let source_template = state
        .project_path
        .as_ref()
        .unwrap()
        .join("gen/static.c.template");

    let mut header_defs = String::new();
    let mut static_impls = String::new();
    for (def_, impl_) in Iterator::zip(static_def_list.iter(), static_impl_list.iter()) {
        header_defs += &format!("{};\n", def_);
        static_impls += &format!("{}{}\n", def_, impl_);
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
    let mut target = File::create(source_path).unwrap();
    target.write_fmt(format_args!("{}", &formatted_string)).ok();
}

fn generate_rust_def(state: &mut State) {
    let dpdk_include_path = state.include_path.as_ref().unwrap();
    let dpdk_config_path = state.dpdk_config.as_ref().unwrap();
    let out_path = state.out_path.as_ref().unwrap();

    let header_path = out_path.join("static.h");
    let target_path = out_path.join("dpdk.rs");
    bindgen::builder()
        .header(header_path.to_str().unwrap())
        .clang_arg(format!("-I{}", dpdk_include_path.to_str().unwrap()))
        .clang_arg(format!("-I{}", out_path.to_str().unwrap()))
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

fn generate_lib_rs(state: &mut State) {
    let project_path = state.project_path.as_ref().unwrap();
    let template_path = project_path.join("gen/lib.rs.template");
    let target_path = state.out_path.as_ref().unwrap().join("lib.rs");

    let format = Regex::new(r"rte_pmd_(\w+)").unwrap();

    let mut pmds = vec![];
    for link in &state.dpdk_links {
        let link_name = link.file_stem().unwrap().to_str().unwrap();
        if let Some(capture) = format.captures(link_name) {
            pmds.push(String::from(&capture[1]));
        }
    }

    let mut pmds_string = String::new();
    for pmd in pmds {
        pmds_string += &format!("\n\"{}\",", pmd);
    }

    let mut static_use_string = String::new();
    for name in &state.static_functions {
        static_use_string += &format!(
            "pub use dpdk::{prefix}{name} as {name};\n",
            prefix = STATIC_PREFIX,
            name = name
        );
    }

    let mut template = File::open(template_path).unwrap();
    let mut template_string = String::new();
    template.read_to_string(&mut template_string).ok();

    let formatted_string = template_string.replace("%pmd_list%", &pmds_string);
    let formatted_string = formatted_string.replace("%static_use_defs%", &static_use_string);

    let mut target = File::create(target_path).unwrap();
    target.write_fmt(format_args!("{}", formatted_string)).ok();
}

fn compile(state: &mut State) {
    let out_path = state.out_path.as_ref().unwrap();
    let dpdk_include_path = state.include_path.as_ref().unwrap();
    let dpdk_config = state.dpdk_config.as_ref().unwrap();
    let source_path = out_path.join("static.c");
    let lib_path = state.library_path.as_ref().unwrap();
    cc::Build::new()
        .file(source_path)
        .static_flag(true)
        .shared_flag(false)
        .opt_level(3)
        .include(dpdk_include_path)
        .include(out_path)
        .flag("-w") // hide warnings
        .flag("-march=native")
        .flag("-imacros")
        .flag(dpdk_config.to_str().unwrap())
        .compile("lib_static_wrapper.a");

    let mut link_list = String::new();
    if lib_path.join("libdpdk.a").exists() {
        link_list += " -C link-arg=-ldpdk";
    //println!("cargo:rustc-link-lib=dpdk");
    } else {
        // legacy mode
        //let format = Regex::new(r"lib(.*)\.(so)").unwrap();
        let format = Regex::new(r"lib(.*)\.(a)").unwrap();
        for link in &state.dpdk_links {
            let lib_name = link.file_name().unwrap().to_str().unwrap();

            if let Some(capture) = format.captures(lib_name) {
                let link_name = &capture[1];
                if link_name == "dpdk" {
                    continue;
                }
                link_list += &format!(" -C link-arg=-l{}", link_name);
                //println!("cargo:rustc-link-lib={}", link_name);
            }
        }
    }

    let mut additional_libs = String::new();
    let additional_libs_list = vec!["numa", "m", "c"];
    for lib in &additional_libs_list {
        //println!("cargo:rustc-link-lib={}", lib);
        additional_libs += &format!(" -C link-arg=-l{}", lib);
    }

    let expected_env = format!(
        "-C link-arg=-L{} -C link-arg=-Wl,--whole-archive{} -C link-arg=-Wl,--no-whole-archive{}",
        lib_path.to_str().unwrap(),
        &link_list,
        &additional_libs
    );

    if let Ok(env_string) = env::var("RUSTFLAGS") {
        if env_string.contains(&expected_env) {
            // return;
        } else {
            panic!(
                "RUSTFLAGS env var is different from expected. Expected: {}, Current: {}.",
                expected_env, env_string
            );
        }
    } else {
        panic!("RUSTFLAGS env var is not set. Please set via following command:\nexport RUSTFLAGS=\"{}\"", expected_env);
    }
}
fn main() {
    let mut state: State = Default::default();
    state.project_path = Some(PathBuf::from(".").canonicalize().unwrap());
    state.out_path = Some(
        PathBuf::from(env::var("OUT_DIR").unwrap())
            .canonicalize()
            .unwrap(),
    );
    check_os(&mut state);
    check_compiler(&mut state);
    find_dpdk(&mut state);
    find_link_libs(&mut state);
    make_all_in_one_header(&mut state);
    generate_static_impl(&mut state);
    generate_rust_def(&mut state);
    generate_lib_rs(&mut state);
    compile(&mut state);
}
