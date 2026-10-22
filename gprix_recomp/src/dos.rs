/// MS-DOS & BIOS High-Level Emulation (HLE) Services.

use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::bus::MemoryBus;
use crate::cpu::CpuContext;

pub struct DosEnvironment {
    pub asset_dir: PathBuf,
    pub open_files: HashMap<u16, File>,
    pub next_handle: u16,
    pub dta_seg: u16,
    pub dta_off: u16,
    pub heap_next_seg: u16,
    pub timer_ticks: u32,
    pub timer_isr: Option<(u16, u16)>,
    pub key_queue: VecDeque<(u8, u8)>, // FIFO (ascii, scancode)
    pub find_matches: Vec<(String, PathBuf)>,
    pub find_index: usize,
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = None;
    let mut star_ti = 0;

    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_pi = Some(pi);
            pi += 1;
            star_ti = ti;
        } else if let Some(sp) = star_pi {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }

    pi == p.len()
}

fn to_dos_8_3(name: &str) -> String {
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let truncated_stem: String = stem.chars().take(8).collect();
    let truncated_ext: String = ext.chars().take(3).collect();

    if truncated_ext.is_empty() {
        truncated_stem.to_ascii_uppercase()
    } else {
        format!(
            "{}.{}",
            truncated_stem.to_ascii_uppercase(),
            truncated_ext.to_ascii_uppercase()
        )
    }
}

fn dos_matches_pattern(pattern: &str, candidate: &str) -> bool {
    let p = pattern.to_ascii_uppercase();
    let c = candidate.to_ascii_uppercase();
    if p == c || p == "*.*" || p == "*" {
        return true;
    }
    // Also match via 8.3 FAT truncation (e.g. Brazilian.PES -> BRAZILIA.PES)
    if !pattern.contains('*') && !pattern.contains('?') {
        if to_dos_8_3(pattern) == to_dos_8_3(candidate) {
            return true;
        }
    }
    glob_match(&p, &c)
}

impl DosEnvironment {
    pub fn new<P: AsRef<Path>>(asset_dir: P) -> Self {
        Self {
            asset_dir: asset_dir.as_ref().to_path_buf(),
            open_files: HashMap::new(),
            next_handle: 5, // Handles 0..4 reserved for stdin/out/err/aux/prn
            dta_seg: 0x1000,
            dta_off: 0x0080,
            heap_next_seg: 0x4000, // Dynamic heap starts at 0x4000 (well above loaded program)
            timer_ticks: 0,
            timer_isr: None,
            key_queue: VecDeque::new(),
            find_matches: Vec::new(),
            find_index: 0,
        }
    }

