#![no_std]
#![no_main]

use core::panic::PanicInfo;

const VGA_BUFFER: *mut u8 = 0xb8000 as *mut u8;
const WIDTH: usize = 80;

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

fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!("in al, dx", in("dx") port, out("al") value);
    }
    value
}

// --- Filesystem structures and helpers ---

const MAX_FILES: usize = 16;
const MAX_DIRS: usize = 8;
const MAX_NAME: usize = 16;
const MAX_DATA: usize = 256;
const MAX_DIR_STORAGE: usize = 32;

#[derive(Clone, Copy)]
struct File {
    name: [u8; MAX_NAME],
    data: [u8; MAX_DATA],
    len: usize,
}

#[derive(Clone, Copy)]
struct Directory {
    name: [u8; MAX_NAME],
    files: [Option<File>; MAX_FILES],
    dirs: [Option<usize>; MAX_DIRS], // indexes into DIR_STORAGE
    parent: Option<usize>,           // index into DIR_STORAGE
}

// Pre-allocate all directories statically
static mut DIR_STORAGE: [Directory; MAX_DIR_STORAGE] = [Directory {
    name: [0; MAX_NAME],
    files: [None; MAX_FILES],
    dirs: [None; MAX_DIRS],
    parent: None,
}; MAX_DIR_STORAGE];

static mut DIR_ALLOC_INDEX: usize = 1; // 0 is root

// Root dir is always at index 0
static mut CURRENT_DIR_IDX: usize = 0;

unsafe fn alloc_dir() -> Option<usize> {
    if DIR_ALLOC_INDEX < MAX_DIR_STORAGE {
        let idx = DIR_ALLOC_INDEX;
        DIR_ALLOC_INDEX += 1;
        Some(idx)
    } else {
        None
    }
}

fn name_eq(a: &[u8], b: &[u8]) -> bool {
    let a_end = a.iter().position(|&c| c == 0 || c == b' ').unwrap_or(a.len());
    let b_end = b.iter().position(|&c| c == 0 || c == b' ').unwrap_or(b.len());
    a[..a_end] == b[..b_end]
}

unsafe fn find_dir(dir: &Directory, name: &[u8]) -> Option<usize> {
    for d in dir.dirs.iter() {
        if let Some(idx) = d {
            let subdir = &DIR_STORAGE[*idx];
            if name_eq(&subdir.name, name) {
                return Some(*idx);
            }
        }
    }
    None
}

unsafe fn find_file<'a>(dir: &'a Directory, name: &[u8]) -> Option<&'a File> {
    for f in dir.files.iter() {
        if let Some(file) = f {
            if name_eq(&file.name, name) {
                return Some(file);
            }
        }
    }
    None
}

unsafe fn find_file_mut<'a>(dir: &'a mut Directory, name: &[u8]) -> Option<&'a mut File> {
    for f in dir.files.iter_mut() {
        if let Some(file) = f {
            if name_eq(&file.name, name) {
                return Some(file);
            }
        }
    }
    None
}

// --- Main entry point ---

