//! Integrated Commodore 64 SID (MOS 6581) Player for Grand Prix Circuit.
//! Executes Kris Hatlelid's C64 soundtrack using mos6502 and sidera.

use mos6502::cpu::CPU;
use mos6502::instruction::Nmos6502;
use mos6502::memory::Bus;
use mos6502::registers::StackPointer;
use sidera::registers::{ENV3, OSC3};
use sidera::{ChipModel, Sid};

fn load_sid_data() -> Option<Vec<u8>> {
    let candidate_paths = [
        "Grand_Prix_Circuit.sid",
        "assets/Grand_Prix_Circuit.sid",
        "gprix2/Grand_Prix_Circuit.sid",
        "c64_assets/Grand_Prix_Circuit.sid",
        "../assets/Grand_Prix_Circuit.sid",
        "../Grand_Prix_Circuit.sid",
    ];

    for path in &candidate_paths {
        if let Ok(bytes) = std::fs::read(path) {
            println!("[SID] Loaded Commodore 64 soundtrack from '{}' ({} bytes)", path, bytes.len());
            return Some(bytes);
        }
    }

    println!("[SID] 'Grand_Prix_Circuit.sid' not found. C64 Mode will run with PC Speaker audio.");
    None
}

pub struct C64Memory {
    pub ram: Box<[u8; 0x10000]>,
    pub sid: Sid,
}

impl C64Memory {
    pub fn new(sid: Sid) -> Self {
        let mut ram = Box::new([0u8; 0x10000]);
        ram[0x00] = 0x2F; // DDR
        ram[0x01] = 0x37; // Standard C64 memory map
        Self { ram, sid }
    }
}

impl Bus for C64Memory {
    fn get_byte(&mut self, addr: u16) -> u8 {
        if (0xD400..=0xD7FF).contains(&addr) {
            let reg = ((addr - 0xD400) & 0x1F) as u8;
            match reg {
                OSC3 | ENV3 => self.sid.read_register(reg),
                _ => 0xFF,
            }
        } else {
            self.ram[addr as usize]
        }
    }

    fn set_byte(&mut self, addr: u16, value: u8) {
        if (0xD400..=0xD7FF).contains(&addr) {
            let reg = ((addr - 0xD400) & 0x1F) as u8;
            self.sid.write_register(reg, value);
        } else {
            self.ram[addr as usize] = value;
        }
    }
}

pub struct SidPlayer {
    cpu: CPU<C64Memory, Nmos6502>,
    play_addr: u16,
    sample_rate: u32,
    samples_per_tick: usize,
    sample_accum: usize,
    pub current_song: u8,
    raw_data: Vec<u8>,
}

impl SidPlayer {
    pub fn new(sample_rate: u32) -> Option<Self> {
        let sid_data = load_sid_data()?;
        let player = Self::init_with_data(&sid_data, 0, sample_rate)?;
        Some(player)
    }

    fn init_with_data(raw: &[u8], song0: u8, sample_rate: u32) -> Option<Self> {
        if raw.len() < 0x7C {
            return None;
        }
        let be16 = |o: usize| u16::from_be_bytes([raw[o], raw[o + 1]]);
        let data_offset = be16(0x06) as usize;
        let mut load_addr = be16(0x08);
        let init_addr = be16(0x0A);
        let play_addr = be16(0x0C);

        let mut data = &raw[data_offset..];
        if load_addr == 0 {
            if data.len() < 2 {
                return None;
            }
            load_addr = u16::from_le_bytes([data[0], data[1]]);
            data = &data[2..];
        }

        let mut sid = Sid::new(sample_rate);
        sid.set_model(ChipModel::Mos6581);
        let mut mem = C64Memory::new(sid);

        // Load tune into RAM
        let base = load_addr as usize;
        for (i, &b) in data.iter().enumerate() {
            if base + i < 0x10000 {
                mem.ram[base + i] = b;
            }
        }

        // Install KERNAL $EA31 IRQ return stub: PLA, TAY, PLA, TAX, PLA, RTI
        for (off, b) in [0x68u8, 0xA8, 0x68, 0xAA, 0x68, 0x40].into_iter().enumerate() {
            mem.ram[0xEA31 + off] = b;
        }
        mem.ram[0xEA81] = 0x40; // bare RTI

        let mut cpu = CPU::new(mem, Nmos6502);

        // Call tune init routine with A = sub-song index (0-based)
        cpu.registers.accumulator = song0;
        cpu.registers.index_x = 0;
        cpu.registers.index_y = 0;
        Self::execute_subroutine(&mut cpu, init_addr);

        // 50 Hz PAL C64 refresh rate
        let samples_per_tick = (sample_rate / 50).max(1) as usize;

        Some(Self {
            cpu,
            play_addr,
            sample_rate,
            samples_per_tick,
            sample_accum: 0,
            current_song: song0,
            raw_data: raw.to_vec(),
        })
    }

