extern crate dpdk_sys;

use std::env;
use std::ffi;
use std::os::raw::*;

fn main() {
    unsafe {
        let c_argv: Vec<_> = env::args()
            .map(ffi::CString::new)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        println!("{:?}", c_argv);
        let argv: Vec<_> = c_argv
            .iter()
            .map(|arg| arg.as_bytes_with_nul().as_ptr() as *mut c_char)
            .chain(std::iter::once(std::ptr::null_mut()))
            .collect();
        let argc = c_argv.len();
        let ret = dpdk_sys::rte_eal_init(argc as c_int, argv.as_ptr() as *mut *mut c_char);
        assert!(ret >= 0);

        assert_eq!(dpdk_sys::rte_is_power_of_2(7), 0);
        assert_eq!(dpdk_sys::rte_is_power_of_2(16), 1);
    }
}