#[no_mangle]
pub extern "C" fn _start() -> ! {
    unsafe {
        // Initialize root directory
        DIR_STORAGE[0].name = *b"/               ";
        DIR_STORAGE[0].files = [None; MAX_FILES];
        DIR_STORAGE[0].dirs = [None; MAX_DIRS];
        DIR_STORAGE[0].parent = None;
        CURRENT_DIR_IDX = 0;
    }

    print_boot_logo();
    for _ in 0..5_000_000 { unsafe { core::arch::asm!("nop"); } }

    clear_screen();
    print_at("OxOS Command Line", 0);

    let mut row = 7;
    let mut col;
    let mut prompt_len;
    let mut path_buf = [0u8; 64];

    // Print initial prompt
    let prompt = build_path(unsafe { CURRENT_DIR_IDX }, &mut path_buf);
    print_at(prompt, row);
    prompt_len = prompt.len();
    col = prompt_len;

    let mut last_scancode = 0u8;
    let mut cmd_buf = [0u8; 80];
    let mut cmd_len = 0;
    let mut shift = false;
    let mut blink_counter = 0u32;

    loop {
        let scancode = inb(0x60);

        // Shift press/release handling
        match scancode {
            0x2A | 0x36 => { shift = true; }
            0xAA | 0xB6 => { shift = false; }
            _ => {}
        }

        // Only handle make codes (ignore break codes) and avoid repeats
        if scancode != 0 && scancode & 0x80 == 0 && scancode != last_scancode {
            match scancode {
                0x0E => { // Backspace
                    if cmd_len > 0 {
                        cmd_len -= 1;
                        let erase_col = prompt_len + cmd_len;
                        unsafe {
                            *VGA_BUFFER.add((row * WIDTH + erase_col) * 2) = b' ';
                            *VGA_BUFFER.add((row * WIDTH + erase_col) * 2 + 1) = 0x0f;
                        }
                    }
                }
                0x1C => { // Enter
                    let cmd = &cmd_buf[..cmd_len];
                    row += 1;

                    if cmd.starts_with(b"echo ") {
                        let msg = &cmd[5..];
                        print_at(core::str::from_utf8(msg).unwrap_or(""), row);
                        row += 1;
                    } else if cmd == b"clear" {
                        clear_screen();
                        row = 1;
                        let prompt = build_path(unsafe { CURRENT_DIR_IDX }, &mut path_buf);
                        print_at("OxOS Command Line", 0);
                        print_at(prompt, row);
                        prompt_len = prompt.len();
                        col = prompt_len;
                    } else if cmd == b"ls" {
                        unsafe {
                            let dir = &DIR_STORAGE[CURRENT_DIR_IDX];
                            let mut out = [0u8; 80];
                            let mut out_len = 0;
                            for d in dir.dirs.iter() {
                                if let Some(idx) = d {
                                    let subdir = &DIR_STORAGE[*idx];
                                    let name = &subdir.name;
                                    let name_len = name.iter().position(|&c| c == 0 || c == b' ').unwrap_or(MAX_NAME);
                                    if out_len + name_len + 2 < out.len() {
                                        out[out_len] = b'[';
                                        out_len += 1;
                                        out[out_len..out_len + name_len].copy_from_slice(&name[..name_len]);
                                        out_len += name_len;
                                        out[out_len] = b']';
                                        out_len += 1;
                                        out[out_len] = b' ';
                                        out_len += 1;
                                    }
                                }
                            }
                            for f in dir.files.iter() {
                                if let Some(ref file) = f {
                                    let name = &file.name;
                                    let name_len = name.iter().position(|&c| c == 0 || c == b' ').unwrap_or(MAX_NAME);
                                    if out_len + name_len + 1 < out.len() {
                                        out[out_len..out_len + name_len].copy_from_slice(&name[..name_len]);
                                        out_len += name_len;
                                        out[out_len] = b' ';
                                        out_len += 1;
                                    }
                                }
                            }
                            print_at(core::str::from_utf8(&out[..out_len]).unwrap_or(""), row);
                            row += 1;
                        }
                    } else if cmd.starts_with(b"mkdir ") {
                        unsafe {
                            let dir = &mut DIR_STORAGE[CURRENT_DIR_IDX];
                            let name = &cmd[6..];
                            let name_len = name.iter().position(|&c| c == 0 || c == b' ').unwrap_or(name.len());
                            if let Some(new_idx) = alloc_dir() {
                                let new_dir = &mut DIR_STORAGE[new_idx];
                                new_dir.name = [0; MAX_NAME];
                                new_dir.files = [None; MAX_FILES];
                                new_dir.dirs = [None; MAX_DIRS];
                                new_dir.parent = Some(CURRENT_DIR_IDX);
                                new_dir.name[..name_len].copy_from_slice(&name[..name_len]);
                                for d in dir.dirs.iter_mut() {
                                    if d.is_none() {
                                        *d = Some(new_idx);
                                        print_at("Directory created", row);
                                        row += 1;
                                        break;
                                    }
                                }
                            }
                        }
                    } else if cmd.starts_with(b"cd ") {
                        unsafe {
                            let dir = &DIR_STORAGE[CURRENT_DIR_IDX];
                            let name = &cmd[3..];
                            if name == b".." {
                                if let Some(parent_idx) = dir.parent {
                                    CURRENT_DIR_IDX = parent_idx;
                                    print_at("Moved up", row);
                                    row += 1;
                                } else {
                                    print_at("Already at root", row);
                                    row += 1;
                                }
                            } else if let Some(subdir_idx) = find_dir(dir, name) {
                                CURRENT_DIR_IDX = subdir_idx;
                                print_at("Changed directory", row);
                                row += 1;
                            } else {
                                print_at("No such directory", row);
                                row += 1;
                            }
                        }
                    } else if cmd.starts_with(b"touch ") {
                        unsafe {
                            let dir = &mut DIR_STORAGE[CURRENT_DIR_IDX];
                            let name = &cmd[6..];
                            let name_len = name.iter().position(|&c| c == 0 || c == b' ').unwrap_or(name.len());
                            let mut new_file = File {
                                name: [0u8; MAX_NAME],
                                data: [0u8; MAX_DATA],
                                len: 0,
                            };
                            new_file.name[..name_len].copy_from_slice(&name[..name_len]);
                            for f in dir.files.iter_mut() {
                                if f.is_none() {
                                    *f = Some(new_file);
                                    print_at("File created", row);
                                    row += 1;
                                    break;
                                }
                            }
                        }
                    } else if cmd.starts_with(b"write ") {
                        unsafe {
                            let dir = &mut DIR_STORAGE[CURRENT_DIR_IDX];
                            let rest = &cmd[6..];
                            if let Some(space) = rest.iter().position(|&c| c == b' ') {
                                let name = &rest[..space];
                                let text = &rest[space+1..];
                                if name.ends_with(b".txt") {
                                    let name_len = name.len();
                                    // 1. Try to find the file first
                                    let mut file_idx = None;
                                    for (i, f) in dir.files.iter().enumerate() {
                                        if let Some(file) = f {
                                            if name_eq(&file.name, name) {
                                                file_idx = Some(i);
                                                break;
                                            }
                                        }
                                    }
                                    // 2. If not found, create it
                                    if file_idx.is_none() {
                                        let mut new_file = File {
                                            name: [0u8; MAX_NAME],
                                            data: [0u8; MAX_DATA],
                                            len: 0,
                                        };
                                        new_file.name[..name_len].copy_from_slice(name);
                                        for (i, f) in dir.files.iter_mut().enumerate() {
                                            if f.is_none() {
                                                *f = Some(new_file);
                                                file_idx = Some(i);
                                                break;
                                            }
                                        }
                                    }
                                    // 3. Write to the file if we have an index
                                    if let Some(i) = file_idx {
                                        if let Some(file) = dir.files[i].as_mut() {
                                            let write_len = text.len().min(MAX_DATA);
                                            file.data[..write_len].copy_from_slice(&text[..write_len]);
                                            file.len = write_len;
                                            print_at("Wrote file", row);
                                            row += 1;
                                        } else {
                                            print_at("No space for file", row);
                                            row += 1;
                                        }
                                    } else {
                                        print_at("No space for file", row);
                                        row += 1;
                                    }
                                } else {
                                    print_at("Only .txt files supported", row);
                                    row += 1;
                                }
                            } else {
                                print_at("Usage: write <file.txt> <text>", row);
                                row += 1;
                            }
                        }
                    } else if cmd.starts_with(b"cat ") {
                        unsafe {
                            let dir = &DIR_STORAGE[CURRENT_DIR_IDX];
                            let name = &cmd[4..];
                            if name.ends_with(b".txt") {
                                if let Some(file) = find_file(dir, name) {
                                    let s = core::str::from_utf8(&file.data[..file.len]).unwrap_or("");
                                    print_at(s, row);
                                    row += 1;
                                } else {
                                    print_at("No such file", row);
                                    row += 1;
                                }
                            } else {
                                print_at("Only .txt files supported", row);
                                row += 1;
                            }
                        }
                    } else if cmd == b"about" {
                        print_at("OxOS: A hobby x86_64 OS in Rust.", row);
                        row += 1;
                        print_at("github.com/TacoDark/oxos", row);
                        row += 1;
                    } else if cmd_len > 0 {
                        print_at("Unknown command", row);
                        row += 1;
                    }

                    cmd_len = 0;
                    let prompt = build_path(unsafe { CURRENT_DIR_IDX }, &mut path_buf);
                    print_at(prompt, row);
                    prompt_len = prompt.len();
                    col = prompt_len;
                }
                _ => {
                    if let Some(ascii) = scancode_to_ascii(scancode, shift) {
                        if cmd_len < cmd_buf.len() {
                            cmd_buf[cmd_len] = ascii;
                            let draw_col = prompt_len + cmd_len;
                            unsafe {
                                *VGA_BUFFER.add((row * WIDTH + draw_col) * 2) = ascii;
                                *VGA_BUFFER.add((row * WIDTH + draw_col) * 2 + 1) = 0x0f;
                            }
                            cmd_len += 1;
                        }
                        if prompt_len + cmd_len >= WIDTH {
                            row += 1;
                            if row >= 25 {
                                row = 1;
                                clear_screen();
                                print_at("OxOS Command Line", 0);
                            }
                            let prompt = build_path(unsafe { CURRENT_DIR_IDX }, &mut path_buf);
                            print_at(prompt, row);
                            prompt_len = prompt.len();
                            col = prompt_len;
                            cmd_len = 0;
                        }
                    }
                }
            }
            last_scancode = scancode;
        }

        // Always update col before drawing the cursor
        col = prompt_len + cmd_len;
        // Cursor blinking
        blink_counter = blink_counter.wrapping_add(1);
        if blink_counter % 1_000_000 < 500_000 {
            unsafe {
                *VGA_BUFFER.add((row * WIDTH + col) * 2) = b'_';
                *VGA_BUFFER.add((row * WIDTH + col) * 2 + 1) = 0x0f;
            }
        } else {
            unsafe {
                *VGA_BUFFER.add((row * WIDTH + col) * 2) = b' ';
                *VGA_BUFFER.add((row * WIDTH + col) * 2 + 1) = 0x0f;
            }
        }

        unsafe { core::arch::asm!("pause"); }
    }
}

