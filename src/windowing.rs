use crate::framebuffer::draw_window;
use alloc::{
    collections::btree_map::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::Write;
use spin::mutex::Mutex;

use xunil::{
    io::{
        ipc::{Permissions, create_port_rust, manage_port_rust, read_port_rust, write_port_rust},
        time::sleep_ms,
    },
    shm::{SHM_SLOT_SIZE, USER_SHM_BASE, shm_open_rust},
    syscall::{FRAMEBUFFER_SWAP, syscall0},
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn framebuffer_swap() -> i32 {
    unsafe {
        return syscall0(FRAMEBUFFER_SWAP) as i32;
    }
}

pub static WINDOWS: Mutex<Option<BTreeMap<u64, Window>>> = Mutex::new(None);

const INITIAL_WINDOW_WIDTH: usize = 960;
const INITIAL_WINDOW_HEIGHT: usize = 640;

pub struct Window {
    pub width: usize,
    pub height: usize,
    pub x: usize,
    pub y: usize,
    pub shm_name: String,
    pub shm_id: u64,
    pub shm_size: u64,
    pub dirty: bool,
}

unsafe impl Send for Window {}

fn request_priv_ipc(sender: u64) -> String {
    let ipc_name = format!("wm_priv_{}", sender);
    create_port_rust(ipc_name.clone(), Permissions::empty());
    manage_port_rust(
        ipc_name.clone(),
        sender,
        Permissions::READ | Permissions::WRITE,
    );
    write_port_rust(
        "window_manager".to_string(),
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

fn request_window_buf(sender: u64, private_ipc_name: String) {
    let mut windows_opt = WINDOWS.lock();
    let windows = windows_opt.as_mut().expect("Could not get Windows");

    let (width, height, x, y) = (INITIAL_WINDOW_WIDTH, INITIAL_WINDOW_HEIGHT, 0, 0);

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
        return;
    }

    let window = Window {
        width,
        height,
        x,
        y,
        shm_name: shm_name.clone(),
        shm_id: shm_id as u64,
        shm_size,
        dirty: false,
    };

    windows.insert(sender, window);

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
                request_window_buf(pid.clone(), ipc_name.clone());
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
            draw_window(
                (USER_SHM_BASE + window.shm_id * SHM_SLOT_SIZE) as *const u32,
                window.width as u32,
                window.height as u32,
                window.x,
                window.y,
            )
        };
    }
}

pub fn main_loop() -> ! {
    create_port_rust(
        String::from("window_manager"),
        Permissions::READ | Permissions::WRITE,
    );

    *WINDOWS.lock() = Some(BTreeMap::new());

    let mut private_ipcs: Vec<(u64, String)> = Vec::new();

    loop {
        process_messages(&mut private_ipcs);
        draw_windows();
        unsafe { framebuffer_swap() };
        unsafe { sleep_ms(1000 / 90) };
    }
}
