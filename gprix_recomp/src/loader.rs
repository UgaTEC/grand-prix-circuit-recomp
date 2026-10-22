/// MS-DOS MZ Executable Loader and Relocator.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::bus::MemoryBus;
use crate::cpu::CpuContext;

#[derive(Debug, Clone)]
pub struct MzHeader {
    pub cblp: u16,
    pub cp: u16,
    pub crlc: u16,
    pub cparhdr: u16,
    pub minalloc: u16,
    pub maxalloc: u16,
    pub ss: u16,
    pub sp: u16,
    pub csum: u16,
    pub ip: u16,
    pub cs: u16,
    pub lfarlc: u16,
    pub ovno: u16,
}

pub struct DosProgram {
    pub psp_segment: u16,
    pub load_segment: u16,
    pub initial_cs: u16,
    pub initial_ip: u16,
    pub initial_ss: u16,
    pub initial_sp: u16,
}

pub fn load_mz_executable<P: AsRef<Path>>(
    path: P,
    bus: &mut MemoryBus,
    cpu: &mut CpuContext,
    psp_segment: u16,
) -> std::io::Result<DosProgram> {
    let mut file = File::open(path)?;
    let mut hdr_bytes = [0u8; 28];
    file.read_exact(&mut hdr_bytes)?;

    if &hdr_bytes[0..2] != b"MZ" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Not an MZ executable",
        ));
    }

    let read_u16 = |idx: usize| -> u16 {
        u16::from_le_bytes([hdr_bytes[idx], hdr_bytes[idx + 1]])
    };

    let hdr = MzHeader {
        cblp: read_u16(2),
        cp: read_u16(4),
        crlc: read_u16(6),
        cparhdr: read_u16(8),
        minalloc: read_u16(10),
        maxalloc: read_u16(12),
        ss: read_u16(14),
        sp: read_u16(16),
        csum: read_u16(18),
        ip: read_u16(20),
        cs: read_u16(22),
        lfarlc: read_u16(24),
        ovno: read_u16(26),
    };

    let load_segment = psp_segment + 0x10; // 256 bytes (16 paragraphs) for PSP
    let header_size = (hdr.cparhdr as usize) * 16;
    let file_len = file.seek(SeekFrom::End(0))?;

    // Check if executable is compressed with Microsoft EXEPACK (signature 0x4252 / 'RB')
    let exepack_offset = (hdr.cparhdr as usize + hdr.cs as usize) * 16;
    let is_exepack = if (file_len as usize) >= exepack_offset + 18 {
        file.seek(SeekFrom::Start(exepack_offset as u64))?;
        let mut eh_bytes = [0u8; 18];
        file.read_exact(&mut eh_bytes)?;
        let sig = u16::from_le_bytes([eh_bytes[16], eh_bytes[17]]);
        let skip_len = u16::from_le_bytes([eh_bytes[14], eh_bytes[15]]);
        sig == 0x4252 || skip_len == 0x4252
    } else {
        false
    };

    let (real_cs, real_ip, real_ss, real_sp) = if is_exepack {
        println!("[LOADER] Detected Microsoft EXEPACK compression. Decompressing in-memory...");
        file.seek(SeekFrom::Start(0))?;
        let mut full_file = vec![0u8; file_len as usize];
        file.read_exact(&mut full_file)?;

        let (unpacked, relocs, r_cs, r_ip, r_ss, r_sp) =
            unpack_exepack(&full_file, &hdr, exepack_offset)?;

        println!(
            "[LOADER] EXEPACK decompressed {} bytes, applied {} relocations.",
            unpacked.len(),
            relocs.len()
        );

        let load_phys = MemoryBus::phys_addr(load_segment, 0);
        bus.ram[load_phys..load_phys + unpacked.len()].copy_from_slice(&unpacked);

        // Apply relocations
        for (rel_seg, rel_off) in relocs {
            let target_seg = load_segment + rel_seg;
            let original_word = bus.read_u16(target_seg, rel_off);
            bus.write_u16(target_seg, rel_off, original_word.wrapping_add(load_segment));
        }

        (r_cs, r_ip, r_ss, r_sp)
    } else {
        // Determine load image size
        let image_size = if hdr.cp > 0 {
            let total_pages_size = (hdr.cp as usize) * 512;
            let last_page_bytes = if hdr.cblp > 0 {
                512 - (hdr.cblp as usize)
            } else {
                0
            };
            (total_pages_size - last_page_bytes).saturating_sub(header_size)
        } else {
            (file_len as usize).saturating_sub(header_size)
        };

        // Load payload image into memory
        file.seek(SeekFrom::Start(header_size as u64))?;
        let mut image = vec![0u8; image_size];
        file.read_exact(&mut image)?;

        let load_phys = MemoryBus::phys_addr(load_segment, 0);
        bus.ram[load_phys..load_phys + image_size].copy_from_slice(&image);

        // Apply relocations
        file.seek(SeekFrom::Start(hdr.lfarlc as u64))?;
        for _ in 0..hdr.crlc {
            let mut reloc_entry = [0u8; 4];
            file.read_exact(&mut reloc_entry)?;
            let rel_off = u16::from_le_bytes([reloc_entry[0], reloc_entry[1]]);
            let rel_seg = u16::from_le_bytes([reloc_entry[2], reloc_entry[3]]);

            let target_seg = load_segment + rel_seg;
            let original_word = bus.read_u16(target_seg, rel_off);
            bus.write_u16(target_seg, rel_off, original_word.wrapping_add(load_segment));
        }

        (hdr.cs, hdr.ip, hdr.ss, hdr.sp)
    };

    // Initialize PSP (Program Segment Prefix)
    let psp_phys = MemoryBus::phys_addr(psp_segment, 0);
    bus.ram[psp_phys] = 0xCD;     // INT 20h
    bus.ram[psp_phys + 1] = 0x20;
    bus.write_u16(psp_segment, 0x02, 0x9FFF); // Top of conventional memory (640KB)
    bus.write_u16(psp_segment, 0x2C, psp_segment.wrapping_sub(0x10)); // Environment segment
    bus.ram[psp_phys + 0x80] = 0; // Empty command tail length
    bus.ram[psp_phys + 0x81] = 0x0D; // CR

    // Set initial CPU registers
    cpu.cs = load_segment + real_cs;
    cpu.ip = real_ip;
    cpu.ss = load_segment + real_ss;
    cpu.sp = real_sp;
    cpu.ds = psp_segment;
    cpu.es = psp_segment;
    cpu.ax = 0;
    cpu.bx = 0;
    cpu.cx = 0;
    cpu.dx = psp_segment;
    cpu.bp = 0;
    cpu.si = 0;
    cpu.di = 0;
    cpu.flags.if_ = true;

    Ok(DosProgram {
        psp_segment,
        load_segment,
        initial_cs: cpu.cs,
        initial_ip: cpu.ip,
        initial_ss: cpu.ss,
        initial_sp: cpu.sp,
    })
}

