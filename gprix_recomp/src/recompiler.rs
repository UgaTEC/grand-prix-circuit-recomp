/// x86 16-bit block decoder and execution engine with complete 8086/80286 instruction set.

use iced_x86::{
    Decoder, DecoderOptions, Formatter, Instruction, IntelFormatter, Mnemonic, OpKind, Register,
};

use crate::bus::MemoryBus;
use crate::cpu::CpuContext;
use crate::dos::DosEnvironment;

pub struct Recompiler {
    pub cs: u16,
}

impl Recompiler {
    pub fn new(cs: u16) -> Self {
        Self { cs }
    }

    /// Disassemble and print instructions from the given CS:IP.
    pub fn dump_instructions(&self, bus: &mut MemoryBus, start_ip: u16, count: usize) {
        let code_phys = MemoryBus::phys_addr(self.cs, 0);
        let max_len = 65536 - (start_ip as usize);
        let slice = &bus.ram[code_phys + (start_ip as usize)
            ..code_phys + (start_ip as usize) + max_len.min(512)];

        let mut decoder = Decoder::with_ip(16, slice, start_ip as u64, DecoderOptions::NONE);
        let mut formatter = IntelFormatter::new();
        let mut output = String::new();
        let mut instr = Instruction::default();

        println!("=== Disassembly at {:04X}:{:04X} ===", self.cs, start_ip);
        for _ in 0..count {
            if !decoder.can_decode() {
                break;
            }
            decoder.decode_out(&mut instr);
            output.clear();
            formatter.format(&instr, &mut output);
            println!("{:04X}:{:04X}  {}", self.cs, instr.ip() as u16, output);

            if instr.mnemonic() == Mnemonic::Jmp
                || instr.mnemonic() == Mnemonic::Ret
                || instr.mnemonic() == Mnemonic::Retf
            {
                break;
            }
        }
    }

    /// Step and execute one instruction at current CPU CS:IP.
    pub fn step(&self, cpu: &mut CpuContext, bus: &mut MemoryBus, dos: &mut DosEnvironment) -> bool {
        if cpu.halted {
            return false;
        }

        // Check if game engine just completed rendering a frame (wait function at 1010:681E)
        if cpu.cs == self.cs && cpu.ip == 0x681E && bus.ega_dirty {
            bus.on_engine_frame_boundary();
        }

        let code_phys = MemoryBus::phys_addr(cpu.cs, 0);
        let ip = cpu.ip as usize;
        let slice = &bus.ram[code_phys + ip..code_phys + ip + 15.min(65536 - ip)];

        let mut decoder = Decoder::with_ip(16, slice, cpu.ip as u64, DecoderOptions::NONE);
        let instr = decoder.decode();

        if instr.is_invalid() {
            eprintln!(
                "[CPU ERROR] Invalid instruction at {:04X}:{:04X} (byte=0x{:02X})",
                cpu.cs, cpu.ip, bus.ram[code_phys + ip]
            );
            cpu.halted = true;
            return false;
        }

        let next_ip = (cpu.ip as usize + instr.len()) as u16;
        cpu.ip = next_ip;

        self.execute_instruction(&instr, cpu, bus, dos);
        true
    }

    fn is_byte(&self, instr: &Instruction, op_idx: u32) -> bool {
        match instr.op_kind(op_idx) {
            OpKind::Register => {
                let r = instr.op_register(op_idx);
                matches!(
                    r,
                    Register::AL
                        | Register::AH
                        | Register::BL
                        | Register::BH
                        | Register::CL
                        | Register::CH
                        | Register::DL
                        | Register::DH
                )
            }
            OpKind::Memory => instr.memory_size().size() == 1,
            _ => {
                if instr.op_count() > 0 && instr.op_kind(0) == OpKind::Register {
                    self.is_byte(instr, 0)
                } else {
                    false
                }
            }
        }
    }

