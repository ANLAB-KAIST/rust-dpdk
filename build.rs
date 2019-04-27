extern crate bindgen;
extern crate num_cpus;
extern crate regex;

use regex::Regex;
use std::env;
use std::fs::*;
use std::io::*;
use std::path::*;
use std::process::Command;

#[derive(Default)]
struct State {
    project_path: Option<PathBuf>,
    include_path: Option<PathBuf>,
    library_path: Option<PathBuf>,
    dpdk_headers: Vec<PathBuf>,
    dpdk_links: Vec<PathBuf>,
    dpdk_config: Option<PathBuf>,
}

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
    } else if Path::new("/usr/include/dpdk/rte_config.h").exists() {
        state.include_path = Some(PathBuf::from("/usr/include/dpdk"));
        state.library_path = Some(PathBuf::from("/usr/lib"));
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
                .args(&[
                    "clone",
                    "-b",
                    "releases",
                    "https://gitlab.kaist.ac.kr/3rdparty/dpdk",
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
    assert!(state.include_path.clone().unwrap().exists());
    assert!(state.library_path.clone().unwrap().exists());
    let config_header = state.include_path.clone().unwrap().join("rte_config.h");
    assert!(config_header.exists());
    state.dpdk_config = Some(config_header);
}

fn find_link_libs(state: &mut State) {
    let lib_dir = state.library_path.clone().unwrap();

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
        let line_str = line.ok().unwrap().trim().to_lowercase();
        if line_str.starts_with("#error") {
            if line_str.find("do not").is_some()
                && line_str.find("include").is_some()
                && line_str.find("directly").is_some()
            {
                return false;
            }
        }
    }
    return true;
}

fn make_all_in_one_header(state: &mut State) {
    let include_dir = state.include_path.clone().unwrap();
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
    assert!(headers.len() > 0);

    // Heuristically remove platform-specific headers
    let mut name_set = vec![];
    for file in &headers {
        let file_name = String::from(file.file_stem().unwrap().to_str().unwrap());
        name_set.push(file_name);
    }
    let mut new_vec = vec![];
    'outer: for file in &headers {
        let file_name = file.file_stem().unwrap().to_str().unwrap();
        println!("{}", file_name);
        for prev_name in &name_set {
            if file_name.starts_with(&format!("{}_", prev_name)) {
                continue 'outer;
            }
        }
        new_vec.push(file.clone());
    }

    new_vec.sort();
    headers = new_vec;

    state.dpdk_headers = headers;
    let template_path = state.project_path.clone().unwrap().join("dpdk.h.template");
    let target_path = state.project_path.clone().unwrap().join("dpdk.h");
    let mut template = File::open(template_path).unwrap();
    let mut target = File::create(target_path).unwrap();

    let mut template_string = String::new();
    template.read_to_string(&mut template_string).ok();

    let mut headers_string = String::new();
    for header in &state.dpdk_headers {
        headers_string += &format!(
            "#include <{}>\n",
            header.clone().file_name().unwrap().to_str().unwrap()
        );
    }
    let formatted_string = template_string.replace("%header_list%", &headers_string);

    target.write_fmt(format_args!("{}", formatted_string)).ok();
}

fn generate_rust_def(state: &mut State) {
    let dpdk_include_path = state.include_path.clone().unwrap();
    let c_include_path = state.project_path.clone().unwrap().join("c_header");
    let dpdk_config_path = state.dpdk_config.clone().unwrap();

    let header_path = state.project_path.clone().unwrap().join("dpdk.h");
    let target_path = state
        .project_path
        .clone()
        .unwrap()
        .join("src")
        .join("dpdk.rs");
    bindgen::builder()
        .header(header_path.to_str().unwrap())
        .clang_arg(format!("-I{}", dpdk_include_path.to_str().unwrap()))
        .clang_arg(format!("-I{}", c_include_path.to_str().unwrap()))
        .clang_arg("-imacros")
        .clang_arg(dpdk_config_path.to_str().unwrap())
        .clang_arg("-march=native")
        .clang_arg("-Wno-everything")
        .rustfmt_bindings(true)
        .generate()
        .unwrap()
        .write_to_file(target_path)
        .ok();
}

fn generate_lib_rs(state: &mut State) {
    let template_path = state.project_path.clone().unwrap().join("lib.rs.template");
    let target_path = state
        .project_path
        .clone()
        .unwrap()
        .join("src")
        .join("lib.rs");

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

    let mut template = File::open(template_path).unwrap();
    let mut template_string = String::new();
    template.read_to_string(&mut template_string).ok();

    let formatted_string = template_string.replace("%pmd_list%", &pmds_string);
    let mut target = File::create(target_path).unwrap();
    target.write_fmt(format_args!("{}", formatted_string)).ok();
}

fn compile(state: &mut State) {
    let lib_path = state.library_path.clone().unwrap();
    let dpdk_include_path = state.include_path.clone().unwrap();
    let dpdk_config = state.dpdk_config.clone().unwrap();
    let project_path = state.project_path.clone().unwrap();
    println!(
        "cargo:rustc-link-search=native={}",
        lib_path.to_str().unwrap()
    );
    let format = Regex::new(r"lib(.*)\.(a|so)").unwrap();
    for link in &state.dpdk_links {
        let link_name = link.file_name().unwrap().to_str().unwrap();
        if let Some(capture) = format.captures(link_name) {
            println!("cargo:rustc-link-lib={}", &capture[1]);
        }
    }
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
