extern crate dpdk;
use std::env;
use std::ffi;
use std::os::raw::*;

fn main() {
    unsafe {
        let args: Vec<String> = env::args().collect();
        let mut args = vec![
            ffi::CString::new(args[0].clone()).unwrap(),
            ffi::CString::new("--no-pci").unwrap(),
            ffi::CString::new("--no-huge").unwrap(),
        ];
        let argc = args.len();
        let mut argv: Vec<*mut c_char> = vec![];
        for arg in &mut args {
            argv.push(arg.as_bytes_with_nul().as_ptr() as *mut c_char);
        }
        argv.push(std::ptr::null_mut());
        let ret = dpdk::rte_eal_init(argc as c_int, argv.as_mut_ptr() as *mut *mut c_char);
        assert!(ret > 0);
        println!("{:?}", dpdk::pmd_list());

        assert_eq!(dpdk::rte_is_power_of_2(7), 0);
        assert_eq!(dpdk::rte_is_power_of_2(16), 1);
    }
}