// --- Keyboard scancode to ASCII ---

fn scancode_to_ascii(scancode: u8, shift: bool) -> Option<u8> {
    // US QWERTY scancode set 1
    let normal = [
        0, 0, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=', 0, 0,
        b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n', 0,
        b'a', b's', b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', b'\'', b'`', 0, b'\\',
        b'z', b'x', b'c', b'v', b'b', b'n', b'm', b',', b'.', b'/', 0, b'*', 0, b' ', 0,
    ];
    let shifted = [
        0, 0, b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'_', b'+', 0, 0,
        b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P', b'{', b'}', b'\n', 0,
        b'A', b'S', b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':', b'"', b'~', 0, b'|',
        b'Z', b'X', b'C', b'V', b'B', b'N', b'M', b'<', b'>', b'?', 0, b'*', 0, b' ', 0,
    ];
    let idx = scancode as usize;
    if idx < normal.len() {
        let c = if shift { shifted[idx] } else { normal[idx] };
        if c != 0 { Some(c) } else { None }
    } else {
        None
    }
}

// --- Panic handler ---

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

// --- Boot logo ---

fn print_boot_logo() {
    let logo = [
        "   ____        ____   ",
        "  / __ \\__  _/ __ \\  ",
        " / / / / / / / / / /  ",
        "/ /_/ / /_/ / /_/ /   ",
        "\\____/\\__,_/\\____/    ",
        "      OxOS            ",
        "",
    ];
    for (i, line) in logo.iter().enumerate() {
        print_at(line, i);
    }
}