    pub fn set_song(&mut self, song0: u8) {
        if let Some(new_p) = Self::init_with_data(&self.raw_data, song0, self.sample_rate) {
            *self = new_p;
        }
    }

    fn execute_subroutine(cpu: &mut CPU<C64Memory, Nmos6502>, addr: u16) {
        let sp0 = cpu.registers.stack_pointer.0;
        // Push sentinel return address (0x0000 - 1 = 0xFFFF)
        let push = |cpu: &mut CPU<C64Memory, Nmos6502>, v: u8| {
            let sp = cpu.registers.stack_pointer.0;
            cpu.memory.set_byte(0x0100 + sp as u16, v);
            cpu.registers.stack_pointer = StackPointer(sp.wrapping_sub(1));
        };
        push(cpu, 0x00);
        push(cpu, 0x00);
        cpu.registers.program_counter = addr;

        let mut guard: u32 = 0;
        while cpu.registers.stack_pointer.0 != sp0 {
            if !cpu.single_step() {
                break;
            }
            guard += 1;
            if guard > 2_000_000 {
                break;
            }
        }
    }

    fn step_play(&mut self) {
        if self.play_addr != 0 {
            Self::execute_subroutine(&mut self.cpu, self.play_addr);
        } else {
            self.call_irq();
        }
    }

    fn call_irq(&mut self) -> bool {
        let rb = |m: &mut C64Memory, a: u16| m.ram[a as usize];
        let kernal = (rb(&mut self.cpu.memory, 0x0314) as u16)
            | ((rb(&mut self.cpu.memory, 0x0315) as u16) << 8);
        let hw = (rb(&mut self.cpu.memory, 0xFFFE) as u16)
            | ((rb(&mut self.cpu.memory, 0xFFFF) as u16) << 8);

        let (vector, push_axy) = if kernal != 0 {
            (kernal, true)
        } else if hw != 0 {
            (hw, false)
        } else {
            return false;
        };

        let sp0 = self.cpu.registers.stack_pointer.0;
        let (a, x, y) = (
            self.cpu.registers.accumulator,
            self.cpu.registers.index_x,
            self.cpu.registers.index_y,
        );
        let p_byte = (self.cpu.registers.status.bits() & !0x10) | 0x24;

        let push = |cpu: &mut CPU<C64Memory, Nmos6502>, v: u8| {
            let sp = cpu.registers.stack_pointer.0;
            cpu.memory.set_byte(0x0100 + sp as u16, v);
            cpu.registers.stack_pointer = StackPointer(sp.wrapping_sub(1));
        };

        push(&mut self.cpu, 0x00);
        push(&mut self.cpu, 0x00);
        push(&mut self.cpu, p_byte);
        if push_axy {
            push(&mut self.cpu, a);
            push(&mut self.cpu, x);
            push(&mut self.cpu, y);
        }
        self.cpu.registers.program_counter = vector;

        let mut guard: u32 = 0;
        while self.cpu.registers.stack_pointer.0 != sp0 {
            if !self.cpu.single_step() {
                break;
            }
            guard += 1;
            if guard > 2_000_000 {
                break;
            }
        }
        true
    }

    /// Render next batch of audio samples in [-1.0, 1.0] range.
    pub fn render_sample(&mut self) -> f32 {
        if self.sample_accum == 0 {
            self.step_play();
            self.sample_accum = self.samples_per_tick;
        }
        self.sample_accum -= 1;
        let sample_i16: i16 = self.cpu.memory.sid.clock();
        (sample_i16 as f32) / 32768.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sid_player_renders_audio() {
        let mut player = SidPlayer::new(48000).expect("failed to init SidPlayer");
        let mut non_zero = 0;
        for _ in 0..48000 {
            let s = player.render_sample();
            if s.abs() > 0.0001 {
                non_zero += 1;
            }
        }
        println!("Rendered 1 second of SID audio: {} non-zero samples", non_zero);
        assert!(non_zero > 0, "SID should produce audible samples");
    }
}

