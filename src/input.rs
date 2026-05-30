use xunil::{
    io::input::*,
    print,
    syscall::{INPUT_READ, syscall2},
};

pub struct Mouse {
    pub left_button_pressed: bool,
    pub right_button_pressed: bool,
    pub middle_button_pressed: bool,
    pub x_delta: i16,
    pub y_delta: i16,
}

impl Mouse {
    const fn new() -> Mouse {
        Mouse {
            left_button_pressed: false,
            right_button_pressed: false,
            middle_button_pressed: false,
            x_delta: 0,
            y_delta: 0,
        }
    }

    pub fn button_state(&self) -> (bool, bool, bool) {
        (
            self.left_button_pressed,
            self.right_button_pressed,
            self.middle_button_pressed,
        )
    }
    pub fn take_motion(&mut self) -> (i16, i16) {
        let old_x_delta = self.x_delta.clone();
        let old_y_delta = self.y_delta.clone();
        self.x_delta = 0;
        self.y_delta = 0;
        (old_x_delta, old_y_delta)
    }
}

pub const KEY_TO_CHAR: [Option<(char, char)>; 84] = [
    None,                   // 0  KEY_RESERVED
    Some(('\x1b', '\x1b')), // 1  KEY_ESC
    Some(('1', '!')),       // 2  KEY_1
    Some(('2', '@')),       // 3  KEY_2
    Some(('3', '#')),       // 4  KEY_3
    Some(('4', '$')),       // 5  KEY_4
    Some(('5', '%')),       // 6  KEY_5
    Some(('6', '^')),       // 7  KEY_6
    Some(('7', '&')),       // 8  KEY_7
    Some(('8', '*')),       // 9  KEY_8
    Some(('9', '(')),       // 10 KEY_9
    Some(('0', ')')),       // 11 KEY_0
    Some(('-', '_')),       // 12 KEY_MINUS
    Some(('=', '+')),       // 13 KEY_EQUAL
    Some(('\x08', '\x08')), // 14 KEY_BACKSPACE
    Some(('\t', '\t')),     // 15 KEY_TAB
    Some(('q', 'Q')),       // 16 KEY_Q
    Some(('w', 'W')),       // 17 KEY_W
    Some(('e', 'E')),       // 18 KEY_E
    Some(('r', 'R')),       // 19 KEY_R
    Some(('t', 'T')),       // 20 KEY_T
    Some(('y', 'Y')),       // 21 KEY_Y
    Some(('u', 'U')),       // 22 KEY_U
    Some(('i', 'I')),       // 23 KEY_I
    Some(('o', 'O')),       // 24 KEY_O
    Some(('p', 'P')),       // 25 KEY_P
    Some(('[', '{')),       // 26 KEY_LEFTBRACE
    Some((']', '}')),       // 27 KEY_RIGHTBRACE
    Some(('\n', '\n')),     // 28 KEY_ENTER
    None,                   // 29 KEY_LEFTCTRL
    Some(('a', 'A')),       // 30 KEY_A
    Some(('s', 'S')),       // 31 KEY_S
    Some(('d', 'D')),       // 32 KEY_D
    Some(('f', 'F')),       // 33 KEY_F
    Some(('g', 'G')),       // 34 KEY_G
    Some(('h', 'H')),       // 35 KEY_H
    Some(('j', 'J')),       // 36 KEY_J
    Some(('k', 'K')),       // 37 KEY_K
    Some(('l', 'L')),       // 38 KEY_L
    Some((';', ':')),       // 39 KEY_SEMICOLON
    Some(('\'', '"')),      // 40 KEY_APOSTROPHE
    Some(('`', '~')),       // 41 KEY_GRAVE
    None,                   // 42 KEY_LEFTSHIFT
    Some(('\\', '|')),      // 43 KEY_BACKSLASH
    Some(('z', 'Z')),       // 44 KEY_Z
    Some(('x', 'X')),       // 45 KEY_X
    Some(('c', 'C')),       // 46 KEY_C
    Some(('v', 'V')),       // 47 KEY_V
    Some(('b', 'B')),       // 48 KEY_B
    Some(('n', 'N')),       // 49 KEY_N
    Some(('m', 'M')),       // 50 KEY_M
    Some((',', '<')),       // 51 KEY_COMMA
    Some(('.', '>')),       // 52 KEY_DOT
    Some(('/', '?')),       // 53 KEY_SLASH
    None,                   // 54 KEY_RIGHTSHIFT
    Some(('*', '*')),       // 55 KEY_KPASTERISK
    None,                   // 56 KEY_LEFTALT
    Some((' ', ' ')),       // 57 KEY_SPACE
    None,                   // 58 KEY_CAPSLOCK
    None,                   // 59 KEY_F1
    None,                   // 60 KEY_F2
    None,                   // 61 KEY_F3
    None,                   // 62 KEY_F4
    None,                   // 63 KEY_F5
    None,                   // 64 KEY_F6
    None,                   // 65 KEY_F7
    None,                   // 66 KEY_F8
    None,                   // 67 KEY_F9
    None,                   // 68 KEY_F10
    None,                   // 69 KEY_NUMLOCK
    None,                   // 70 KEY_SCROLLLOCK
    Some(('7', '7')),       // 71 KEY_KP7
    Some(('8', '8')),       // 72 KEY_KP8
    Some(('9', '9')),       // 73 KEY_KP9
    Some(('-', '-')),       // 74 KEY_KPMINUS
    Some(('4', '4')),       // 75 KEY_KP4
    Some(('5', '5')),       // 76 KEY_KP5
    Some(('6', '6')),       // 77 KEY_KP6
    Some((('+'), '+')),     // 78 KEY_KPPLUS
    Some(('1', '1')),       // 79 KEY_KP1
    Some(('2', '2')),       // 80 KEY_KP2
    Some(('3', '3')),       // 81 KEY_KP3
    Some(('0', '0')),       // 82 KEY_KP0
    Some(('.', '.')),       // 83 KEY_KPDOT
];