    fn execute_instruction(
        &self,
        instr: &Instruction,
        cpu: &mut CpuContext,
        bus: &mut MemoryBus,
        dos: &mut DosEnvironment,
    ) {
        let is_byte = self.is_byte(instr, 0);

        match instr.mnemonic() {
            Mnemonic::Nop => {}
            Mnemonic::Cli => cpu.flags.if_ = false,
            Mnemonic::Sti => cpu.flags.if_ = true,
            Mnemonic::Cld => cpu.flags.df = false,
            Mnemonic::Std => cpu.flags.df = true,
            Mnemonic::Clc => cpu.flags.cf = false,
            Mnemonic::Stc => cpu.flags.cf = true,
            Mnemonic::Cmc => cpu.flags.cf = !cpu.flags.cf,

            Mnemonic::Int => {
                let int_num = instr.immediate8();
                dos.handle_interrupt(int_num, cpu, bus);
            }

            Mnemonic::Mov => {
                let val = self.read_op(instr, 1, is_byte, cpu, bus);
                self.write_op(instr, 0, val, is_byte, cpu, bus);
            }

            Mnemonic::Lea => {
                let (_, off) = self.resolve_mem(instr, cpu);
                self.write_op(instr, 0, off, false, cpu, bus);
            }

            Mnemonic::Lds => {
                let (seg, off) = self.resolve_mem(instr, cpu);
                let new_off = bus.read_u16(seg, off);
                let new_seg = bus.read_u16(seg, off.wrapping_add(2));
                self.write_op(instr, 0, new_off, false, cpu, bus);
                cpu.ds = new_seg;
            }

            Mnemonic::Les => {
                let (seg, off) = self.resolve_mem(instr, cpu);
                let new_off = bus.read_u16(seg, off);
                let new_seg = bus.read_u16(seg, off.wrapping_add(2));
                self.write_op(instr, 0, new_off, false, cpu, bus);
                cpu.es = new_seg;
            }

            Mnemonic::Xchg => {
                let v0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let v1 = self.read_op(instr, 1, is_byte, cpu, bus);
                self.write_op(instr, 0, v1, is_byte, cpu, bus);
                self.write_op(instr, 1, v0, is_byte, cpu, bus);
            }

            Mnemonic::Cbw => {
                cpu.ax = (cpu.al() as i8 as i16) as u16;
            }
            Mnemonic::Cwd => {
                cpu.dx = if (cpu.ax as i16) < 0 { 0xFFFF } else { 0x0000 };
            }

            Mnemonic::Cmp => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let op1 = self.read_op(instr, 1, is_byte, cpu, bus);
                if is_byte {
                    let (diff, borrow) = (op0 as u8).overflowing_sub(op1 as u8);
                    cpu.flags.cf = borrow;
                    cpu.flags.of = (((op0 as u8) ^ (op1 as u8))
                        & ((op0 as u8) ^ diff)
                        & 0x80)
                        != 0;
                    cpu.update_szp_u8(diff);
                } else {
                    let (diff, borrow) = op0.overflowing_sub(op1);
                    cpu.flags.cf = borrow;
                    cpu.flags.of = ((op0 ^ op1) & (op0 ^ diff) & 0x8000) != 0;
                    cpu.update_szp_u16(diff);
                }
            }

            Mnemonic::Test => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let op1 = self.read_op(instr, 1, is_byte, cpu, bus);
                let res = op0 & op1;
                cpu.flags.cf = false;
                cpu.flags.of = false;
                if is_byte {
                    cpu.update_szp_u8(res as u8);
                } else {
                    cpu.update_szp_u16(res);
                }
            }

