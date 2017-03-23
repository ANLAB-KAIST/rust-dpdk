extern crate gcc;
//extern crate bindgen;
extern crate regex;

use std::path::*;
use std::env;
use std::process::Command;
use std::fs::*;
use std::io::*;
use regex::Regex;

#[derive(Default)]
struct State {
    project_path: Option<PathBuf>,
    dpdk_path: Option<PathBuf>,
    dpdk_headers: Vec<PathBuf>,
    dpdk_links: Vec<PathBuf>,
    dpdk_config: Option<PathBuf>,
}

fn find_dpdk(state: &mut State) {
    if let Ok(path_string) = env::var("RTE_SDK") {
        state.dpdk_path = Some(PathBuf::from(&path_string));
    } else {
        // Automatic download
        let dir_path = Path::new("3rdparty");
        if !dir_path.exists() {
            create_dir(dir_path).ok();
        }
        assert!(dir_path.exists());
        let git_path = dir_path.join("dpdk");
        if !git_path.exists() {
            Command::new("git")
                .args(&["clone",
                        "-b",
                        "anlab",
                        "https://github.com/ANLAB-KAIST/dpdk",
                        git_path.to_str().unwrap()])
                .output()
                .expect("failed to run git command");
        }
        Command::new("make")
            .args(&["-C", git_path.to_str().unwrap(), "config", "T=anlab_shared"])
            .output()
            .expect("failed to run make command");
        Command::new("make")
            .args(&["-C", git_path.to_str().unwrap(), "-j", "T=anlab_shared"])
            .output()
            .expect("failed to run make command");

        state.dpdk_path = Some(git_path.join("build"));
    }
    assert!(state.dpdk_path
                .clone()
                .unwrap()
                .exists());
    let config_header = state.dpdk_path
        .clone()
        .unwrap()
        .join("include")
        .join("rte_config.h");
    assert!(config_header.exists());
    state.dpdk_config = Some(config_header);
}

