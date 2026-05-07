#![no_std]
#![no_main]

use xunil::{
    keyboard::{KeyboardEvent, RETURN, kbd_read},
    mem::{malloc, memset},
    putchar,
    time::sleep_ms,
};

fn run_command(command: *mut u32, n: usize) {}

#[unsafe(no_mangle)]
extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
    let kbd_events_buf: *mut KeyboardEvent =
        malloc(16 * size_of::<KeyboardEvent>() as u64) as *mut KeyboardEvent;
    let command = malloc(256 * size_of::<u32>() as u64) as *mut u32;
    unsafe {
        loop {
            xunil::printf(b"\n> \0".as_ptr());

            loop {
                let n = kbd_read(kbd_events_buf, 16);

                if n <= 0 {
                    sleep_ms(5);
                    continue;
                }

                let mut should_return = false;
                let mut command_n: usize = 0;

                for i in 0..(n as usize) {
                    let event = kbd_events_buf.add(i);

                    if (*event).key == RETURN && (*event).state == 1 {
                        run_command(command, command_n);
                        should_return = true;
                        break;
                    }

                    if (*event).unicode != 0 {
                        *command.add(command_n) = (*event).unicode;
                        putchar((*event).unicode as i32);
                        command_n += 1;

                        if command_n >= 256 {
                            run_command(command, command_n);
                            should_return = true;
                            break;
                        }
                    }
                }

                sleep_ms(5);

                if should_return {
                    memset(
                        kbd_events_buf as *mut u8,
                        0,
                        16 * size_of::<KeyboardEvent>(),
                    );
                    memset(command as *mut u8, 0, 256 * size_of::<u32>());
                    break;
                }
            }
        }
    }
}
