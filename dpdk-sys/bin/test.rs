use std::env;
use std::ffi;
use std::os::raw::*;

fn main() {
    unsafe {
        let args: Vec<String> = env::args().collect();
        let mut args: Vec<ffi::CString> = args
            .into_iter()
            .map(|x| ffi::CString::new(x).unwrap())
            .collect();
        println!("{:?}", args);
        let argc = args.len();
        let mut argv: Vec<*mut c_char> = vec![];
        for arg in &mut args {
            argv.push(arg.as_bytes_with_nul().as_ptr() as *mut c_char);
        }
        argv.push(std::ptr::null_mut());
        let ret = dpdk_sys::rte_eal_init(argc as c_int, argv.as_mut_ptr() as *mut *mut c_char);
        assert!(ret >= 0);
        println!("{:?}", dpdk_sys::pmd_list());

        assert_eq!(dpdk_sys::rte_is_power_of_2(7), 0);
        assert_eq!(dpdk_sys::rte_is_power_of_2(16), 1);
    }
}
