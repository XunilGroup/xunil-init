use crate::framebuffer::{
    USER_FB_BASE, UserFrameBuffer, draw_window, get_framebuffer_size, rectangle_filled,
};
use alloc::{
    collections::btree_map::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::Write;
use spin::mutex::Mutex;

use xunil::{
    graphics::rgb,
    io::{
        input::{KeyboardEvent, MOUSE, input_read},
        ipc::{Permissions, create_port_rust, manage_port_rust, read_port_rust, write_port_rust},
        time::sleep_ms,
    },
    mem::malloc,
    print, println,
    shm::{SHM_SLOT_SIZE, USER_SHM_BASE, shm_open_rust},
    syscall::{FRAMEBUFFER_SWAP, syscall0},
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn framebuffer_swap() -> i32 {
    unsafe {
        return syscall0(FRAMEBUFFER_SWAP) as i32;
    }
}

pub static WINDOWS: Mutex<Option<BTreeMap<u64, CompositorWindow>>> = Mutex::new(None);

pub struct CompositorWindow {
    pub width: usize,
    pub height: usize,
    pub x: usize,
    pub y: usize,
    pub shm_name: String,
    pub shm_id: u64,
    pub shm_size: u64,
    pub dirty: bool,
}

pub struct CompositorMouse {
    pub x: usize,
    pub y: usize,
    pub dragged_window: Option<u64>,
}

unsafe impl Send for CompositorWindow {}

static CURSOR_BYTES: &[u8] = include_bytes!("../../../assets/cursors/default.bmp");
const BMP_HEADER_SIZE: usize = 138;
const CURSOR_W: usize = 24;
const CURSOR_H: usize = 24;

pub fn mouse_draw(mouse_x: usize, mouse_y: usize, fb: &mut UserFrameBuffer) {
    let pixels = &CURSOR_BYTES[BMP_HEADER_SIZE..]; // remove header

    for row in 0..CURSOR_H {
        let src_row = (CURSOR_H - 1 - row) * CURSOR_W * 4;
        for col in 0..CURSOR_W {
            let i = src_row + col * 4; // 4 because rgba

            let b = pixels[i];
            let g = pixels[i + 1];
            let r = pixels[i + 2];
            let a = pixels[i + 3];

            if a < 255 {
                continue;
            }

            let color = rgb(r, g, b);

            fb.put_pixel(mouse_x + col, mouse_y + row, color);
        }
    }
}

fn request_priv_ipc(sender: u64) -> String {
    let ipc_name = format!("wm_priv_{}", sender);
    create_port_rust(ipc_name.clone(), Permissions::empty());
    manage_port_rust(
        ipc_name.clone(),
        sender,
        Permissions::READ | Permissions::WRITE,
    );
    write_port_rust(
        ipc_name.clone(),
        format!("ack_request_priv_ipc {} wm_priv_{}", sender, sender),
    );

    return ipc_name;
}

pub fn fnv1a_hex(data: String) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;

    for b in data.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }

    let mut out = String::new();
    write!(&mut out, "{:016x}", hash).unwrap();
    out
}

fn request_window_buf(sender: u64, private_ipc_name: String, arguments: Vec<&str>) {
    let mut windows_opt = WINDOWS.lock();
    let windows = windows_opt.as_mut().expect("Could not get Windows");

    let requested_size = (
        arguments[1].parse::<usize>().unwrap_or(0),
        arguments[2].parse::<usize>().unwrap_or(0),
    );

    if requested_size.0 == 0 || requested_size.1 == 0 {
        print("Could not parse window size");
        return;
    }

    let (width, height, x, y) = (requested_size.0, requested_size.1, 0, windows.len() * 100);

    let shm_name = fnv1a_hex(format!(
        "{} {} {} {} {} {}",
        width, height, x, y, private_ipc_name, sender
    ));

    let shm_id = shm_open_rust(
        shm_name.clone(),
        (width * height * size_of::<u32>() + 1) as u64,
    );

    let shm_size = (width * height * size_of::<u32>()) as u64;

    if shm_id == -1 {
        print("Could not get SHM");
        return;
    }

    let window = CompositorWindow {
        width,
        height,
        x,
        y,
        shm_name: shm_name.clone(),
        shm_id: shm_id as u64,
        shm_size,
        dirty: true, // make sure even if program doesn't set dirty, atleast we have a movable window
    };

    windows.insert(sender, window);

    println!(
        "ack_request_window_buf {} {} {} {} {} {}",
        width, height, x, y, shm_name, shm_id
    );

    write_port_rust(
        private_ipc_name,
        format!(
            "ack_request_window_buf {} {} {} {} {} {}",
            width, height, x, y, shm_name, shm_id
        ),
    );
}

