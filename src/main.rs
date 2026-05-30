#![no_std]
#![no_main]
#![feature(unboxed_closures)]

use xunil::{
    print,
    syscall::{EXECVE, syscall1},
};

extern crate alloc;

pub mod framebuffer;
pub mod input;
pub mod windowing;

use crate::{framebuffer::map_framebuffer, windowing::main_loop};

#[unsafe(no_mangle)]
extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    print("Starting XunilOS...\n");

    print("Mapping Framebuffer...\n");
    unsafe { map_framebuffer() };

    main_loop();
}