    /// Resolve a DOS path case-insensitively in the asset directory.
    fn resolve_path(&self, dos_path: &str) -> PathBuf {
        let clean = dos_path.replace('\\', "/");
        let filename = Path::new(&clean)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(dos_path);

        if let Ok(entries) = std::fs::read_dir(&self.asset_dir) {
            let all: Vec<_> = entries.flatten().collect();
            // 1. Exact case-insensitive match
            for entry in &all {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.eq_ignore_ascii_case(filename) {
                        return entry.path();
                    }
                }
            }

            // 2. DOS 8.3 FAT name match (e.g. "Brazilian.PES" -> "BRAZILIA.PES")
            let dos_83_target = to_dos_8_3(filename);
            for entry in &all {
                if let Ok(name) = entry.file_name().into_string() {
                    if to_dos_8_3(&name) == dos_83_target {
                        println!("[DOS] 8.3 truncation match: {} -> {}", filename, name);
                        return entry.path();
                    }
                }
            }

            // 3. DSI extension fallbacks (.PSN -> .SND, .PES -> .ESH / .PCS)
            let path_obj = Path::new(filename);
            let stem = path_obj.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let ext = path_obj.extension().and_then(|e| e.to_str()).unwrap_or("");
            let alt_ext = if ext.eq_ignore_ascii_case("PSN") {
                Some("SND")
            } else if ext.eq_ignore_ascii_case("PES") {
                Some("ESH")
            } else {
                None
            };

            if let Some(alt) = alt_ext {
                let alt_name = format!("{}.{}", stem, alt);
                let alt_83 = to_dos_8_3(&alt_name);
                for entry in &all {
                    if let Ok(name) = entry.file_name().into_string() {
                        if name.eq_ignore_ascii_case(&alt_name) || to_dos_8_3(&name) == alt_83 {
                            println!("[DOS] Path fallback: {} -> {}", filename, name);
                            return entry.path();
                        }
                    }
                }
            }
        }
        self.asset_dir.join(filename)
    }

    /// Read an ASCIIZ string from DS:DX.
    fn read_asciiz(&self, bus: &mut MemoryBus, seg: u16, mut off: u16) -> String {
        let mut bytes = Vec::new();
        loop {
            let b = bus.read_u8(seg, off);
            if b == 0 {
                break;
            }
            bytes.push(b);
            off = off.wrapping_add(1);
        }
        String::from_utf8_lossy(&bytes).to_string()
    }

    /// Write a FindFirst/FindNext matching entry into the current DTA.
    fn write_dta_entry(&self, bus: &mut MemoryBus, index: usize) {
        if index >= self.find_matches.len() {
            return;
        }
        let (name, path) = &self.find_matches[index];
        let file_len = std::fs::metadata(path).map(|m| m.len() as u32).unwrap_or(0);
        let dta_seg = self.dta_seg;
        let dta_off = self.dta_off;

        // Bytes 0..20: Reserved (DOS drive/cluster/search info). Clear to 0.
        for i in 0..21u16 {
            bus.write_u8(dta_seg, dta_off.wrapping_add(i), 0);
        }
        // Byte 21 (0x15): File attributes. 0x20 = Archive
        bus.write_u8(dta_seg, dta_off.wrapping_add(21), 0x20);

        // Bytes 22..23 (0x16..0x17): Time (12:00:00 -> 0x6000)
        bus.write_u8(dta_seg, dta_off.wrapping_add(22), 0x00);
        bus.write_u8(dta_seg, dta_off.wrapping_add(23), 0x60);

        // Bytes 24..25 (0x18..0x19): Date (1988-11-01 -> 0x1161)
        bus.write_u8(dta_seg, dta_off.wrapping_add(24), 0x61);
        bus.write_u8(dta_seg, dta_off.wrapping_add(25), 0x11);

        // Bytes 26..29 (0x1A..0x1D): File size (32-bit little-endian)
        bus.write_u8(dta_seg, dta_off.wrapping_add(26), (file_len & 0xFF) as u8);
        bus.write_u8(dta_seg, dta_off.wrapping_add(27), ((file_len >> 8) & 0xFF) as u8);
        bus.write_u8(dta_seg, dta_off.wrapping_add(28), ((file_len >> 16) & 0xFF) as u8);
        bus.write_u8(dta_seg, dta_off.wrapping_add(29), ((file_len >> 24) & 0xFF) as u8);

        // Bytes 30..42 (0x1E..0x2A): ASCIIZ 8.3 filename uppercase (max 13 bytes including \0)
        let up_name = name.to_ascii_uppercase();
        let name_bytes = up_name.as_bytes();
        for i in 0..13u16 {
            let b = if (i as usize) < name_bytes.len() {
                name_bytes[i as usize]
            } else {
                0
            };
            bus.write_u8(dta_seg, dta_off.wrapping_add(30 + i), b);
            if b == 0 {
                break;
            }
        }
    }

    pub fn handle_interrupt(&mut self, int_num: u8, cpu: &mut CpuContext, bus: &mut MemoryBus) {
        match int_num {
            0x21 => self.handle_int21(cpu, bus),
            0x10 => self.handle_int10(cpu, bus),
            0x16 => self.handle_int16(cpu, bus),
            0x1A => self.handle_int1a(cpu, bus),
            0x11 => {
                // Equipment list: 80x25 color, 1 diskette drive
                cpu.ax = 0x0021;
            }
            0x12 => {
                // Get memory size: 640 KB
                cpu.ax = 640;
            }
            0x20 => {
                // INT 20h: Terminate program
                println!("[DOS] Program exited via INT 20h");
                cpu.halted = true;
            }
            _ => {
                // Unknown or unhandled interrupt stub
            }
        }
    }

    fn handle_int21(&mut self, cpu: &mut CpuContext, bus: &mut MemoryBus) {
        let ah = cpu.ah();
        match ah {
            0x30 => {
                // Get DOS Version: MS-DOS 5.0
                cpu.set_al(5);
                cpu.set_ah(0);
                cpu.bx = 0xFF00;
                cpu.cx = 0x0000;
            }
            0x48 => {
                // Allocate Memory: BX = paragraphs requested
                let paras = cpu.bx;
                let seg = self.heap_next_seg;
                self.heap_next_seg = self.heap_next_seg.wrapping_add(paras + 1);
                cpu.ax = seg;
                cpu.flags.cf = false;
            }
            0x49 => {
                // Free Memory: ES = segment
                cpu.flags.cf = false;
            }
            0x4A => {
                // Resize Memory Block: ES = segment, BX = new size
                cpu.flags.cf = false;
            }
            0x3D => {
                // Open File: DS:DX = filename, AL = access mode
                let filename = self.read_asciiz(bus, cpu.ds, cpu.dx);
                let path = self.resolve_path(&filename);
                println!("[DOS INT 21h] Open file: {:?} -> {:?}", filename, path);

                let read_only = (cpu.al() & 3) == 0;
                let open_result = if read_only {
                    File::open(&path)
                } else {
                    OpenOptions::new().read(true).write(true).open(&path)
                };

                match open_result {
                    Ok(file) => {
                        let handle = self.next_handle;
                        self.next_handle += 1;
                        self.open_files.insert(handle, file);
                        cpu.ax = handle;
                        cpu.flags.cf = false;
                    }
                    Err(e) => {
                        eprintln!("[DOS] Failed to open {:?}: {}", path, e);
                        cpu.ax = 2; // File not found
                        cpu.flags.cf = true;
                    }
                }
            }
            0x3E => {
                // Close File: BX = handle
                let handle = cpu.bx;
                self.open_files.remove(&handle);
                cpu.flags.cf = false;
            }
            0x3F => {
                // Read File: BX = handle, CX = count, DS:DX = buffer
                let handle = cpu.bx;
                let count = cpu.cx as usize;
                let buf_seg = cpu.ds;
                let mut buf_off = cpu.dx;

                if let Some(file) = self.open_files.get_mut(&handle) {
                    let mut temp = vec![0u8; count];
                    match file.read(&mut temp) {
                        Ok(bytes_read) => {
                            for &b in &temp[..bytes_read] {
                                bus.write_u8(buf_seg, buf_off, b);
                                buf_off = buf_off.wrapping_add(1);
                            }
                            cpu.ax = bytes_read as u16;
                            cpu.flags.cf = false;
                        }
                        Err(_) => {
                            cpu.ax = 0;
                            cpu.flags.cf = true;
                        }
                    }
                } else {
                    cpu.ax = 6; // Invalid handle
                    cpu.flags.cf = true;
                }
            }
            0x40 => {
                // Write File / Stdout: BX = handle, CX = count, DS:DX = buffer
                let handle = cpu.bx;
                let count = cpu.cx as usize;
                let buf_seg = cpu.ds;
                let mut buf_off = cpu.dx;

                let mut bytes = Vec::with_capacity(count);
                for _ in 0..count {
                    bytes.push(bus.read_u8(buf_seg, buf_off));
                    buf_off = buf_off.wrapping_add(1);
                }

                if handle == 1 || handle == 2 {
                    let s = String::from_utf8_lossy(&bytes);
                    print!("{}", s);
                    cpu.ax = count as u16;
                    cpu.flags.cf = false;
                } else if let Some(file) = self.open_files.get_mut(&handle) {
                    match file.write(&bytes) {
                        Ok(written) => {
                            cpu.ax = written as u16;
                            cpu.flags.cf = false;
                        }
                        Err(_) => {
                            cpu.flags.cf = true;
                        }
                    }
                } else {
                    cpu.flags.cf = true;
                }
            }
            0x42 => {
                // Lseek: BX = handle, AL = origin, CX:DX = offset
                let handle = cpu.bx;
                let origin = cpu.al();
                let offset = (((cpu.cx as i32) << 16) | (cpu.dx as i32)) as i64;

                let seek_from = match origin {
                    0 => SeekFrom::Start(offset as u64),
                    1 => SeekFrom::Current(offset),
                    2 => SeekFrom::End(offset),
                    _ => SeekFrom::Start(0),
                };

                if let Some(file) = self.open_files.get_mut(&handle) {
                    match file.seek(seek_from) {
                        Ok(new_pos) => {
                            cpu.dx = (new_pos >> 16) as u16;
                            cpu.ax = (new_pos & 0xFFFF) as u16;
                            cpu.flags.cf = false;
                        }
                        Err(_) => {
                            cpu.flags.cf = true;
                        }
                    }
                } else {
                    cpu.flags.cf = true;
                }
            }
            0x4C => {
                // Exit Program: AL = exit code
                println!("[DOS] Program exited with code {}", cpu.al());
                cpu.halted = true;
            }
            0x09 => {
                // Print String terminated by '$': DS:DX
                let mut off = cpu.dx;
                let mut out = Vec::new();
                loop {
                    let b = bus.read_u8(cpu.ds, off);
                    if b == b'$' {
                        break;
                    }
                    out.push(b);
                    off = off.wrapping_add(1);
                }
                print!("{}", String::from_utf8_lossy(&out));
            }
            0x01 => {
                // Read character from STDIN with echo (blocking)
                if let Some((ascii, _)) = self.key_queue.pop_front() {
                    cpu.set_al(ascii);
                } else {
                    cpu.ip = cpu.ip.wrapping_sub(2);
                }
            }
            0x06 => {
                // Direct console I/O
                let dl = cpu.dl();
                if dl == 0xFF {
                    // Non-blocking input
                    if let Some((ascii, _)) = self.key_queue.pop_front() {
                        cpu.set_al(ascii);
                        cpu.flags.zf = false;
                    } else {
                        cpu.set_al(0);
                        cpu.flags.zf = true;
                    }
                } else {
                    print!("{}", dl as char);
                }
            }
            0x07 | 0x08 => {
                // Direct console input without echo (blocking)
                if let Some((ascii, _)) = self.key_queue.pop_front() {
                    cpu.set_al(ascii);
                } else {
                    cpu.ip = cpu.ip.wrapping_sub(2);
                }
            }
            0x0B => {
                // Check STDIN status
                cpu.set_al(if self.key_queue.is_empty() { 0x00 } else { 0xFF });
            }
            0x0C => {
                // Clear buffer and invoke input function in AL
                self.key_queue.clear();
                let func = cpu.al();
                cpu.set_ah(func);
                self.handle_int21(cpu, bus);
            }
            0x1A => {
                // Set DTA: DS:DX
                self.dta_seg = cpu.ds;
                self.dta_off = cpu.dx;
            }
            0x25 => {
                // Set Interrupt Vector: AL = int, DS:DX = handler
                let int_num = cpu.al();
                println!("[DOS INT 21h] Set Interrupt Vector 0x{:02X} -> {:04X}:{:04X}", int_num, cpu.ds, cpu.dx);
                if int_num == 0x08 || int_num == 0x1C {
                    self.timer_isr = Some((cpu.ds, cpu.dx));
                }
                let ivt_phys = (int_num as usize) * 4;
                bus.ram[ivt_phys] = cpu.dx as u8;
                bus.ram[ivt_phys + 1] = (cpu.dx >> 8) as u8;
                bus.ram[ivt_phys + 2] = cpu.ds as u8;
                bus.ram[ivt_phys + 3] = (cpu.ds >> 8) as u8;
            }
            0x35 => {
                // Get Interrupt Vector: AL = int -> ES:BX
                let ivt_phys = (cpu.al() as usize) * 4;
                let off = u16::from_le_bytes([bus.ram[ivt_phys], bus.ram[ivt_phys + 1]]);
                let seg = u16::from_le_bytes([bus.ram[ivt_phys + 2], bus.ram[ivt_phys + 3]]);
                cpu.es = seg;
                cpu.bx = off;
            }
            0x3C => {
                // Create / Truncate File: DS:DX = filename, CX = attribute
                let filename = self.read_asciiz(bus, cpu.ds, cpu.dx);
                let path = self.resolve_path(&filename);
                println!("[DOS INT 21h] Create file: {:?} -> {:?}", filename, path);
                match OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&path)
                {
                    Ok(file) => {
                        let handle = self.next_handle;
                        self.next_handle += 1;
                        self.open_files.insert(handle, file);
                        cpu.ax = handle;
                        cpu.flags.cf = false;
                    }
                    Err(e) => {
                        eprintln!("[DOS] Failed to create {:?}: {}", path, e);
                        cpu.ax = 3; // Path not found
                        cpu.flags.cf = true;
                    }
                }
            }
            0x41 => {
                // Delete File: DS:DX = filename
                let filename = self.read_asciiz(bus, cpu.ds, cpu.dx);
                let path = self.resolve_path(&filename);
                println!("[DOS INT 21h] Delete file: {:?} -> {:?}", filename, path);
                if path.exists() {
                    let _ = std::fs::remove_file(&path);
                }
                cpu.ax = 0;
                cpu.flags.cf = false;
            }
            0x43 => {
                // Get / Set File Attributes: DS:DX = filename, AL = subfunction
                let filename = self.read_asciiz(bus, cpu.ds, cpu.dx);
                let path = self.resolve_path(&filename);
                if cpu.al() == 0x00 {
                    // Get attributes
                    if path.exists() {
                        cpu.cx = 0x20; // Archive
                        cpu.ax = 0x20;
                        cpu.flags.cf = false;
                    } else {
                        cpu.ax = 2; // File not found
                        cpu.flags.cf = true;
                    }
                } else {
                    // Set attributes (AL = 0x01)
                    cpu.flags.cf = false;
                }
            }
            0x44 => {
                // IOCTL: AL = subfunction, BX = handle
                let handle = cpu.bx;
                if cpu.al() == 0x00 {
                    // Get device information
                    if handle == 0 {
                        cpu.dx = 0x80D3; // Stdin char device
                        cpu.ax = 0x80D3;
                        cpu.flags.cf = false;
                    } else if handle == 1 || handle == 2 {
                        cpu.dx = 0x80D2; // Stdout/err char device
                        cpu.ax = 0x80D2;
                        cpu.flags.cf = false;
                    } else if self.open_files.contains_key(&handle) {
                        cpu.dx = 0x0002; // Disk file (block device)
                        cpu.ax = 0x0002;
                        cpu.flags.cf = false;
                    } else {
                        cpu.ax = 6; // Invalid handle
                        cpu.flags.cf = true;
                    }
                } else {
                    cpu.flags.cf = false;
                }
            }
            0x4E => {
                // Find First File: DS:DX = pattern, CX = attribute mask
                let pattern = self.read_asciiz(bus, cpu.ds, cpu.dx);
                let clean = pattern.replace('\\', "/");
                let file_pat = Path::new(&clean)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or(&pattern);

                let mut matches = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&self.asset_dir) {
                    for entry in entries.flatten() {
                        if let Ok(name) = entry.file_name().into_string() {
                            if dos_matches_pattern(file_pat, &name) {
                                matches.push((name, entry.path()));
                            }
                        }
                    }
                }
                matches.sort_by(|a, b| a.0.cmp(&b.0));

                if !matches.is_empty() {
                    println!(
                        "[DOS INT 21h] FindFirst \"{}\" -> Found \"{}\" ({} matches)",
                        pattern, matches[0].0, matches.len()
                    );
                    self.find_matches = matches;
                    self.find_index = 0;
                    self.write_dta_entry(bus, 0);
                    cpu.ax = 0;
                    cpu.flags.cf = false;
                } else {
                    println!("[DOS INT 21h] FindFirst \"{}\" -> NOT FOUND", pattern);
                    self.find_matches.clear();
                    self.find_index = 0;
                    cpu.ax = 18; // No more files / file not found
                    cpu.flags.cf = true;
                }
            }
            0x4F => {
                // Find Next File
                if self.find_index + 1 < self.find_matches.len() {
                    self.find_index += 1;
                    self.write_dta_entry(bus, self.find_index);
                    println!(
                        "[DOS INT 21h] FindNext -> Found \"{}\"",
                        self.find_matches[self.find_index].0
                    );
                    cpu.ax = 0;
                    cpu.flags.cf = false;
                } else {
                    println!("[DOS INT 21h] FindNext -> End of list");
                    cpu.ax = 18; // No more files
                    cpu.flags.cf = true;
                }
            }
            _ => {
                println!("[DOS INT 21h] Unhandled function AH=0x{:02X}, AL=0x{:02X}", ah, cpu.al());
                cpu.flags.cf = false;
            }
        }
    }

    fn handle_int10(&mut self, cpu: &mut CpuContext, bus: &mut MemoryBus) {
        let ah = cpu.ah();
        match ah {
            0x00 => {
                // Set Video Mode: AL = mode
                bus.video_mode = cpu.al();
                println!("[BIOS INT 10h] Set video mode: 0x{:02X}", bus.video_mode);
            }
            0x0F => {
                // Get Video Mode: AL = mode, AH = cols, BH = page
                cpu.set_al(bus.video_mode);
                cpu.set_ah(40);
                cpu.set_bh(0);
            }
            0x10 => {
                // Set Palette
                cpu.flags.cf = false;
            }
            _ => {}
        }
    }

    fn handle_int16(&mut self, cpu: &mut CpuContext, _bus: &mut MemoryBus) {
        let ah = cpu.ah();
        match ah {
            0x00 | 0x10 => {
                // Read keystroke (blocking): wait for key without advancing IP
                if let Some((ascii, scan)) = self.key_queue.pop_front() {
                    cpu.set_al(ascii);
                    cpu.set_ah(scan);
                } else {
                    cpu.ip = cpu.ip.wrapping_sub(2);
                }
            }
            0x01 | 0x11 => {
                // Check keystroke (non-blocking)
                if let Some(&(ascii, scan)) = self.key_queue.front() {
                    cpu.flags.zf = false;
                    cpu.set_al(ascii);
                    cpu.set_ah(scan);
                } else {
                    cpu.flags.zf = true; // No key waiting
                }
            }
            _ => {}
        }
    }

    fn handle_int1a(&mut self, cpu: &mut CpuContext, _bus: &mut MemoryBus) {
        if cpu.ah() == 0x00 {
            // Read system timer counter
            let ticks = self.timer_ticks;
            cpu.cx = (ticks >> 16) as u16;
            cpu.dx = (ticks & 0xFFFF) as u16;
            cpu.set_al(0); // Midnight flag
        }
    }
}