fn find_link_libs(state: &mut State) {
    let dpdk_path = state.dpdk_path.clone().unwrap();
    let lib_dir = dpdk_path.join("lib");

    let mut libs = vec![];
    for entry in lib_dir.read_dir().expect("read_dir failed") {
        if let Ok(entry) = entry {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            if let Some(ext) = path.extension() {
                if ext != "a" && ext != "so" {
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
    libs.sort();
    state.dpdk_links = libs;
}

fn check_direct_include(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let file = File::open(path).unwrap();
    let reader = BufReader::new(&file);
    for (_, line) in reader.lines().enumerate() {
        let line_str = line.ok()
            .unwrap()
            .trim()
            .to_lowercase();
        if line_str.starts_with("#error") {
            if line_str.find("do not").is_some() && line_str.find("include").is_some() &&
               line_str.find("directly").is_some() {
                return false;
            }
        }
    }
    return true;
}

fn make_all_in_one_header(state: &mut State) {
    let include_dir = state.dpdk_path
        .clone()
        .unwrap()
        .join("include");
    let dpdk_config = state.dpdk_config.clone().unwrap();
    let mut headers = vec![];
    for entry in include_dir.read_dir().expect("read_dir failed") {
        if let Ok(entry) = entry {

            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            if let Some(ext) = path.extension() {
                if ext != "h" {
                    continue;
                }
            } else {
                continue;
            }
            if path == dpdk_config {
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
    state.dpdk_headers = headers;
    let template_path = state.project_path
        .clone()
        .unwrap()
        .join("dpdk.h.template");
    let target_path = state.project_path
        .clone()
        .unwrap()
        .join("dpdk.h");
    let mut template = File::open(template_path).unwrap();
    let mut target = File::create(target_path).unwrap();

    let mut template_string = String::new();
    template.read_to_string(&mut template_string).ok();

    let mut headers_string = String::new();
    for header in &state.dpdk_headers {
        headers_string += &format!("#include <{}>\n",
                                   header.clone()
                                       .file_name()
                                       .unwrap()
                                       .to_str()
                                       .unwrap());
    }
    let formatted_string = template_string.replace("%header_list%", &headers_string);

    target.write_fmt(format_args!("{}", formatted_string)).ok();
}

fn generate_rust_def(state: &mut State) {
    let dpdk_include_path = state.dpdk_path
        .clone()
        .unwrap()
        .join("include");
    let c_include_path = state.project_path
        .clone()
        .unwrap()
        .join("c_header");
    let dpdk_config_path = state.dpdk_config.clone().unwrap();

    let header_path = state.project_path
        .clone()
        .unwrap()
        .join("dpdk.h");
    let target_path = state.project_path
        .clone()
        .unwrap()
        .join("src")
        .join("dpdk.rs");
    /*
    bindgen::builder()
        .header(header_path.to_str().unwrap())
        .no_unstable_rust()
        .clang_arg(format!("-I{}", dpdk_include_path.to_str().unwrap()))
        .clang_arg(format!("-I{}", c_include_path.to_str().unwrap()))
        .clang_arg("-imacros")
        .clang_arg(dpdk_config_path.to_str().unwrap())
        .clang_arg("-march=native")
        .generate()
        .unwrap()
        .write_to_file(target_path)
        .ok();
        */

    //XXX replace with native bindgen package later
    Command::new("bindgen")
        .args(&[header_path.to_str().unwrap(),
                "--output",
                target_path.to_str().unwrap(),
                "--no-unstable-rust",
                "--",
                &format!("-I{}", dpdk_include_path.to_str().unwrap()),
                &format!("-I{}", c_include_path.to_str().unwrap()),
                "-imacros",
                dpdk_config_path.to_str().unwrap(),
                "-march=native"])
        .output()
        .expect("failed to run bindgen command");
}

fn generate_lib_rs(state: &mut State) {
    let template_path = state.project_path
        .clone()
        .unwrap()
        .join("lib.rs.template");
    let target_path = state.project_path
        .clone()
        .unwrap()
        .join("src")
        .join("lib.rs");


    let format = Regex::new(r"rte_pmd_(\w+)").unwrap();

    let mut pmds = vec![];
    for link in &state.dpdk_links {
        let link_name = link.file_stem()
            .unwrap()
            .to_str()
            .unwrap();
        if let Some(capture) = format.captures(link_name) {
            pmds.push(String::from(&capture[1]));
        }
    }

    let mut pmds_string = String::new();
    for pmd in pmds {
        pmds_string += &format!("\n\"{}\",", pmd);
    }

    let mut template = File::open(template_path).unwrap();
    let mut template_string = String::new();
    template.read_to_string(&mut template_string).ok();

    let formatted_string = template_string.replace("%pmd_list%", &pmds_string);
    let mut target = File::create(target_path).unwrap();
    target.write_fmt(format_args!("{}", formatted_string)).ok();
}

fn compile(state: &mut State) {

    let dpdk_path = state.dpdk_path.clone().unwrap();
    let dpdk_config = state.dpdk_config.clone().unwrap();
    let project_path = state.project_path.clone().unwrap();
    let lib_path = dpdk_path.join("lib");
    println!("cargo:rustc-link-search=native={}",
             lib_path.to_str().unwrap());
    for link in &state.dpdk_links {
        println!("cargo:rustc-link-lib={}",
                 link.file_stem()
                     .unwrap()
                     .to_str()
                     .unwrap());
    }
    let dpdk_include_path = dpdk_path.join("include");
    let c_include_path = project_path.join("c_header");
    let c_source_path = project_path.join("c_source");
    gcc::Config::new()
        .file(c_source_path.join("inline_wrapper.c"))
        .include(&dpdk_include_path)
        .include(&c_include_path)
        .include(&project_path)
        .flag("-march=native")
        .flag("-imacros")
        .flag(dpdk_config.to_str().unwrap())
        .compile("lib_c_inline_wrapper.a");

    gcc::Config::new()
        .file(c_source_path.join("macro_wrapper.c"))
        .include(&c_include_path)
        .flag("-march=native")
        .compile("lib_c_macro_wrapper.a");
}
fn main() {
    let mut state: State = Default::default();
    state.project_path = Some(PathBuf::from("."));
    find_dpdk(&mut state);
    find_link_libs(&mut state);
    make_all_in_one_header(&mut state);
    generate_rust_def(&mut state);
    generate_lib_rs(&mut state);
    compile(&mut state);
}
