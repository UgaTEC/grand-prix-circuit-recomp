import struct
import sys
import os

def unpack_exepack(input_path, output_path):
    with open(input_path, 'rb') as f:
        data = bytearray(f.read())
    
    if len(data) < 64 or data[:2] != b'MZ':
        raise ValueError("Not a valid MZ executable")
    
    # DOS Header
    (e_magic, e_cblp, e_cp, e_crlc, e_cparhdr, e_minalloc, e_maxalloc,
     e_ss, e_sp, e_csum, e_ip, e_cs, e_lfarlc, e_ovno) = struct.unpack('<14H', data[:28])
    
    exepack_offset = (e_cparhdr + e_cs) * 16
    print(f"EXEPACK header offset: 0x{exepack_offset:05X}")
    
    # Read EXEPACK header (18 bytes)
    # real_ip, real_cs, mem_start, exepack_size, real_sp, real_ss, dest_len, skip_len, signature
    eh_bytes = data[exepack_offset:exepack_offset+18]
    if len(eh_bytes) < 18:
        raise ValueError("File truncated at EXEPACK header")
    
    (real_ip, real_cs, mem_start, exepack_size, real_sp, real_ss,
     dest_len, skip_len, signature) = struct.unpack('<9H', eh_bytes)
    
    print(f"Signature: 0x{signature:04X} ('{eh_bytes[16:18].decode('latin1', errors='replace')}')")
    print(f"Real CS:IP = {real_cs:04X}:{real_ip:04X}")
    print(f"Real SS:SP = {real_ss:04X}:{real_sp:04X}")
    print(f"Dest len: {dest_len} paras ({dest_len * 16} bytes)")
    print(f"EXEPACK stub size: {exepack_size}")
    
    if signature != 0x4252 and skip_len != 0x4252:
        raise ValueError("Not an EXEPACK file (missing 'RB' signature)")
    
    packed_start = e_cparhdr * 16
    packed_end = exepack_offset
    packed_data = bytearray(data[packed_start:packed_end])
    packed_data.reverse()
    
    unpacked_max_size = dest_len * 16
    unpacked = bytearray()
    
    idx = 0
    while idx < len(packed_data) and packed_data[idx] == 0xFF:
        idx += 1
        
    while idx < len(packed_data):
        opcode = packed_data[idx]
        idx += 1
        count = packed_data[idx] * 256 + packed_data[idx + 1]
        idx += 2
        
        op_type = opcode & 0xFE
        if op_type == 0xB0:
            fill_byte = packed_data[idx]
            idx += 1
            unpacked.extend(bytes([fill_byte]) * count)
        elif op_type == 0xB2:
            chunk = packed_data[idx:idx + count]
            idx += count
            unpacked.extend(chunk)
        else:
            raise ValueError(f"Unknown EXEPACK opcode: 0x{opcode:02X}")
            
        if (opcode & 1) == 1:
            break
            
    # Remainder raw data
    if idx < len(packed_data):
        unpacked.extend(packed_data[idx:])
        
    unpacked.reverse()
    print(f"Unpacked code/data size: {len(unpacked)} bytes")
    
    # Relocation table
    stub = data[exepack_offset:]
    corrupt_str = b"Packed file is corrupt"
    corrupt_idx = stub.find(corrupt_str)
    if corrupt_idx == -1:
        raise ValueError("Cannot find 'Packed file is corrupt' string")
        
    reloc_offset = exepack_offset + corrupt_idx + len(corrupt_str)
    relocs = []
    
    r_idx = reloc_offset
    for i in range(16):
        count = struct.unpack('<H', data[r_idx:r_idx+2])[0]
        r_idx += 2
        seg = (i * 0x1000) & 0xFFFF
        for _ in range(count):
            entry_off = struct.unpack('<H', data[r_idx:r_idx+2])[0]
            r_idx += 2
            relocs.append((entry_off, seg))
            
    print(f"Extracted {len(relocs)} relocations")
    
    # Build new MZ executable
    reloc_bytes = bytearray()
    for off, seg in relocs:
        reloc_bytes.extend(struct.pack('<HH', off, seg))
        
    header_size = 28 + len(reloc_bytes)
    # Align header to paragraph (16 bytes) or 32 paragraphs
    cparhdr = (header_size + 15) // 16
    cparhdr = ((cparhdr // 32) + 1) * 32
    padding = (cparhdr * 16) - (28 + len(reloc_bytes))
    
    total_len = (cparhdr * 16) + len(unpacked)
    cblp = total_len % 512
    cp = (total_len + 511) // 512
    
    new_hdr = struct.pack(
        '<2s13H',
        b'MZ',
        cblp,
        cp,
        len(relocs),
        cparhdr,
        e_minalloc,
        0xFFFF,
        real_ss,
        real_sp,
        0, # csum
        real_ip,
        real_cs,
        28, # reloc table right after header
        0
    )
    
    with open(output_path, 'wb') as out_f:
        out_f.write(new_hdr)
        out_f.write(reloc_bytes)
        out_f.write(b'\x00' * padding)
        out_f.write(unpacked)
        
    print(f"Successfully wrote unpacked executable to {output_path} ({os.path.getsize(output_path)} bytes)")

if __name__ == '__main__':
    unpack_exepack(sys.argv[1], sys.argv[2])