fn process_messages(private_ipcs: &mut Vec<(u64, String)>) {
    if let Some((sender, message)) = read_port_rust("window_manager".to_string(), -1) {
        if message.starts_with("request_priv_ipc") {
            // TODO: check if it already exists
            private_ipcs.push((sender, request_priv_ipc(sender)));
        }
    }

    for (pid, ipc_name) in private_ipcs {
        if let Some((_, message)) = read_port_rust(ipc_name.clone(), pid.clone() as i64) {
            // using pid is safer than trusting read_port_rust
            if message.starts_with("request_window_buf") {
                request_window_buf(
                    pid.clone(),
                    ipc_name.clone(),
                    message.split_whitespace().collect::<Vec<&str>>(),
                );
            } else if message.starts_with("set_dirty") {
                let mut windows_opt = WINDOWS.lock();
                let windows = windows_opt.as_mut().expect("Could not lock WINDOWS");

                if let Some(window) = windows.get_mut(pid) {
                    window.dirty = true;
                }
            }
        }
    }
}

fn draw_windows() {
    let mut windows_opt = WINDOWS.lock();
    let windows = windows_opt.as_mut().expect("Could not get Windows");

    for (_, window) in windows.iter_mut() {
        if !window.dirty {
            continue;
        }

        unsafe {
            rectangle_filled(window.x, window.y, window.width, 10, rgb(128, 128, 128)); // top
            rectangle_filled(
                window.x,
                window.y + window.height + 10,
                window.width,
                2,
                rgb(0, 255, 0),
            ); // bottom

            rectangle_filled(window.x, window.y, 2, window.height + 10, rgb(0, 255, 0)); // left
            rectangle_filled(
                window.x + window.width + 2,
                window.y,
                2,
                window.height + 10,
                rgb(0, 255, 0),
            ); // right
            draw_window(
                (USER_SHM_BASE + window.shm_id * SHM_SLOT_SIZE) as *const u32,
                window.width as u32,
                window.height as u32,
                window.x + 2,
                window.y + 10,
            )
        };
    }
}

pub fn update_mouse(fb_width: usize, fb_height: usize, mouse: &mut CompositorMouse) {
    let mut windows_opt = WINDOWS.lock();
    let windows = windows_opt.as_mut().expect("Could not get Windows");

    let mut kbd_buf = [KeyboardEvent::default(); 32];
    input_read(kbd_buf.as_mut_ptr(), 32);

    #[allow(static_mut_refs)]
    let (rel_x, rel_y) = unsafe { MOUSE.take_motion() };

    mouse.x = (mouse.x as i64 + rel_x as i64).clamp(0, fb_width as i64 - 1) as usize;
    mouse.y = (mouse.y as i64 + rel_y as i64).clamp(0, fb_height as i64 - 1) as usize;

    if let Some(dragged_window_pid) = mouse.dragged_window {
        let dragged_window = windows.get_mut(&dragged_window_pid).unwrap();
        dragged_window.x = (dragged_window.x as i64 + rel_x as i64)
            .clamp(0, fb_width as i64 - dragged_window.width as i64 - 2)
            as usize;
        dragged_window.y = (dragged_window.y as i64 + rel_y as i64)
            .clamp(0, fb_height as i64 - dragged_window.height as i64 - 10)
            as usize;
    }
}

fn update_window_drag(mouse: &mut CompositorMouse) {
    let mut windows_opt = WINDOWS.lock();
    let windows = windows_opt.as_mut().expect("Could not get Windows");

    if !unsafe { MOUSE.left_button_pressed } {
        mouse.dragged_window = None;
    } else if mouse.dragged_window.is_none() {
        for (pid, window) in windows.iter() {
            if mouse.x >= window.x
                && mouse.x <= window.x + window.width
                && mouse.y >= window.y
                && mouse.y <= window.y + 8
            {
                mouse.dragged_window = Some(pid.clone());
                break;
            }
        }
    }
}

pub fn main_loop() -> ! {
    create_port_rust(
        String::from("window_manager"),
        Permissions::READ | Permissions::WRITE,
    );

    *WINDOWS.lock() = Some(BTreeMap::new());

    let mut private_ipcs: Vec<(u64, String)> = Vec::new();
    let mut mouse = CompositorMouse {
        x: 0,
        y: 0,
        dragged_window: None,
    };
    let (fb_width, fb_height) = get_framebuffer_size();
    let empty_framebuffer = malloc((fb_width * fb_height * size_of::<u32>()) as u64) as *mut u32;
    unsafe {
        let fb_slice = core::slice::from_raw_parts_mut(empty_framebuffer, fb_width * fb_height);
        fb_slice.fill(rgb(0, 0, 255));
    }

    let fb_ptr = USER_FB_BASE as *mut UserFrameBuffer;

    loop {
        unsafe {
            (*fb_ptr).load_from_ptr(empty_framebuffer, fb_width, fb_height);
        }
        process_messages(&mut private_ipcs);
        draw_windows();
        update_window_drag(&mut mouse);
        update_mouse(fb_width, fb_height, &mut mouse);
        unsafe {
            mouse_draw(mouse.x, mouse.y, &mut (*fb_ptr));
            framebuffer_swap();
            sleep_ms(1000 / 90);
        };
    }
}
