extern crate dpdk;
use std::env;
use std::ffi;
use std::mem;
use std::os::raw::*;

fn main() {
    unsafe {
        let args: Vec<String> = env::args().collect();
        let args = vec![
            args[0].clone(),
            String::from("--no-pci"),
            String::from("--no-huge"),
        ];
        let argc = args.len();
        let mut argv: Vec<*const c_char> = vec![];
        for arg in args {
            let arg = ffi::CString::new(arg).unwrap();
            argv.push(mem::transmute(arg.into_bytes_with_nul().as_ptr()));
        }
        argv.push(std::ptr::null());
        let ret = dpdk::rte_eal_init(argc as c_int, argv.as_mut_ptr() as *mut *mut c_char);
        assert!(ret == 0);
    }
}
