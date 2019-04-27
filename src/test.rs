extern crate dpdk;
use std::os::raw::*;
use std::ffi;
use std::mem;

fn main() {
    unsafe {
        
		dpdk::rte_set_log_level(dpdk::RTE_LOG_DEBUG);

		let args = Vec::from_iter(env::args().into_iter());
        let args = vec![args[0], "--no-pci", "--no-huge"];
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