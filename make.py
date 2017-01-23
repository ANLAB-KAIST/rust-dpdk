import subprocess
import os
import logging
from pathlib import Path
import re


def check_direct_include(path: Path):
    if not path.is_file():
        return False
    with path.open("r") as f:
        for line in f:
            line = line.strip().lower()
            if line.startswith("#error"):
                if line.find("do not") >= 0 and \
                                line.find("include") >= 0 and \
                                line.find("directly") >= 0:
                    return False
        return True


class State:
    def __init__(self):
        self.dpdk_path = None
        self.dpdk_headers = None
        self.dpdk_links = None
        self.dpdk_config = None
        self.phase = 1

    def find_dpdk(self):
        if "RTE_SDK" in os.environ:
            path = Path(os.environ["RTE_SDK"])
        else:
            logging.info("RTE_SDK environment variable is not found")
            path = Path(input("{}. Enter DPDK path (install directory): ".format(self.phase)).strip())
            self.phase += 1

        if not path.exists() and not path.is_dir():
            logging.error("Path {} does not exist".format(path))
            return False

        path = path.absolute()
        config_header = path.joinpath("include","rte_config.h")
        if not config_header.exists():
            logging.error("Cannot find rte_config.h")
            return False

        self.dpdk_path = path
        self.dpdk_config = config_header

        logging.info("DPDK is found at {}, config file is {}".format(self.dpdk_path, self.dpdk_config))

        return True

    def find_link_libs(self):
        path = self.dpdk_path
        lib_dir = path.joinpath("lib")
        libs = set()

        for item in lib_dir.iterdir():
            if not item.is_file():
                continue
            if item.suffix != ".a":
                continue
            if not item.name.startswith("librte_"):
                continue
            libs.add(item)

        self.dpdk_links = libs

        return True

    def make_all_in_one_header(self):
        path = self.dpdk_path
        include_dir = path.joinpath("include")
        headers = set()

        for item in include_dir.iterdir():
            if not item.is_file():
                continue
            if item.suffix != ".h":
                continue
            if item == self.dpdk_config:
                continue
            if not check_direct_include(item):
                continue
            headers.add(item)

        with open("dpdk.h", "w") as f:
            for header in headers:
                f.write("#include <{}>\n".format(header.name))

        return True

    def generate_rust_def(self):
        rust_src_path = Path("src").joinpath("dpdk.rs")
        dpdk_include_path = self.dpdk_path.joinpath("include")
        try:
            subprocess.check_output(["bindgen", "dpdk.h", "--output", str(rust_src_path),
                                     "--no-unstable-rust",
                                     "--", "-I{}".format(dpdk_include_path), "-imacros", str(self.dpdk_config),
                                     "-march=native"])
            return True
        except OSError:
            logging.error("Cannot execute bindgen program")
        except subprocess.CalledProcessError:
            logging.error("bindgen did not exit correctly")
        return False

    def generate_build_rs(self):
        rust_build_rs = Path("build.rs")
        rust_build_rs_template = Path("build.rs.template")

        with rust_build_rs_template.open("r") as template:
            template_string = template.read()
            format = re.compile(r"lib(.*)\.a")

            link_list = ""
            for link in self.dpdk_links:
                result = format.match(link.name)
                if result is not None:
                    link_list += "\n\"{}\",".format(result.group(1))
            formatted = template_string.replace("%link_list%", link_list)
            with rust_build_rs.open("w") as f:
                f.write(formatted)


def main():
    state = State()
    if not state.find_dpdk():
        return
    if not state.find_link_libs():
        return
    if not state.make_all_in_one_header():
        return
    if not state.generate_rust_def():
        return
    if not state.generate_build_rs():
        return
    pass

if __name__ == "__main__":
    main()