fn build_path(mut idx: usize, buf: &mut [u8]) -> &str {
    let mut parts = [[0u8; MAX_NAME]; 8];
    let mut depth = 0;
    unsafe {
        while idx != 0 && depth < parts.len() {
            let dir = &DIR_STORAGE[idx];
            let name_len = dir.name.iter().position(|&c| c == 0 || c == b' ').unwrap_or(MAX_NAME);
            parts[depth][..name_len].copy_from_slice(&dir.name[..name_len]);
            depth += 1;
            idx = dir.parent.unwrap_or(0);
        }
    }
    let mut pos = 0;
    buf[pos] = b'/';
    pos += 1;
    for i in (0..depth).rev() {
        let name = &parts[i];
        let name_len = name.iter().position(|&c| c == 0 || c == b' ').unwrap_or(MAX_NAME);
        if name_len > 0 {
            if pos + name_len < buf.len() {
                buf[pos..pos + name_len].copy_from_slice(&name[..name_len]);
                pos += name_len;
                buf[pos] = b'/';
                pos += 1;
            }
        }
    }
    if pos > 1 { pos -= 1; } // Remove trailing slash unless root
    let prompt = b"> ";
    if pos + prompt.len() < buf.len() {
        buf[pos..pos + prompt.len()].copy_from_slice(prompt);
        pos += prompt.len();
    }
    core::str::from_utf8(&buf[..pos]).unwrap_or("> ")
}
