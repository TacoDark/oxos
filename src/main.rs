#![no_std]
#![no_main]

use core::panic::PanicInfo;

const VGA_BUFFER: *mut u8 = 0xb8000 as *mut u8;
const WIDTH: usize = 80;

fn clear_screen() {
    for i in 0..(WIDTH * 25) {
        unsafe {
            *VGA_BUFFER.add(i * 2) = b' ';
            *VGA_BUFFER.add(i * 2 + 1) = 0x0f;
        }
    }
}

fn print_at(s: &str, row: usize) {
    for (i, byte) in s.bytes().enumerate() {
        let idx = (row * WIDTH + i) * 2;
        unsafe {
            *VGA_BUFFER.add(idx) = byte;
            *VGA_BUFFER.add(idx + 1) = 0x0f;
        }
    }
}

use core::fmt::Write;

fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!("in al, dx", in("dx") port, out("al") value);
    }
    value
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    clear_screen();
    print_at("OxOS Command Line", 0);

    let mut row = 1;
    let mut col = 2;
    print_at(">", row);

    let mut last_scancode = 0u8;
    let mut cmd_buf = [0u8; 80];
    let mut cmd_len = 0;
    let mut shift = false;

    loop {
        let scancode = inb(0x60);

        // Shift press/release handling
        match scancode {
            0x2A | 0x36 => { // Left or Right Shift pressed
                shift = true;
            }
            0xAA | 0xB6 => { // Left or Right Shift released
                shift = false;
            }
            _ => {}
        }

        // Only handle make codes (ignore break codes) and avoid repeats
        if scancode != 0 && scancode & 0x80 == 0 && scancode != last_scancode {
            match scancode {
                0x0E => { // Backspace
                    if col > 2 && cmd_len > 0 {
                        col -= 1;
                        cmd_len -= 1;
                        unsafe {
                            *VGA_BUFFER.add((row * WIDTH + col) * 2) = b' ';
                            *VGA_BUFFER.add((row * WIDTH + col) * 2 + 1) = 0x0f;
                        }
                    }
                }
                0x1C => { // Enter
                    let cmd = &cmd_buf[..cmd_len];
                    row += 1;
                    col = 2;

                    if cmd.starts_with(b"echo ") {
                        let msg = &cmd[5..];
                        print_at(core::str::from_utf8(msg).unwrap_or(""), row);
                        row += 1;
                    } else if cmd == b"clear" {
                        clear_screen();
                        row = 1;
                        col = 2;
                        print_at("OxOS Command Line", 0);
                    } else if cmd_len > 0 {
                        print_at("Unknown command", row);
                        row += 1;
                    }

                    cmd_len = 0;
                    print_at(">", row);
                }
                _ => {
                    if let Some(ascii) = scancode_to_ascii(scancode, shift) {
                        if cmd_len < cmd_buf.len() {
                            // Store in buffer if space
                            cmd_buf[cmd_len] = ascii;
                            cmd_len += 1;
                        }
                        // Always print, even if buffer is full
                        unsafe {
                            *VGA_BUFFER.add((row * WIDTH + col) * 2) = ascii;
                            *VGA_BUFFER.add((row * WIDTH + col) * 2 + 1) = 0x0f;
                        }
                        col += 1;
                        if col >= WIDTH {
                            col = 2;
                            row += 1;
                            if row >= 25 {
                                row = 1;
                                clear_screen();
                                print_at("OxOS Command Line", 0);
                            }
                            print_at(">", row);
                        }
                    }
                }
            }
        }

        // Update last_scancode, but reset to 0 if key is released
        if scancode == 0 || scancode & 0x80 != 0 {
            last_scancode = 0;
        } else {
            last_scancode = scancode;
        }
    }
}

// Add this function to map scancodes to ASCII (US QWERTY, minimal)
fn scancode_to_ascii(scancode: u8, shift: bool) -> Option<u8> {
    match (scancode, shift) {
        // Number row and symbols
        (0x02, false) => Some(b'1'), (0x02, true) => Some(b'!'),
        (0x03, false) => Some(b'2'), (0x03, true) => Some(b'@'),
        (0x04, false) => Some(b'3'), (0x04, true) => Some(b'#'),
        (0x05, false) => Some(b'4'), (0x05, true) => Some(b'$'),
        (0x06, false) => Some(b'5'), (0x06, true) => Some(b'%'),
        (0x07, false) => Some(b'6'), (0x07, true) => Some(b'^'),
        (0x08, false) => Some(b'7'), (0x08, true) => Some(b'&'),
        (0x09, false) => Some(b'8'), (0x09, true) => Some(b'*'),
        (0x0A, false) => Some(b'9'), (0x0A, true) => Some(b'('),
        (0x0B, false) => Some(b'0'), (0x0B, true) => Some(b')'),
        (0x0C, false) => Some(b'-'), (0x0C, true) => Some(b'_'),
        (0x0D, false) => Some(b'='), (0x0D, true) => Some(b'+'),
        (0x1A, false) => Some(b'['), (0x1A, true) => Some(b'{'),
        (0x1B, false) => Some(b']'), (0x1B, true) => Some(b'}'),
        (0x2B, false) => Some(b'\\'), (0x2B, true) => Some(b'|'),
        (0x27, false) => Some(b';'), (0x27, true) => Some(b':'),
        (0x28, false) => Some(b'\''), (0x28, true) => Some(b'"'),
        (0x29, false) => Some(b'`'), (0x29, true) => Some(b'~'),
        (0x33, false) => Some(b','), (0x33, true) => Some(b'<'),
        (0x34, false) => Some(b'.'), (0x34, true) => Some(b'>'),
        (0x35, false) => Some(b'/'), (0x35, true) => Some(b'?'),
        // Letters
        (sc, false) if (0x10..=0x19).contains(&sc) => Some(b"qwertyuiop"[(sc - 0x10) as usize]),
        (sc, true)  if (0x10..=0x19).contains(&sc) => Some(b"QWERTYUIOP"[(sc - 0x10) as usize]),
        (sc, false) if (0x1E..=0x26).contains(&sc) => Some(b"asdfghjkl"[(sc - 0x1E) as usize]),
        (sc, true)  if (0x1E..=0x26).contains(&sc) => Some(b"ASDFGHJKL"[(sc - 0x1E) as usize]),
        (sc, false) if (0x2C..=0x32).contains(&sc) => Some(b"zxcvbnm"[(sc - 0x2C) as usize]),
        (sc, true)  if (0x2C..=0x32).contains(&sc) => Some(b"ZXCVBNM"[(sc - 0x2C) as usize]),
        // Space
        (0x39, _) => Some(b' '),
        _ => None,
    }
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

#[no_mangle]
pub extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    unsafe {
        while i < n {
            *dest.add(i) = *src.add(i);
            i += 1;
        }
    }
    dest
}

#[no_mangle]
pub extern "C" fn memset(s: *mut u8, c: i32, n: usize) -> *mut u8 {
    let mut i = 0;
    unsafe {
        while i < n {
            *s.add(i) = c as u8;
            i += 1;
        }
    }
    s
}

#[no_mangle]
pub extern "C" fn memcmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let a = unsafe { *s1.add(i) };
        let b = unsafe { *s2.add(i) };
        if a != b {
            return a as i32 - b as i32;
        }
    }
    0
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}