/// Unpack Microsoft EXEPACK compressed executable in-memory.
fn unpack_exepack(
    file_bytes: &[u8],
    hdr: &MzHeader,
    exepack_offset: usize,
) -> std::io::Result<(Vec<u8>, Vec<(u16, u16)>, u16, u16, u16, u16)> {
    if file_bytes.len() < exepack_offset + 18 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Truncated EXEPACK header",
        ));
    }

    let read_u16 = |o: usize| u16::from_le_bytes([file_bytes[o], file_bytes[o + 1]]);
    let real_ip = read_u16(exepack_offset);
    let real_cs = read_u16(exepack_offset + 2);
    let real_sp = read_u16(exepack_offset + 8);
    let real_ss = read_u16(exepack_offset + 10);
    let dest_len = (read_u16(exepack_offset + 12) as usize) * 16;

    let packed_start = (hdr.cparhdr as usize) * 16;
    let packed_end = exepack_offset;
    if packed_start >= packed_end || packed_end > file_bytes.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid EXEPACK offsets",
        ));
    }

    let mut packed_data = file_bytes[packed_start..packed_end].to_vec();
    packed_data.reverse();

    let mut unpacked = Vec::with_capacity(dest_len);
    let mut idx = 0;
    while idx < packed_data.len() && packed_data[idx] == 0xFF {
        idx += 1;
    }

    while idx < packed_data.len() {
        let opcode = packed_data[idx];
        idx += 1;
        if idx + 1 >= packed_data.len() {
            break;
        }
        let count = (packed_data[idx] as usize) * 256 + (packed_data[idx + 1] as usize);
        idx += 2;

        let op_type = opcode & 0xFE;
        if op_type == 0xB0 {
            if idx >= packed_data.len() {
                break;
            }
            let fill_byte = packed_data[idx];
            idx += 1;
            unpacked.resize(unpacked.len() + count, fill_byte);
        } else if op_type == 0xB2 {
            if idx + count > packed_data.len() {
                break;
            }
            unpacked.extend_from_slice(&packed_data[idx..idx + count]);
            idx += count;
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unknown EXEPACK opcode: 0x{:02X}", opcode),
            ));
        }

        if (opcode & 1) == 1 {
            break;
        }
    }

    if idx < packed_data.len() {
        unpacked.extend_from_slice(&packed_data[idx..]);
    }
    unpacked.reverse();

    // Parse relocations from stub
    let stub = &file_bytes[exepack_offset..];
    let corrupt_str = b"Packed file is corrupt";
    let corrupt_pos = stub
        .windows(corrupt_str.len())
        .position(|w| w == corrupt_str)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Could not find EXEPACK relocation marker",
            )
        })?;

    let reloc_offset = exepack_offset + corrupt_pos + corrupt_str.len();
    let mut relocs = Vec::new();
    let mut r_idx = reloc_offset;

    for i in 0..16 {
        if r_idx + 2 > file_bytes.len() {
            break;
        }
        let count = read_u16(r_idx) as usize;
        r_idx += 2;
        let seg = ((i * 0x1000) & 0xFFFF) as u16;

        for _ in 0..count {
            if r_idx + 2 > file_bytes.len() {
                break;
            }
            let off = read_u16(r_idx);
            r_idx += 2;
            relocs.push((seg, off));
        }
    }

    Ok((unpacked, relocs, real_cs, real_ip, real_ss, real_sp))
}
