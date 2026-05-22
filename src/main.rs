#![no_std]
#![no_main]

use xunil::{
    file::{SEEK_END, SEEK_SET, fclose, fopen, fread, fseek, ftell},
    keyboard::{KEY_ENTER, KeyboardEvent, kbd_read},
    print, putchar,
    syscall::{EXECVE, syscall1},
    time::sleep_ms,
};

extern crate alloc;

use alloc::{
    ffi::CString,
    string::{String, ToString},
    vec::Vec,
};

use alloc::vec;

fn run_command(input: &String) -> i32 {
    let argument_vec: Vec<&str> = input.split_ascii_whitespace().collect();
    if let Some((command, args)) = argument_vec.split_first() {
        print("\n");
        if command == &"echo" {
            print(args[0]);
        } else if command == &"read" {
            if args.is_empty() {
                print("usage: read <file>\n");
                return -1;
            }

            let file = fopen(
                CString::new(args[0]).unwrap_or_default().as_ptr() as *const i8,
                b"r\0".as_ptr() as *const i8,
            );

            if file.is_null() {
                print("fopen failed\n");
                return -1;
            }

            fseek(file, 0, SEEK_END);
            let size = ftell(file);
            if size < 0 {
                print("ftell failed\n");
                return -1;
            }
            fseek(file, 0, SEEK_SET);

            let size = size as usize;
            let mut buf = vec![0u8; size];

            let items = fread(buf.as_mut_ptr(), size, 1, file);
            if items == 0 {
                print("fread failed/empty\n");
                return -1;
            }

            let text = core::str::from_utf8(&buf[..size]).unwrap_or("<non-utf8>");
            print(text);

            fclose(file);
        } else if command == &"run" {
            unsafe {
                let _ = syscall1(
                    EXECVE,
                    CString::new(args[0]).unwrap_or_default().as_ptr() as *const u8 as isize,
                )
                .to_string()
                .as_str();
            };
        } else {
            print(input);
            print(": ");
            print("Command not found");
            return -1;
        }

        return 0;
    } else {
        print("\n");
        print(input);
        print(": ");
        print("Syntax Error");
        return -1;
    }
}

#[unsafe(no_mangle)]
extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let mut kbd_events: [KeyboardEvent; 16] = [KeyboardEvent::default(); 16];
    let mut command = String::new();
    print("Starting XunilOS...\n");
    loop {
        print("> ");

        loop {
            if kbd_events.len() != 16 {
                // make sure we dont cause memory overwrite errors because the buffer isnt 16 KeyboardEvents long
                kbd_events = [KeyboardEvent::default(); 16];
            }

            let n = unsafe { kbd_read(kbd_events.as_mut_ptr(), 16) };

            if n <= 0 {
                unsafe { sleep_ms(5) };
                continue;
            }

            let mut should_return = false;

            for i in 0..(n as usize) {
                let event = kbd_events[i];

                if event.state != 0 {
                    continue;
                }

                if event.key == KEY_ENTER as u16 {
                    run_command(&command);
                    should_return = true;
                    break;
                }

                if event.unicode != 0 {
                    command.push(event.unicode as u8 as char);
                    putchar(event.unicode as i32);

                    if command.len() >= 256 {
                        run_command(&command);
                        should_return = true;
                        break;
                    }
                }
            }

            unsafe { sleep_ms(5) };

            if should_return {
                command.clear();
                print("\n");
                break;
            }
        }
    }
}
