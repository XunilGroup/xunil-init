#![no_std]
#![no_main]

use xunil::{
    print,
    syscall::{EXECVE, syscall1},
};

extern crate alloc;

pub mod framebuffer;
pub mod windowing;

use alloc::ffi::CString;

use crate::{framebuffer::map_framebuffer, windowing::main_loop};

#[unsafe(no_mangle)]
extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    print("Starting XunilOS...\n");

    print("Mapping Framebuffer...\n");
    unsafe { map_framebuffer() };

    unsafe {
        let _ = syscall1(
            EXECVE,
            CString::new("badapple").unwrap_or_default().as_ptr() as *const u8 as isize,
        );
    };

    unsafe {
        let _ = syscall1(
            EXECVE,
            CString::new("doomgeneric").unwrap_or_default().as_ptr() as *const u8 as isize,
        );
    };

    unsafe {
        let _ = syscall1(
            EXECVE,
            CString::new("shell").unwrap_or_default().as_ptr() as *const u8 as isize,
        );
    };

    main_loop();
}