            Mnemonic::Inc => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                if is_byte {
                    let res = (op0 as u8).wrapping_add(1);
                    cpu.flags.of = res == 0x80;
                    cpu.update_szp_u8(res);
                    self.write_op(instr, 0, res as u16, true, cpu, bus);
                } else {
                    let res = op0.wrapping_add(1);
                    cpu.flags.of = res == 0x8000;
                    cpu.update_szp_u16(res);
                    self.write_op(instr, 0, res, false, cpu, bus);
                }
            }

            Mnemonic::Dec => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                if is_byte {
                    let res = (op0 as u8).wrapping_sub(1);
                    cpu.flags.of = res == 0x7F;
                    cpu.update_szp_u8(res);
                    self.write_op(instr, 0, res as u16, true, cpu, bus);
                } else {
                    let res = op0.wrapping_sub(1);
                    cpu.flags.of = res == 0x7FFF;
                    cpu.update_szp_u16(res);
                    self.write_op(instr, 0, res, false, cpu, bus);
                }
            }

            Mnemonic::Neg => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                if is_byte {
                    let (res, borrow) = 0u8.overflowing_sub(op0 as u8);
                    cpu.flags.cf = borrow;
                    cpu.flags.of = op0 as u8 == 0x80;
                    cpu.update_szp_u8(res);
                    self.write_op(instr, 0, res as u16, true, cpu, bus);
                } else {
                    let (res, borrow) = 0u16.overflowing_sub(op0);
                    cpu.flags.cf = borrow;
                    cpu.flags.of = op0 == 0x8000;
                    cpu.update_szp_u16(res);
                    self.write_op(instr, 0, res, false, cpu, bus);
                }
            }

            Mnemonic::Not => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                self.write_op(instr, 0, !op0, is_byte, cpu, bus);
            }

            Mnemonic::Add => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let op1 = self.read_op(instr, 1, is_byte, cpu, bus);
                if is_byte {
                    let (res, carry) = (op0 as u8).overflowing_add(op1 as u8);
                    cpu.flags.cf = carry;
                    cpu.flags.of = (!((op0 as u8) ^ (op1 as u8))
                        & ((op0 as u8) ^ res)
                        & 0x80)
                        != 0;
                    cpu.update_szp_u8(res);
                    self.write_op(instr, 0, res as u16, true, cpu, bus);
                } else {
                    let (res, carry) = op0.overflowing_add(op1);
                    cpu.flags.cf = carry;
                    cpu.flags.of = (!(op0 ^ op1) & (op0 ^ res) & 0x8000) != 0;
                    cpu.update_szp_u16(res);
                    self.write_op(instr, 0, res, false, cpu, bus);
                }
            }

            Mnemonic::Adc => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let op1 = self.read_op(instr, 1, is_byte, cpu, bus);
                let c = if cpu.flags.cf { 1 } else { 0 };
                if is_byte {
                    let (res1, c1) = (op0 as u8).overflowing_add(op1 as u8);
                    let (res2, c2) = res1.overflowing_add(c);
                    cpu.flags.cf = c1 || c2;
                    cpu.flags.of = (!((op0 as u8) ^ (op1 as u8)) & ((op0 as u8) ^ res2) & 0x80) != 0;
                    cpu.update_szp_u8(res2);
                    self.write_op(instr, 0, res2 as u16, true, cpu, bus);
                } else {
                    let (res1, c1) = op0.overflowing_add(op1);
                    let (res2, c2) = res1.overflowing_add(c as u16);
                    cpu.flags.cf = c1 || c2;
                    cpu.flags.of = (!(op0 ^ op1) & (op0 ^ res2) & 0x8000) != 0;
                    cpu.update_szp_u16(res2);
                    self.write_op(instr, 0, res2, false, cpu, bus);
                }
            }

            Mnemonic::Sub => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let op1 = self.read_op(instr, 1, is_byte, cpu, bus);
                if is_byte {
                    let (res, borrow) = (op0 as u8).overflowing_sub(op1 as u8);
                    cpu.flags.cf = borrow;
                    cpu.flags.of = (((op0 as u8) ^ (op1 as u8))
                        & ((op0 as u8) ^ res)
                        & 0x80)
                        != 0;
                    cpu.update_szp_u8(res);
                    self.write_op(instr, 0, res as u16, true, cpu, bus);
                } else {
                    let (res, borrow) = op0.overflowing_sub(op1);
                    cpu.flags.cf = borrow;
                    cpu.flags.of = ((op0 ^ op1) & (op0 ^ res) & 0x8000) != 0;
                    cpu.update_szp_u16(res);
                    self.write_op(instr, 0, res, false, cpu, bus);
                }
            }

            Mnemonic::Sbb => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let op1 = self.read_op(instr, 1, is_byte, cpu, bus);
                let b = if cpu.flags.cf { 1 } else { 0 };
                if is_byte {
                    let (res1, b1) = (op0 as u8).overflowing_sub(op1 as u8);
                    let (res2, b2) = res1.overflowing_sub(b);
                    cpu.flags.cf = b1 || b2;
                    cpu.flags.of = (((op0 as u8) ^ (op1 as u8)) & ((op0 as u8) ^ res2) & 0x80) != 0;
                    cpu.update_szp_u8(res2);
                    self.write_op(instr, 0, res2 as u16, true, cpu, bus);
                } else {
                    let (res1, b1) = op0.overflowing_sub(op1);
                    let (res2, b2) = res1.overflowing_sub(b as u16);
                    cpu.flags.cf = b1 || b2;
                    cpu.flags.of = ((op0 ^ op1) & (op0 ^ res2) & 0x8000) != 0;
                    cpu.update_szp_u16(res2);
                    self.write_op(instr, 0, res2, false, cpu, bus);
                }
            }

            Mnemonic::Rol => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let count = (self.read_op(instr, 1, true, cpu, bus) & 0x1F) as u32;
                if count > 0 {
                    if is_byte {
                        let res = (op0 as u8).rotate_left(count);
                        cpu.flags.cf = (res & 1) != 0;
                        self.write_op(instr, 0, res as u16, true, cpu, bus);
                    } else {
                        let res = op0.rotate_left(count);
                        cpu.flags.cf = (res & 1) != 0;
                        self.write_op(instr, 0, res, false, cpu, bus);
                    }
                }
            }

            Mnemonic::Ror => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let count = (self.read_op(instr, 1, true, cpu, bus) & 0x1F) as u32;
                if count > 0 {
                    if is_byte {
                        let res = (op0 as u8).rotate_right(count);
                        cpu.flags.cf = (res & 0x80) != 0;
                        self.write_op(instr, 0, res as u16, true, cpu, bus);
                    } else {
                        let res = op0.rotate_right(count);
                        cpu.flags.cf = (res & 0x8000) != 0;
                        self.write_op(instr, 0, res, false, cpu, bus);
                    }
                }
            }

            Mnemonic::Rcl => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let count = (self.read_op(instr, 1, true, cpu, bus) & 0x1F) as usize;
                if is_byte {
                    let mut val = op0 as u8;
                    for _ in 0..count {
                        let old_cf = if cpu.flags.cf { 1 } else { 0 };
                        cpu.flags.cf = (val & 0x80) != 0;
                        val = (val << 1) | old_cf;
                    }
                    self.write_op(instr, 0, val as u16, true, cpu, bus);
                } else {
                    let mut val = op0;
                    for _ in 0..count {
                        let old_cf = if cpu.flags.cf { 1 } else { 0 };
                        cpu.flags.cf = (val & 0x8000) != 0;
                        val = (val << 1) | old_cf;
                    }
                    self.write_op(instr, 0, val, false, cpu, bus);
                }
            }

            Mnemonic::Rcr => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let count = (self.read_op(instr, 1, true, cpu, bus) & 0x1F) as usize;
                if is_byte {
                    let mut val = op0 as u8;
                    for _ in 0..count {
                        let old_cf = if cpu.flags.cf { 0x80 } else { 0 };
                        cpu.flags.cf = (val & 1) != 0;
                        val = (val >> 1) | old_cf;
                    }
                    self.write_op(instr, 0, val as u16, true, cpu, bus);
                } else {
                    let mut val = op0;
                    for _ in 0..count {
                        let old_cf = if cpu.flags.cf { 0x8000 } else { 0 };
                        cpu.flags.cf = (val & 1) != 0;
                        val = (val >> 1) | old_cf;
                    }
                    self.write_op(instr, 0, val, false, cpu, bus);
                }
            }

            Mnemonic::And => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let op1 = self.read_op(instr, 1, is_byte, cpu, bus);
                let res = op0 & op1;
                cpu.flags.cf = false;
                cpu.flags.of = false;
                if is_byte {
                    cpu.update_szp_u8(res as u8);
                } else {
                    cpu.update_szp_u16(res);
                }
                self.write_op(instr, 0, res, is_byte, cpu, bus);
            }

            Mnemonic::Or => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let op1 = self.read_op(instr, 1, is_byte, cpu, bus);
                let res = op0 | op1;
                cpu.flags.cf = false;
                cpu.flags.of = false;
                if is_byte {
                    cpu.update_szp_u8(res as u8);
                } else {
                    cpu.update_szp_u16(res);
                }
                self.write_op(instr, 0, res, is_byte, cpu, bus);
            }

            Mnemonic::Xor => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let op1 = self.read_op(instr, 1, is_byte, cpu, bus);
                let res = op0 ^ op1;
                cpu.flags.cf = false;
                cpu.flags.of = false;
                if is_byte {
                    cpu.update_szp_u8(res as u8);
                } else {
                    cpu.update_szp_u16(res);
                }
                self.write_op(instr, 0, res, is_byte, cpu, bus);
            }

            Mnemonic::Shl => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let count = (self.read_op(instr, 1, true, cpu, bus) & 0x1F) as u32;
                if count > 0 {
                    if is_byte {
                        let val = op0 as u8;
                        let res = val << count;
                        cpu.flags.cf = ((val >> (8 - count)) & 1) != 0;
                        cpu.update_szp_u8(res);
                        self.write_op(instr, 0, res as u16, true, cpu, bus);
                    } else {
                        let res = op0 << count;
                        cpu.flags.cf = ((op0 >> (16 - count)) & 1) != 0;
                        cpu.update_szp_u16(res);
                        self.write_op(instr, 0, res, false, cpu, bus);
                    }
                }
            }

            Mnemonic::Shr => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let count = (self.read_op(instr, 1, true, cpu, bus) & 0x1F) as u32;
                if count > 0 {
                    if is_byte {
                        let val = op0 as u8;
                        let res = val >> count;
                        cpu.flags.cf = ((val >> (count - 1)) & 1) != 0;
                        cpu.update_szp_u8(res);
                        self.write_op(instr, 0, res as u16, true, cpu, bus);
                    } else {
                        let res = op0 >> count;
                        cpu.flags.cf = ((op0 >> (count - 1)) & 1) != 0;
                        cpu.update_szp_u16(res);
                        self.write_op(instr, 0, res, false, cpu, bus);
                    }
                }
            }

            Mnemonic::Sar => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                let count = (self.read_op(instr, 1, true, cpu, bus) & 0x1F) as u32;
                if count > 0 {
                    if is_byte {
                        let val = op0 as i8;
                        let res = (val >> count) as u8;
                        cpu.flags.cf = ((val >> (count - 1)) & 1) != 0;
                        cpu.update_szp_u8(res);
                        self.write_op(instr, 0, res as u16, true, cpu, bus);
                    } else {
                        let val = op0 as i16;
                        let res = (val >> count) as u16;
                        cpu.flags.cf = ((val >> (count - 1)) & 1) != 0;
                        cpu.update_szp_u16(res);
                        self.write_op(instr, 0, res, false, cpu, bus);
                    }
                }
            }

            Mnemonic::Push => {
                let val = self.read_op(instr, 0, false, cpu, bus);
                bus.push_u16(cpu, val);
            }

            Mnemonic::Pop => {
                let val = bus.pop_u16(cpu);
                self.write_op(instr, 0, val, false, cpu, bus);
            }

            Mnemonic::Pushf => {
                bus.push_u16(cpu, cpu.flags.to_u16());
            }

            Mnemonic::Popf => {
                let f = bus.pop_u16(cpu);
                cpu.flags.from_u16(f);
            }

            Mnemonic::Jmp => {
                if instr.is_jmp_far() {
                    if instr.op0_kind() == OpKind::Memory {
                        let (seg, off) = self.resolve_mem(instr, cpu);
                        let new_ip = bus.read_u16(seg, off);
                        let new_cs = bus.read_u16(seg, off.wrapping_add(2));
                        cpu.cs = new_cs;
                        cpu.ip = new_ip;
                    } else {
                        cpu.cs = instr.far_branch_selector();
                        cpu.ip = instr.far_branch16();
                    }
                } else if instr.op0_kind() == OpKind::Register || instr.op0_kind() == OpKind::Memory {
                    cpu.ip = self.read_op(instr, 0, false, cpu, bus);
                } else {
                    cpu.ip = instr.near_branch16();
                }
            }

            Mnemonic::Call => {
                if instr.is_call_far() {
                    bus.push_u16(cpu, cpu.cs);
                    bus.push_u16(cpu, cpu.ip);
                    if instr.op0_kind() == OpKind::Memory {
                        let (seg, off) = self.resolve_mem(instr, cpu);
                        let new_ip = bus.read_u16(seg, off);
                        let new_cs = bus.read_u16(seg, off.wrapping_add(2));
                        cpu.cs = new_cs;
                        cpu.ip = new_ip;
                    } else {
                        cpu.cs = instr.far_branch_selector();
                        cpu.ip = instr.far_branch16();
                    }
                } else {
                    bus.push_u16(cpu, cpu.ip);
                    if instr.op0_kind() == OpKind::Register || instr.op0_kind() == OpKind::Memory {
                        cpu.ip = self.read_op(instr, 0, false, cpu, bus);
                    } else {
                        cpu.ip = instr.near_branch16();
                    }
                }
            }

            Mnemonic::Ret => {
                cpu.ip = bus.pop_u16(cpu);
                if instr.op_count() > 0 {
                    let imm = instr.immediate16();
                    cpu.sp = cpu.sp.wrapping_add(imm);
                }
            }

            Mnemonic::Retf => {
                cpu.ip = bus.pop_u16(cpu);
                cpu.cs = bus.pop_u16(cpu);
                if instr.op_count() > 0 {
                    let imm = instr.immediate16();
                    cpu.sp = cpu.sp.wrapping_add(imm);
                }
            }

            Mnemonic::Iret => {
                cpu.ip = bus.pop_u16(cpu);
                cpu.cs = bus.pop_u16(cpu);
                let f = bus.pop_u16(cpu);
                cpu.flags.from_u16(f);
            }

            // Conditional jumps
            Mnemonic::Je => { if cpu.flags.zf { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jne => { if !cpu.flags.zf { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jae => { if !cpu.flags.cf { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jb => { if cpu.flags.cf { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Ja => { if !cpu.flags.cf && !cpu.flags.zf { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jbe => { if cpu.flags.cf || cpu.flags.zf { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jg => { if !cpu.flags.zf && (cpu.flags.sf == cpu.flags.of) { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jge => { if cpu.flags.sf == cpu.flags.of { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jl => { if cpu.flags.sf != cpu.flags.of { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jle => { if cpu.flags.zf || (cpu.flags.sf != cpu.flags.of) { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Js => { if cpu.flags.sf { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jns => { if !cpu.flags.sf { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jcxz => { if cpu.cx == 0 { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jp => { if cpu.flags.pf { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jnp => { if !cpu.flags.pf { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jo => { if cpu.flags.of { cpu.ip = instr.near_branch16(); } }
            Mnemonic::Jno => { if !cpu.flags.of { cpu.ip = instr.near_branch16(); } }

            // Processor control & BCD / special instructions
            Mnemonic::Wait => {}
            Mnemonic::Salc => {
                cpu.set_al(if cpu.flags.cf { 0xFF } else { 0x00 });
            }
            Mnemonic::Daa => {
                let mut al = cpu.al();
                let old_cf = cpu.flags.cf;
                let mut cf = false;
                if (al & 0x0F) > 9 || cpu.flags.af {
                    al = al.wrapping_add(6);
                    cpu.flags.af = true;
                } else {
                    cpu.flags.af = false;
                }
                if al > 0x9F || old_cf {
                    al = al.wrapping_add(0x60);
                    cf = true;
                }
                cpu.set_al(al);
                cpu.flags.cf = cf;
                cpu.update_szp_u8(al);
            }
            Mnemonic::Das => {
                let mut al = cpu.al();
                let old_cf = cpu.flags.cf;
                let mut cf = false;
                if (al & 0x0F) > 9 || cpu.flags.af {
                    al = al.wrapping_sub(6);
                    cpu.flags.af = true;
                } else {
                    cpu.flags.af = false;
                }
                if al > 0x9F || old_cf {
                    al = al.wrapping_sub(0x60);
                    cf = true;
                }
                cpu.set_al(al);
                cpu.flags.cf = cf;
                cpu.update_szp_u8(al);
            }
            Mnemonic::Aam => {
                let base = if instr.op_count() > 0 { instr.immediate8() } else { 10 };
                if base != 0 {
                    let al = cpu.al();
                    cpu.set_ah(al / base);
                    cpu.set_al(al % base);
                    cpu.update_szp_u16(cpu.ax);
                }
            }
            Mnemonic::Aad => {
                let base = if instr.op_count() > 0 { instr.immediate8() } else { 10 };
                let al = cpu.al().wrapping_add(cpu.ah().wrapping_mul(base));
                cpu.set_al(al);
                cpu.set_ah(0);
                cpu.update_szp_u16(cpu.ax);
            }
            Mnemonic::Aaa => {
                if (cpu.al() & 0x0F) > 9 || cpu.flags.af {
                    cpu.set_al(cpu.al().wrapping_add(6));
                    cpu.set_ah(cpu.ah().wrapping_add(1));
                    cpu.flags.af = true;
                    cpu.flags.cf = true;
                } else {
                    cpu.flags.af = false;
                    cpu.flags.cf = false;
                }
                cpu.set_al(cpu.al() & 0x0F);
            }
            Mnemonic::Aas => {
                if (cpu.al() & 0x0F) > 9 || cpu.flags.af {
                    cpu.set_al(cpu.al().wrapping_sub(6));
                    cpu.set_ah(cpu.ah().wrapping_sub(1));
                    cpu.flags.af = true;
                    cpu.flags.cf = true;
                } else {
                    cpu.flags.af = false;
                    cpu.flags.cf = false;
                }
                cpu.set_al(cpu.al() & 0x0F);
            }

            // Multiplication and Division
            Mnemonic::Imul => {
                if instr.op_count() == 1 {
                    let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                    if is_byte {
                        let res = (cpu.al() as i8 as i32) * (op0 as i8 as i32);
                        cpu.ax = res as u16;
                        let carry = res != (res as i8 as i32);
                        cpu.flags.cf = carry;
                        cpu.flags.of = carry;
                    } else {
                        let res = (cpu.ax as i16 as i32) * (op0 as i16 as i32);
                        cpu.ax = (res & 0xFFFF) as u16;
                        cpu.dx = ((res >> 16) & 0xFFFF) as u16;
                        let carry = res != (res as i16 as i32);
                        cpu.flags.cf = carry;
                        cpu.flags.of = carry;
                    }
                } else if instr.op_count() == 2 {
                    let src = self.read_op(instr, 1, false, cpu, bus) as i16 as i32;
                    let dst = self.read_op(instr, 0, false, cpu, bus) as i16 as i32;
                    let res = dst * src;
                    self.write_op(instr, 0, (res & 0xFFFF) as u16, false, cpu, bus);
                    let carry = res != (res as i16 as i32);
                    cpu.flags.cf = carry;
                    cpu.flags.of = carry;
                } else if instr.op_count() == 3 {
                    let src = self.read_op(instr, 1, false, cpu, bus) as i16 as i32;
                    let imm = self.read_op(instr, 2, false, cpu, bus) as i16 as i32;
                    let res = src * imm;
                    self.write_op(instr, 0, (res & 0xFFFF) as u16, false, cpu, bus);
                    let carry = res != (res as i16 as i32);
                    cpu.flags.cf = carry;
                    cpu.flags.of = carry;
                }
            }

            Mnemonic::Mul => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                if is_byte {
                    let res = (cpu.al() as u32) * (op0 as u32);
                    cpu.ax = res as u16;
                    let carry = (res >> 8) != 0;
                    cpu.flags.cf = carry;
                    cpu.flags.of = carry;
                } else {
                    let res = (cpu.ax as u32) * (op0 as u32);
                    cpu.ax = (res & 0xFFFF) as u16;
                    cpu.dx = (res >> 16) as u16;
                    let carry = cpu.dx != 0;
                    cpu.flags.cf = carry;
                    cpu.flags.of = carry;
                }
            }

            Mnemonic::Div => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                if is_byte {
                    if op0 != 0 {
                        let num = cpu.ax;
                        let quot = num / op0;
                        let rem = num % op0;
                        cpu.set_al(quot as u8);
                        cpu.set_ah(rem as u8);
                    }
                } else {
                    if op0 != 0 {
                        let num = ((cpu.dx as u32) << 16) | (cpu.ax as u32);
                        let quot = num / (op0 as u32);
                        let rem = num % (op0 as u32);
                        cpu.ax = quot as u16;
                        cpu.dx = rem as u16;
                    }
                }
            }

            Mnemonic::Idiv => {
                let op0 = self.read_op(instr, 0, is_byte, cpu, bus);
                if is_byte {
                    let divisor = op0 as i8 as i16;
                    if divisor != 0 {
                        let num = cpu.ax as i16;
                        let quot = num / divisor;
                        let rem = num % divisor;
                        cpu.set_al(quot as u8);
                        cpu.set_ah(rem as u8);
                    }
                } else {
                    let divisor = op0 as i16 as i32;
                    if divisor != 0 {
                        let num = ((cpu.dx as i32) << 16) | (cpu.ax as i32);
                        let quot = num / divisor;
                        let rem = num % divisor;
                        cpu.ax = quot as u16;
                        cpu.dx = rem as u16;
                    }
                }
            }

            // Loop instructions
            Mnemonic::Loop => {
                cpu.cx = cpu.cx.wrapping_sub(1);
                if cpu.cx != 0 { cpu.ip = instr.near_branch16(); }
            }
            Mnemonic::Loope => {
                cpu.cx = cpu.cx.wrapping_sub(1);
                if cpu.cx != 0 && cpu.flags.zf { cpu.ip = instr.near_branch16(); }
            }
            Mnemonic::Loopne => {
                cpu.cx = cpu.cx.wrapping_sub(1);
                if cpu.cx != 0 && !cpu.flags.zf { cpu.ip = instr.near_branch16(); }
            }

            // String instructions
            Mnemonic::Movsb => {
                let rep = instr.has_rep_prefix();
                let steps = if rep { cpu.cx } else { 1 };
                let step = if cpu.flags.df { -1i32 } else { 1i32 };
                for _ in 0..steps {
                    let b = bus.read_u8(cpu.ds, cpu.si);
                    bus.write_u8(cpu.es, cpu.di, b);
                    cpu.si = (cpu.si as i32 + step) as u16;
                    cpu.di = (cpu.di as i32 + step) as u16;
                }
                if rep { cpu.cx = 0; }
            }

            Mnemonic::Movsw => {
                let rep = instr.has_rep_prefix();
                let steps = if rep { cpu.cx } else { 1 };
                let step = if cpu.flags.df { -2i32 } else { 2i32 };
                for _ in 0..steps {
                    let w = bus.read_u16(cpu.ds, cpu.si);
                    bus.write_u16(cpu.es, cpu.di, w);
                    cpu.si = (cpu.si as i32 + step) as u16;
                    cpu.di = (cpu.di as i32 + step) as u16;
                }
                if rep { cpu.cx = 0; }
            }

            Mnemonic::Stosb => {
                let rep = instr.has_rep_prefix();
                let steps = if rep { cpu.cx } else { 1 };
                let step = if cpu.flags.df { -1i32 } else { 1i32 };
                let val = cpu.al();
                for _ in 0..steps {
                    bus.write_u8(cpu.es, cpu.di, val);
                    cpu.di = (cpu.di as i32 + step) as u16;
                }
                if rep { cpu.cx = 0; }
            }

            Mnemonic::Stosw => {
                let rep = instr.has_rep_prefix();
                let steps = if rep { cpu.cx } else { 1 };
                let step = if cpu.flags.df { -2i32 } else { 2i32 };
                let val = cpu.ax;
                for _ in 0..steps {
                    bus.write_u16(cpu.es, cpu.di, val);
                    cpu.di = (cpu.di as i32 + step) as u16;
                }
                if rep { cpu.cx = 0; }
            }

            Mnemonic::Lodsb => {
                let step = if cpu.flags.df { -1i32 } else { 1i32 };
                let val = bus.read_u8(cpu.ds, cpu.si);
                cpu.set_al(val);
                cpu.si = (cpu.si as i32 + step) as u16;
            }

            Mnemonic::Lodsw => {
                let step = if cpu.flags.df { -2i32 } else { 2i32 };
                let val = bus.read_u16(cpu.ds, cpu.si);
                cpu.ax = val;
                cpu.si = (cpu.si as i32 + step) as u16;
            }

            Mnemonic::Scasb => {
                let repne = instr.has_repne_prefix();
                let repe = instr.has_repe_prefix();
                let step = if cpu.flags.df { -1i32 } else { 1i32 };
                loop {
                    let b = bus.read_u8(cpu.es, cpu.di);
                    let (diff, borrow) = cpu.al().overflowing_sub(b);
                    cpu.flags.cf = borrow;
                    cpu.flags.of = ((cpu.al() ^ b) & (cpu.al() ^ diff) & 0x80) != 0;
                    cpu.update_szp_u8(diff);
                    cpu.di = (cpu.di as i32 + step) as u16;

                    if repne {
                        cpu.cx = cpu.cx.wrapping_sub(1);
                        if cpu.cx == 0 || cpu.flags.zf { break; }
                    } else if repe {
                        cpu.cx = cpu.cx.wrapping_sub(1);
                        if cpu.cx == 0 || !cpu.flags.zf { break; }
                    } else {
                        break;
                    }
                }
            }

            Mnemonic::Scasw => {
                let repne = instr.has_repne_prefix();
                let repe = instr.has_repe_prefix();
                let step = if cpu.flags.df { -2i32 } else { 2i32 };
                loop {
                    let w = bus.read_u16(cpu.es, cpu.di);
                    let (diff, borrow) = cpu.ax.overflowing_sub(w);
                    cpu.flags.cf = borrow;
                    cpu.flags.of = ((cpu.ax ^ w) & (cpu.ax ^ diff) & 0x8000) != 0;
                    cpu.update_szp_u16(diff);
                    cpu.di = (cpu.di as i32 + step) as u16;

                    if repne {
                        cpu.cx = cpu.cx.wrapping_sub(1);
                        if cpu.cx == 0 || cpu.flags.zf { break; }
                    } else if repe {
                        cpu.cx = cpu.cx.wrapping_sub(1);
                        if cpu.cx == 0 || !cpu.flags.zf { break; }
                    } else {
                        break;
                    }
                }
            }

            Mnemonic::Cmpsb => {
                let repne = instr.has_repne_prefix();
                let repe = instr.has_repe_prefix();
                let step = if cpu.flags.df { -1i32 } else { 1i32 };
                loop {
                    let b0 = bus.read_u8(cpu.ds, cpu.si);
                    let b1 = bus.read_u8(cpu.es, cpu.di);
                    let (diff, borrow) = b0.overflowing_sub(b1);
                    cpu.flags.cf = borrow;
                    cpu.flags.of = ((b0 ^ b1) & (b0 ^ diff) & 0x80) != 0;
                    cpu.update_szp_u8(diff);
                    cpu.si = (cpu.si as i32 + step) as u16;
                    cpu.di = (cpu.di as i32 + step) as u16;

                    if repne {
                        cpu.cx = cpu.cx.wrapping_sub(1);
                        if cpu.cx == 0 || cpu.flags.zf { break; }
                    } else if repe {
                        cpu.cx = cpu.cx.wrapping_sub(1);
                        if cpu.cx == 0 || !cpu.flags.zf { break; }
                    } else {
                        break;
                    }
                }
            }

            // Port I/O
            Mnemonic::In => {
                let port = match instr.op_kind(1) {
                    OpKind::Immediate8 => instr.immediate8() as u16,
                    OpKind::Register => cpu.dx,
                    _ => 0,
                };
                if instr.op0_register() == Register::AX {
                    let low = bus.port_in_u8(port) as u16;
                    let high = bus.port_in_u8(port.wrapping_add(1)) as u16;
                    cpu.ax = low | (high << 8);
                } else {
                    let val = bus.port_in_u8(port);
                    cpu.set_al(val);
                }
            }

            Mnemonic::Out => {
                let port = match instr.op_kind(0) {
                    OpKind::Immediate8 => instr.immediate8() as u16,
                    OpKind::Register => cpu.dx,
                    _ => 0,
                };
                if instr.op_count() > 1 && instr.op_register(1) == Register::AX {
                    let al = cpu.al();
                    let ah = cpu.ah();
                    bus.port_out_u8(port, al);
                    bus.port_out_u8(port.wrapping_add(1), ah);
                } else {
                    let val = cpu.al();
                    bus.port_out_u8(port, val);
                }
            }

            _ => {
                println!(
                    "[UNHANDLED] {:?} at {:04X}:{:04X}",
                    instr.mnemonic(),
                    cpu.cs,
                    cpu.ip
                );
            }
        }
    }

    fn read_op(
        &self,
        instr: &Instruction,
        op_idx: u32,
        is_byte: bool,
        cpu: &mut CpuContext,
        bus: &mut MemoryBus,
    ) -> u16 {
        match instr.op_kind(op_idx) {
            OpKind::Register => {
                let reg = instr.op_register(op_idx);
                self.read_reg(reg, cpu)
            }
            OpKind::Immediate8 => instr.immediate8() as u16,
            OpKind::Immediate16 => instr.immediate16(),
            OpKind::Immediate8to16 => instr.immediate8to16() as u16,
            OpKind::Memory => {
                let (seg, off) = self.resolve_mem(instr, cpu);
                if is_byte {
                    bus.read_u8(seg, off) as u16
                } else {
                    bus.read_u16(seg, off)
                }
            }
            _ => 0,
        }
    }

    fn write_op(
        &self,
        instr: &Instruction,
        op_idx: u32,
        val: u16,
        is_byte: bool,
        cpu: &mut CpuContext,
        bus: &mut MemoryBus,
    ) {
        match instr.op_kind(op_idx) {
            OpKind::Register => self.write_reg(instr.op_register(op_idx), val, cpu),
            OpKind::Memory => {
                let (seg, off) = self.resolve_mem(instr, cpu);
                if is_byte {
                    bus.write_u8(seg, off, val as u8);
                } else {
                    bus.write_u16(seg, off, val);
                }
            }
            _ => {}
        }
    }

    fn read_reg(&self, reg: Register, cpu: &CpuContext) -> u16 {
        match reg {
            Register::AX => cpu.ax,
            Register::CX => cpu.cx,
            Register::DX => cpu.dx,
            Register::BX => cpu.bx,
            Register::SP => cpu.sp,
            Register::BP => cpu.bp,
            Register::SI => cpu.si,
            Register::DI => cpu.di,
            Register::AL => cpu.al() as u16,
            Register::AH => cpu.ah() as u16,
            Register::CL => cpu.cl() as u16,
            Register::CH => cpu.ch() as u16,
            Register::DL => cpu.dl() as u16,
            Register::DH => cpu.dh() as u16,
            Register::BL => cpu.bl() as u16,
            Register::BH => cpu.bh() as u16,
            Register::ES => cpu.es,
            Register::CS => cpu.cs,
            Register::SS => cpu.ss,
            Register::DS => cpu.ds,
            _ => 0,
        }
    }

    fn write_reg(&self, reg: Register, val: u16, cpu: &mut CpuContext) {
        match reg {
            Register::AX => cpu.ax = val,
            Register::CX => cpu.cx = val,
            Register::DX => cpu.dx = val,
            Register::BX => cpu.bx = val,
            Register::SP => cpu.sp = val,
            Register::BP => cpu.bp = val,
            Register::SI => cpu.si = val,
            Register::DI => cpu.di = val,
            Register::AL => cpu.set_al(val as u8),
            Register::AH => cpu.set_ah(val as u8),
            Register::CL => cpu.set_cl(val as u8),
            Register::CH => cpu.set_ch(val as u8),
            Register::DL => cpu.set_dl(val as u8),
            Register::DH => cpu.set_dh(val as u8),
            Register::BL => cpu.set_bl(val as u8),
            Register::BH => cpu.set_bh(val as u8),
            Register::ES => cpu.es = val,
            Register::CS => cpu.cs = val,
            Register::SS => cpu.ss = val,
            Register::DS => cpu.ds = val,
            _ => {}
        }
    }

    fn resolve_mem(&self, instr: &Instruction, cpu: &CpuContext) -> (u16, u16) {
        let seg = match instr.segment_prefix() {
            Register::ES => cpu.es,
            Register::CS => cpu.cs,
            Register::SS => cpu.ss,
            _ => match instr.memory_base() {
                Register::BP => cpu.ss,
                _ => cpu.ds,
            },
        };

        let mut off = instr.memory_displacement32() as u16;
        match instr.memory_base() {
            Register::BX => off = off.wrapping_add(cpu.bx),
            Register::BP => off = off.wrapping_add(cpu.bp),
            Register::SI => off = off.wrapping_add(cpu.si),
            Register::DI => off = off.wrapping_add(cpu.di),
            _ => {}
        }
        match instr.memory_index() {
            Register::SI => off = off.wrapping_add(cpu.si),
            Register::DI => off = off.wrapping_add(cpu.di),
            _ => {}
        }

        (seg, off)
    }
}