pub struct ModState {
    shift: bool,
    caps_lock: bool,
    ctrl: bool,
    alt: bool,
}

impl ModState {
    pub const fn new() -> Self {
        Self {
            shift: false,
            caps_lock: false,
            ctrl: false,
            alt: false,
        }
    }
    pub fn update(&mut self, code: u8, value: u32) {
        match code {
            KEY_LEFTSHIFT | KEY_RIGHTSHIFT => self.shift = value != 0,
            KEY_LEFTCTRL => self.ctrl = value != 0,
            KEY_CAPSLOCK => self.caps_lock = value != 0,
            KEY_LEFTALT => self.alt = value != 0,
            _ => {}
        }
    }

    pub fn effective_shift(&self) -> bool {
        self.shift ^ self.caps_lock
    }
}

pub fn keycode_to_char(keycode: u8, shift: bool) -> Option<char> {
    let entry = KEY_TO_CHAR.get(keycode as usize)?.as_ref()?;
    Some(if shift { entry.1 } else { entry.0 })
}

pub static mut MODIFIERS: ModState = ModState::new();
pub static mut MOUSE: Mouse = Mouse::new();

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InputEvent {
    pub event_type: u16,
    pub code: u16,
    pub value: u32,
}

pub const EVENT_KEY: u16 = 0x01;
pub const EVENT_REL: u16 = 0x02;

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;

pub const REL_X: u16 = 0x00;
pub const REL_Y: u16 = 0x01;
pub const REL_WHEEL: u16 = 0x08;

#[allow(static_mut_refs)]
pub fn handle_event(event: &InputEvent) -> Option<KeyboardEvent> {
    if event.event_type == 0 {
        return None;
    }

    match event.event_type {
        EVENT_KEY => {
            unsafe { MODIFIERS.update(event.code as u8, event.value) };
            match event.code {
                BTN_LEFT => {
                    unsafe { MOUSE.left_button_pressed = event.value == 1 };
                    None
                }
                BTN_RIGHT => {
                    unsafe { MOUSE.right_button_pressed = event.value == 1 };
                    None
                }
                BTN_MIDDLE => {
                    unsafe { MOUSE.middle_button_pressed = event.value == 1 };
                    None
                }
                _ => Some(KeyboardEvent {
                    state: event.value as u8,
                    _pad1: 0,
                    key: event.code as u16,
                    mods: 0,
                    _pad2: 0,
                    unicode: keycode_to_char(event.code as u8, unsafe {
                        MODIFIERS.effective_shift()
                    })
                    .unwrap_or('\0') as u32,
                }),
            }
        }
        EVENT_REL => match event.code {
            REL_X => {
                unsafe { MOUSE.x_delta += event.value as i32 as i16 };
                None
            }
            REL_Y => {
                unsafe { MOUSE.y_delta += event.value as i32 as i16 };
                None
            }
            REL_WHEEL => None,
            _ => None,
        },
        _ => {
            print("Could not recognize virtio input event from interrupt\n");
            None
        }
    }
}

pub fn input_read(kbd_buf: *mut KeyboardEvent, n: u8) -> usize {
    let mut input_buf = [InputEvent {
        event_type: 0,
        code: 0,
        value: 0,
    }; 32];
    let n = n.min(32);
    let len = unsafe { syscall2(INPUT_READ, input_buf.as_mut_ptr() as isize, n as isize) };
    let mut current_event_n = 0;

    for i in 0..len {
        unsafe {
            let event = *((input_buf.as_mut_ptr()).add(i as usize));

            if let Some(keyboard_event) = handle_event(&event) {
                *(kbd_buf.add(current_event_n)) = keyboard_event;
                current_event_n += 1;
            }
        }
    }

    return current_event_n;
}
