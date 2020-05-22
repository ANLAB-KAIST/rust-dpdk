use std::env;
use std::ffi;
use std::os::raw::*;

fn main() {
    unsafe {
        let args: Vec<String> = env::args().collect();
        let args: Vec<ffi::CString> = args
            .into_iter()
            .map(|x| ffi::CString::new(x).unwrap())
            .collect();
        println!("{:?}", args);
        let argc = args.len();
        let argv: Vec<_> = args
            .iter()
            .map(|arg| arg.as_bytes_with_nul().as_ptr() as *mut c_char)
            .chain(std::iter::once(std::ptr::null_mut()))
            .collect();

        let ret = dpdk_sys::rte_eal_init(argc as c_int, argv.as_ptr() as *mut *mut c_char);
        assert!(ret >= 0);
        println!("{:?}", dpdk_sys::pmd_list());

        assert_eq!(dpdk_sys::rte_is_power_of_2(7), 0);
        assert_eq!(dpdk_sys::rte_is_power_of_2(16), 1);
    }
}
