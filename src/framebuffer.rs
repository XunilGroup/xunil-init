use xunil::{
    graphics::rgb,
    syscall::{MAP_FRAMEBUFFER, syscall0},
};

pub const USER_FB_BASE: u64 = 0x0000_7F00_0000_0000;
const BMP_HEADER_SIZE: usize = 138;

#[repr(C)]
pub struct UserFrameBuffer {
    pub buf_virt: *mut u32,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
}

impl UserFrameBuffer {
    pub unsafe fn draw_window(
        &mut self,
        src_ptr: *const u32,
        width: usize,
        height: usize,
        x: usize,
        y: usize,
    ) {
        for dy in 0..height {
            let src_row = core::hint::black_box(unsafe { src_ptr.add(dy * width) });
            let dst_row =
                core::hint::black_box(unsafe { self.buf_virt.add((dy + y) * self.pitch + x) });
            unsafe {
                core::ptr::copy_nonoverlapping(src_row, dst_row, width);
            }
        }
    }

    pub unsafe fn load_from_ptr(
        &mut self,
        src_ptr: *const u32,
        src_width: usize,
        src_height: usize,
    ) {
        let _buf = unsafe { core::ptr::read_volatile(&self.buf_virt) };
        for dy in 0..self.height {
            let sy = dy * src_height / self.height;

            for dx in 0..self.width {
                let sx = dx * src_width / self.width;

                let src_pixel = unsafe { *core::hint::black_box(src_ptr.add(sy * src_width + sx)) };

                unsafe { core::hint::black_box(self.buf_virt.add(dy * self.pitch + dx)).write(src_pixel) };
            }
        }
    }

    #[inline(always)]
    pub fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = y * self.width + x;
        if idx >= self.width * self.height {
            return;
        }
        unsafe { core::hint::black_box(self.buf_virt.add(idx)).write(color) };
    }

    #[inline(always)]
    pub fn fill_span(&mut self, x: usize, y: usize, len: usize, color: u32) {
        if y >= self.height || x >= self.width || len == 0 {
            return;
        }
        let len = core::cmp::min(len, self.width - x);
        let start = y * self.width + x;
        unsafe {
            let slice = core::slice::from_raw_parts_mut(
                core::hint::black_box(self.buf_virt.add(start)),
                len,
            );
            slice.fill(color);
        }
    }
}

pub fn get_framebuffer_size() -> (usize, usize) {
    let fb_ptr = USER_FB_BASE as *mut UserFrameBuffer;

    unsafe { ((*fb_ptr).width, (*fb_ptr).height) }
}

pub unsafe fn map_framebuffer() {
    unsafe { syscall0(MAP_FRAMEBUFFER) };
}

pub unsafe fn draw_window(buffer: *const u32, width: u32, height: u32, x: usize, y: usize) {
    let fb_ptr = USER_FB_BASE as *mut UserFrameBuffer;
    unsafe { (*fb_ptr).draw_window(buffer, width as usize, height as usize, x, y) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn draw_buffer(buffer: *const u32, width: u32, height: u32) -> i32 {
    let fb_ptr = USER_FB_BASE as *mut UserFrameBuffer;
    unsafe { (*fb_ptr).load_from_ptr(buffer, width as usize, height as usize) };

    0
}

pub fn rectangle_filled(x: usize, y: usize, width: usize, height: usize, color: u32) {
    let fb_ptr = USER_FB_BASE as *mut UserFrameBuffer;
    for yy in y..y + height {
        unsafe { (*fb_ptr).fill_span(x, yy, width, color) };
    }
}

pub fn bmp_draw(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    src_width: usize,
    src_height: usize,
    data: &[u8],
) {
    let fb_ptr = USER_FB_BASE as *mut UserFrameBuffer;

    let pixels = &data[BMP_HEADER_SIZE..];

    for dst_y in 0..height {
        let src_y = dst_y * src_height / height;

        for dst_x in 0..width {
            let src_x = dst_x * src_width / width;

            let bmp_y = src_height - 1 - src_y;

            let i = (bmp_y * src_width + src_x) * 4;

            let b = pixels[i];
            let g = pixels[i + 1];
            let r = pixels[i + 2];
            let a = pixels[i + 3];

            if a < 255 {
                continue;
            }

            let color = rgb(r, g, b);

            unsafe {
                (*fb_ptr).put_pixel(x + dst_x, y + dst_y, color);
            }
        }
    }
}
