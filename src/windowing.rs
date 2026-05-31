use crate::{
    framebuffer::{USER_FB_BASE, bmp_draw, draw_window, get_framebuffer_size, rectangle_filled},
    input::{MOUSE, input_read},
};
use alloc::{
    collections::btree_map::BTreeMap,
    ffi::CString,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::{fmt::Write, sync::atomic::AtomicU64};
use spin::mutex::Mutex;

use xunil::{
    graphics::{font_render::render_text, framebuffer::WindowFrameBuffer, rgb},
    io::{
        input::KeyboardEvent,
        ipc::{Permissions, create_port_rust, manage_port_rust, read_port_rust, write_port_rust},
        time::{Timeval, Timezone, gettimeofday, sleep_ms},
    },
    kill,
    mem::{free, malloc},
    print, println,
    shm::{SHM_SLOT_SIZE, USER_SHM_BASE, shm_open_rust},
    syscall::{EXECVE, FRAMEBUFFER_SWAP, syscall0, syscall1},
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn framebuffer_swap() -> i32 {
    unsafe {
        return syscall0(FRAMEBUFFER_SWAP) as i32;
    }
}

pub static WINDOWS: Mutex<Option<BTreeMap<u64, CompositorWindow>>> = Mutex::new(None);
pub static FOCUSED_WINDOW: AtomicU64 = AtomicU64::new(0);

pub struct CompositorWindow {
    pub width: usize,
    pub height: usize,
    pub x: usize,
    pub y: usize,
    pub shm_name: String,
    pub shm_id: u64,
    pub shm_size: u64,
    pub dirty: bool,
    pub minimized: bool,
    pub title: String,
}

pub struct CompositorMouse {
    pub x: usize,
    pub y: usize,
    pub last_left_clicked_state: bool,
    pub dragged_window: Option<u64>,
}

unsafe impl Send for CompositorWindow {}

static WALLPAPER_BYTES: &[u8] = include_bytes!("../../../assets/images/wallpaper.bmp");
static CURSOR_BYTES: &[u8] = include_bytes!("../../../assets/images/cursor.bmp");
static LOGO_BYTES: &[u8] = include_bytes!("../../../assets/images/logo.bmp");

fn point_in_rect(x: usize, y: usize, x2: usize, y2: usize, width: usize, height: usize) -> bool {
    x >= x2 && x <= x2 + width && y >= y2 && y <= y2 + height
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

pub fn rand(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn get_time() -> (u64, u64) {
    let timeval = malloc(size_of::<Timeval>() as u64) as *mut Timeval;
    let timezone = malloc(size_of::<Timezone>() as u64) as *mut Timezone;
    unsafe { gettimeofday(timeval, timezone) };

    let result = unsafe { ((*timeval).tv_usec as u64, (*timeval).tv_sec as u64) };

    free(timeval as *mut u8);
    free(timezone as *mut u8);

    result
}

fn get_time_seed() -> u64 {
    let time = get_time();
    time.0 + time.1
}

fn request_window_buf(
    sender: u64,
    private_ipc_name: String,
    arguments: Vec<&str>,
    fb_width: usize,
    fb_height: usize,
) {
    let mut windows_opt = WINDOWS.lock();
    let windows = windows_opt.as_mut().expect("Could not get Windows");

    let title = arguments[1].to_string();

    let requested_size = (
        arguments[2].parse::<usize>().unwrap_or(0),
        arguments[3].parse::<usize>().unwrap_or(0),
    );

    if requested_size.0 == 0 || requested_size.1 == 0 {
        print("Could not parse window size");
        return;
    }

    let mut rand_seed = get_time_seed();

    let (width, height, x, y) = (
        requested_size.0,
        requested_size.1,
        (rand(&mut rand_seed) % ((fb_width - requested_size.0 - 2) as u64)) as usize,
        (rand(&mut rand_seed) % ((fb_height - requested_size.1 - 10) as u64)) as usize,
    );

    let shm_name = fnv1a_hex(format!(
        "{} {} {} {} {} {} {}",
        title, width, height, x, y, private_ipc_name, sender
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
        title,
        minimized: false,
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

fn process_messages(private_ipcs: &mut Vec<(u64, String)>, fb_width: usize, fb_height: usize) {
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
                    fb_width,
                    fb_height,
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

pub fn update_mouse(fb_width: usize, fb_height: usize, mouse: &mut CompositorMouse) {
    let mut windows_opt = WINDOWS.lock();
    let windows = windows_opt.as_mut().expect("Could not get Windows");

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
            if point_in_rect(mouse.x, mouse.y, window.x, window.y, window.width, 8) {
                FOCUSED_WINDOW.store(pid.clone(), core::sync::atomic::Ordering::Relaxed);
                mouse.dragged_window = Some(pid.clone());
                break;
            }
        }
    }

    for (pid, window) in windows.iter() {
        if point_in_rect(
            mouse.x,
            mouse.y,
            window.x,
            window.y,
            window.width,
            window.height,
        ) {
            FOCUSED_WINDOW.store(pid.clone(), core::sync::atomic::Ordering::Relaxed);
            break;
        }
    }
}

fn send_events(
    kbd_events: &[KeyboardEvent],
    kbd_events_n: usize,
    private_ipcs: &mut Vec<(u64, String)>,
) {
    let focused_window_pid = FOCUSED_WINDOW.load(core::sync::atomic::Ordering::Relaxed);
    for (pid, private_ipc_name) in private_ipcs.iter() {
        if pid.clone() == focused_window_pid {
            let mut kbd_event_string: String = String::new();

            for kbd_event in &kbd_events[0..kbd_events_n] {
                kbd_event_string.push_str(
                    format!(
                        " kbd {} {} {} {}",
                        kbd_event.key, kbd_event.mods, kbd_event.state, kbd_event.unicode
                    )
                    .as_str(),
                );
            }

            #[allow(static_mut_refs)]
            let mouse_string = unsafe {
                let (x, y) = MOUSE.take_motion();

                format!(
                    " mouse {} {} {} {} {}",
                    MOUSE.left_button_pressed.clone(),
                    MOUSE.right_button_pressed.clone(),
                    MOUSE.middle_button_pressed.clone(),
                    x.clone(),
                    y.clone()
                )
            };

            write_port_rust(
                private_ipc_name.clone(),
                format!("input_update{}{}", kbd_event_string, mouse_string),
            );

            return;
        }
    }
}

pub fn add_button(
    fb: &mut WindowFrameBuffer,
    compositor_mouse: &mut CompositorMouse,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: u32,
    text: &str,
    text_color: u32,
    mut on_click: impl FnMut<()>,
) {
    add_rect(fb, x, y, width, height, color, text, text_color);

    if unsafe { MOUSE.left_button_pressed } && compositor_mouse.last_left_clicked_state != true {
        if point_in_rect(compositor_mouse.x, compositor_mouse.y, x, y, width, height) {
            on_click();
        }
    }
}

pub fn add_rect(
    fb: &mut WindowFrameBuffer,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: u32,
    text: &str,
    text_color: u32,
) {
    rectangle_filled(x, y, width, height, color);
    render_text(
        fb,
        x + width / 2 - (4 * text.len()),
        y + height / 2 - 4,
        text,
        1,
        text_color,
        0,
    );
}

fn draw_windows_and_dock(
    compositor_mouse: &mut CompositorMouse,
    fb_width: usize,
    fb_height: usize,
) {
    let mut windows_opt = WINDOWS.lock();
    let windows = windows_opt.as_mut().expect("Could not get Windows");

    let mut fake_window_buffer = WindowFrameBuffer {
        ptr: (USER_FB_BASE + 0x1000) as *mut u32,
        width: fb_width,
        height: fb_height,
    };

    // dock bg
    rectangle_filled(0, fb_height - 20, fb_width, 20, rgb(53, 126, 199));

    let mut dock_x: usize = 80 + 5;

    let mut windows_to_destroy: Vec<u64> = Vec::new();

    for (pid, window) in windows.iter_mut() {
        if !window.dirty {
            continue;
        }

        if !window.minimized {
            unsafe {
                draw_window(
                    (USER_SHM_BASE + window.shm_id * SHM_SLOT_SIZE) as *const u32,
                    window.width as u32,
                    window.height as u32,
                    window.x + 2,
                    window.y + 10,
                );

                // render titlebar
                add_rect(
                    &mut fake_window_buffer,
                    window.x,
                    window.y,
                    window.width,
                    10,
                    rgb(128, 128, 128),
                    &window.title,
                    rgb(0, 0, 0),
                );

                // close button
                add_button(
                    &mut fake_window_buffer,
                    compositor_mouse,
                    window.x + window.width - 10,
                    window.y,
                    10,
                    10,
                    rgb(255, 0, 0),
                    "X",
                    rgb(0, 0, 0),
                    || {
                        kill(pid.clone() as isize);
                        windows_to_destroy.push(pid.clone());
                    },
                );

                // minimize button
                add_button(
                    &mut fake_window_buffer,
                    compositor_mouse,
                    window.x + window.width - 21,
                    window.y,
                    10,
                    10,
                    rgb(67, 70, 75),
                    "-",
                    rgb(0, 0, 0),
                    || {
                        window.minimized = true;
                    },
                );

                // render frame around window
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
            }
        };

        let required_bar_width = window.title.len() * 4 + 2 * 50;

        add_button(
            &mut fake_window_buffer,
            compositor_mouse,
            dock_x,
            fb_height - 20,
            required_bar_width,
            20,
            if window.minimized {
                rgb(220, 220, 220)
            } else {
                rgb(67, 70, 75)
            },
            &window.title,
            rgb(0, 0, 0),
            || {
                window.minimized = !window.minimized;
            },
        );

        dock_x += required_bar_width + 5;
    }

    let (hours, minutes, seconds) = unix_to_hms(get_time().1);
    let time_text = format!("{:02}:{:02}:{:02}", hours, minutes, seconds);

    render_text(
        &mut fake_window_buffer,
        fb_width - (time_text.len() * 8) - 5,
        fb_height - 10,
        &time_text,
        1,
        rgb(255, 255, 255),
        0,
    );

    for pid in &windows_to_destroy {
        windows.remove(&pid);
    }

    drop(windows_to_destroy);
}

fn unix_to_hms(timestamp: u64) -> (u64, u64, u64) {
    let seconds = timestamp % 86400;
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    (h, m, s)
}

fn draw_start_menu(
    fb_width: usize,
    fb_height: usize,
    should_show_start: &mut bool,
    compositor_mouse: &mut CompositorMouse,
) {
    let mut fake_window_buffer = WindowFrameBuffer {
        ptr: (USER_FB_BASE + 0x1000) as *mut u32,
        width: fb_width,
        height: fb_height,
    };

    const START_MENU_ITEMS: [&str; 3] = ["doomgeneric", "shell", "badapple"];
    const START_MENU_WIDTH: usize = 275;
    const START_MENU_HEIGHT: usize = 325;
    const START_MENU_ITEM_WIDTH: usize = 200;
    const START_MENU_ITEM_HEIGHT: usize = 35;
    const START_MENU_SPACING: usize = 0;

    // start menu bg
    rectangle_filled(
        0,
        fb_height - 20 - 5 - START_MENU_HEIGHT,
        START_MENU_WIDTH,
        START_MENU_HEIGHT,
        rgb(63, 131, 196),
    );

    // user
    bmp_draw(
        0,
        fb_height - 20 - 5 - START_MENU_HEIGHT,
        35,
        35,
        160,
        160,
        LOGO_BYTES,
    );

    render_text(
        &mut fake_window_buffer,
        35 + 7,
        fb_height - 20 - 5 - START_MENU_HEIGHT + (35 / 2) - 4,
        "xuT (evil twin)",
        1,
        rgb(255, 255, 255),
        0,
    );

    let mut start_menu_y = fb_height - 20 - 5 - START_MENU_HEIGHT + (35 / 2) - 4 + (35 + 7) + 5;

    rectangle_filled(
        0,
        start_menu_y,
        START_MENU_WIDTH,
        START_MENU_HEIGHT - (START_MENU_ITEM_HEIGHT + START_MENU_SPACING) * START_MENU_ITEMS.len(),
        rgb(255, 255, 255),
    );

    for item in START_MENU_ITEMS {
        add_button(
            &mut fake_window_buffer,
            compositor_mouse,
            0,
            start_menu_y,
            START_MENU_ITEM_WIDTH,
            START_MENU_ITEM_HEIGHT,
            rgb(255, 255, 255),
            item,
            rgb(0, 0, 0),
            || {
                *should_show_start = false;

                unsafe {
                    let _ = syscall1(
                        EXECVE,
                        CString::new(item).unwrap_or_default().as_ptr() as *const u8 as isize,
                    );
                };

                return;
            },
        );

        start_menu_y += START_MENU_ITEM_HEIGHT + START_MENU_SPACING;
    }

    if unsafe { MOUSE.left_button_pressed }
        && compositor_mouse.last_left_clicked_state != true
        && !point_in_rect(
            compositor_mouse.x,
            compositor_mouse.y,
            0,
            fb_height - 20 - 5 - START_MENU_HEIGHT,
            START_MENU_ITEM_WIDTH,
            START_MENU_ITEMS.len() * (START_MENU_ITEM_HEIGHT + START_MENU_SPACING),
        )
        && !point_in_rect(
            compositor_mouse.x,
            compositor_mouse.y,
            0,
            fb_height - 20,
            40,
            20,
        )
    {
        *should_show_start = false;
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
        last_left_clicked_state: false,
    };
    let (fb_width, fb_height) = get_framebuffer_size();
    let empty_framebuffer = malloc((fb_width * fb_height * size_of::<u32>()) as u64) as *mut u32;
    unsafe {
        let fb_slice = core::slice::from_raw_parts_mut(empty_framebuffer, fb_width * fb_height);
        fb_slice.fill(rgb(0, 0, 255));
    }

    let mut kbd_events: [KeyboardEvent; 16] = [KeyboardEvent::default(); 16];

    let mut should_show_start: bool = false;

    loop {
        unsafe { sleep_ms(1000 / 90) };
        process_messages(&mut private_ipcs, fb_width, fb_height);
        let kbd_events_n = input_read(kbd_events.as_mut_ptr(), 16);
        unsafe {
            bmp_draw(0, 0, fb_width, fb_height, 1280, 698, WALLPAPER_BYTES);
            sleep_ms(0); // yield
        }

        draw_windows_and_dock(&mut mouse, fb_width, fb_height);
        update_window_drag(&mut mouse);
        update_mouse(fb_width, fb_height, &mut mouse);
        send_events(&kbd_events, kbd_events_n, &mut private_ipcs);
        unsafe { sleep_ms(0) }; // yield

        if should_show_start {
            draw_start_menu(fb_width, fb_height, &mut should_show_start, &mut mouse);
        }
        // dock start button
        rectangle_filled(0, fb_height - 20, 30, 20, rgb(120, 174, 67));
        bmp_draw(5, fb_height - 20, 18, 18, 160, 160, LOGO_BYTES);
        add_button(
            &mut WindowFrameBuffer {
                ptr: (USER_FB_BASE + 0x1000) as *mut u32,
                width: fb_width,
                height: fb_height,
            },
            &mut mouse,
            30,
            fb_height - 20,
            50,
            20,
            rgb(120, 174, 67),
            "start",
            rgb(255, 255, 255),
            || {
                should_show_start = !should_show_start;
            },
        );

        unsafe {
            bmp_draw(mouse.x, mouse.y, 24, 24, 24, 24, CURSOR_BYTES);
            mouse.last_left_clicked_state = MOUSE.left_button_pressed;
            framebuffer_swap();
        };
    }
}
