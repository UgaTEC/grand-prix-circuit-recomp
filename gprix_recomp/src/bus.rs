/// 1 MB MS-DOS Real Mode memory bus with authentic EGA graphics hardware and IVT interrupt dispatch.

use crate::cpu::CpuContext;

pub const MEMORY_SIZE: usize = 1024 * 1024; // 1 MB
pub const EGA_BASE: usize = 0xA0000;
pub const EGA_END: usize = 0xAFFFF;
pub const EGA_PLANE_SIZE: usize = 64 * 1024;

/// Standard EGA 16-color palette (0x00RRGGBB).
pub const EGA_DEFAULT_PALETTE: [u32; 16] = [
    0x00000000, // 0: Black
    0x000000AA, // 1: Blue
    0x0000AA00, // 2: Green
    0x0000AAAA, // 3: Cyan
    0x00AA0000, // 4: Red
    0x00AA00AA, // 5: Magenta
    0x00AA5500, // 6: Brown
    0x00AAAAAA, // 7: Light Gray
    0x00555555, // 8: Dark Gray
    0x005555FF, // 9: Bright Blue
    0x0055FF55, // 10: Bright Green
    0x0055FFFF, // 11: Bright Cyan
    0x00FF5555, // 12: Bright Red
    0x00FF55FF, // 13: Bright Magenta
    0x00FFFF55, // 14: Yellow
    0x00FFFFFF, // 15: Bright White
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoAudioMode {
    Dos,
    C64,
}

/// Authentic Commodore 64 VIC-II Color Palette (Colodore / Pepto standard)
pub const C64_VIC2_PALETTE: [u32; 16] = [
    0x00000000, // 0: Black
    0x00FFFFFF, // 1: White
    0x00813338, // 2: Red
    0x0075CEC8, // 3: Cyan
    0x008E3C97, // 4: Purple
    0x0056AC4D, // 5: Green
    0x002E2C9B, // 6: Blue
    0x00EDF171, // 7: Yellow
    0x008E5029, // 8: Orange
    0x00553800, // 9: Brown
    0x00C46C71, // 10: Light Red
    0x004A4A4A, // 11: Dark Grey
    0x007B7B7B, // 12: Medium Grey
    0x00A9FF9F, // 13: Light Green
    0x00706DEB, // 14: Light Blue
    0x00B2B2B2, // 15: Light Grey
];

/// Dynamic translation table: MS-DOS EGA 16-color index -> Commodore 64 VIC-II color
pub const EGA_TO_C64_MAP: [u32; 16] = [
    C64_VIC2_PALETTE[0],  // EGA Black -> C64 Black
    C64_VIC2_PALETTE[6],  // EGA Blue -> C64 Blue
    C64_VIC2_PALETTE[5],  // EGA Green -> C64 Green
    C64_VIC2_PALETTE[3],  // EGA Cyan -> C64 Cyan
    C64_VIC2_PALETTE[2],  // EGA Red -> C64 Red
    C64_VIC2_PALETTE[4],  // EGA Magenta -> C64 Purple
    C64_VIC2_PALETTE[9],  // EGA Brown -> C64 Brown
    C64_VIC2_PALETTE[15], // EGA Light Gray -> C64 Light Grey
    C64_VIC2_PALETTE[11], // EGA Dark Gray -> C64 Dark Grey
    C64_VIC2_PALETTE[14], // EGA Bright Blue -> C64 Light Blue
    C64_VIC2_PALETTE[13], // EGA Bright Green -> C64 Light Green
    C64_VIC2_PALETTE[3],  // EGA Bright Cyan -> C64 Cyan
    C64_VIC2_PALETTE[10], // EGA Bright Red -> C64 Light Red
    C64_VIC2_PALETTE[8],  // EGA Bright Magenta -> C64 Orange
    C64_VIC2_PALETTE[7],  // EGA Yellow -> C64 Yellow
    C64_VIC2_PALETTE[1],  // EGA White -> C64 White
];

pub struct EgaController {
    pub planes: [[u8; EGA_PLANE_SIZE]; 4],
    pub latches: [u8; 4],

    // Sequencer (0x3C4 / 0x3C5)
    pub seq_index: u8,
    pub map_mask: u8, // Index 2: bit 0..3 enable planes 0..3 for writing

    // Graphics Controller (0x3CE / 0x3CF)
    pub gc_index: u8,
    pub set_reset: u8,        // Index 0
    pub enable_set_reset: u8, // Index 1
    pub color_compare: u8,    // Index 2
    pub data_rotate: u8,      // Index 3: bits 0-2 rotate, bits 3-4 logical op (0=Copy, 1=AND, 2=OR, 3=XOR)
    pub read_map_select: u8,  // Index 4: plane 0..3 for Read Mode 0
    pub mode: u8,             // Index 5: bits 0-1 Write Mode (0..3), bit 3 Read Mode
    pub misc: u8,             // Index 6
    pub color_dont_care: u8,  // Index 7
    pub bit_mask: u8,         // Index 8

    pub palette_lut: [u8; 16],
}

impl Default for EgaController {
    fn default() -> Self {
        let mut palette_lut = [0u8; 16];
        for i in 0..16 {
            palette_lut[i] = i as u8;
        }
        Self {
            planes: [[0u8; EGA_PLANE_SIZE]; 4],
            latches: [0u8; 4],
            seq_index: 0,
            map_mask: 0x0F, // All 4 planes enabled by default
            gc_index: 0,
            set_reset: 0,
            enable_set_reset: 0,
            color_compare: 0,
            data_rotate: 0, // 0 = Copy, rotate 0
            read_map_select: 0,
            mode: 0, // Write Mode 0
            misc: 0,
            color_dont_care: 0x0F,
            bit_mask: 0xFF,
            palette_lut,
        }
    }
}

pub struct MemoryBus {
    pub ram: Box<[u8; MEMORY_SIZE]>,
    pub ega: EgaController,
    pub video_mode: u8,
    pub pit_counter: u16,
    pub pit_ch0_divisor: u16,
    pub pit_ch0_flip: bool,
    pub pit_ch2_divisor: u16,
    pub pit_ch2_flip: bool,
    pub speaker_port: u8,
    pub kbd_port_60: u8,
    pub speaker: Option<crate::audio::PcSpeaker>,
    pub mode: VideoAudioMode,
    pub osd_frames: u32,
    pub osd_text: String,
    pub smooth_mode: bool,
    pub prev_frame: Vec<u32>,
    pub curr_frame: Vec<u32>,
    pub ega_dirty: bool,
    pub motion_dx: [i16; 200],
    pub motion_dy: [i16; 200],
    pub frame_instant: std::time::Instant,
}

impl MemoryBus {
    pub fn new() -> Self {
        Self {
            ram: vec![0u8; MEMORY_SIZE].into_boxed_slice().try_into().unwrap(),
            ega: EgaController::default(),
            video_mode: 0x03, // 80x25 text mode initially
            pit_counter: 0,
            pit_ch0_divisor: 0, // 0 = 65536 in 8253 PIT (18.2065 Hz)
            pit_ch0_flip: false,
            pit_ch2_divisor: 0,
            pit_ch2_flip: false,
            speaker_port: 0,
            kbd_port_60: 0,
            speaker: Some(crate::audio::PcSpeaker::new()),
            mode: VideoAudioMode::Dos,
            osd_frames: 0,
            osd_text: String::new(),
            smooth_mode: true, // 60 FPS temporal smoothing enabled by default!
            prev_frame: vec![0u32; 320 * 200],
            curr_frame: vec![0u32; 320 * 200],
            ega_dirty: true,
            motion_dx: [0i16; 200],
            motion_dy: [0i16; 200],
            frame_instant: std::time::Instant::now(),
        }
    }

    /// Toggle in real-time between MS-DOS (EGA/PC Speaker) and Commodore 64 (VIC-II/SID).
    pub fn toggle_mode(&mut self) -> VideoAudioMode {
        self.mode = match self.mode {
            VideoAudioMode::Dos => VideoAudioMode::C64,
            VideoAudioMode::C64 => VideoAudioMode::Dos,
        };
        self.osd_frames = 90; // Show OSD banner for 1.5 seconds (90 frames at 60 Hz)
        self.osd_text = match self.mode {
            VideoAudioMode::C64 => "[ C64 MODE ]".to_string(),
            VideoAudioMode::Dos => "[ DOS MODE ]".to_string(),
        };
        if let Some(ref speaker) = self.speaker {
            speaker.set_c64_mode(self.mode == VideoAudioMode::C64);
        }
        self.mode
    }

    /// Toggle in real-time between 60 FPS Motion Interpolation and Classic 20 FPS.
    pub fn toggle_smooth_mode(&mut self) -> bool {
        self.smooth_mode = !self.smooth_mode;
        self.osd_frames = 90;
        self.osd_text = if self.smooth_mode {
            "[ 60 FPS INTERPOLATION ]".to_string()
        } else {
            "[ 20 FPS CLASSIC ]".to_string()
        };
        self.smooth_mode
    }

    /// Calculate the current PIT Channel 0 (Timer IRQ0) frequency in Hz.
    pub fn pit_ch0_frequency(&self) -> f64 {
        let div = if self.pit_ch0_divisor == 0 {
            65536.0
        } else {
            self.pit_ch0_divisor as f64
        };
        1_193_182.0 / div
    }

    pub fn port_in_u8(&mut self, port: u16) -> u8 {
        match port {
            0x60 => {
                // Keyboard controller data port (scancode)
                self.kbd_port_60
            }
            0x40 | 0x42 => {
                // PIT Counter 0 / 2 read
                self.pit_counter = self.pit_counter.wrapping_add(1);
                (self.pit_counter & 0xFF) as u8
            }
            0x61 => {
                // PC Speaker port / refresh toggle
                self.speaker_port ^= 0x10; // Toggle refresh bit 4 for timing loops
                self.speaker_port
            }
            0x201 => {
                // Joystick port: return 0xFF (no joystick attached / buttons released)
                0xFF
            }
            0x3DA => {
                // EGA Input Status Register 1: bit 0 = display disable, bit 3 = vertical retrace!
                static mut VBLANK_TOGGLE: u8 = 0;
                unsafe {
                    VBLANK_TOGGLE ^= 0x08 | 0x01;
                    VBLANK_TOGGLE
                }
            }
            _ => 0xFF,
        }
    }

    pub fn port_out_u8(&mut self, port: u16, val: u8) {
        match port {
            0x20 => {
                // PIC 8259 End of Interrupt (EOI)
            }
            0x40 => {
                // PIT Channel 0 (System Timer frequency divisor)
                if !self.pit_ch0_flip {
                    self.pit_ch0_divisor = (self.pit_ch0_divisor & 0xFF00) | (val as u16);
                    self.pit_ch0_flip = true;
                } else {
                    self.pit_ch0_divisor = (self.pit_ch0_divisor & 0x00FF) | ((val as u16) << 8);
                    self.pit_ch0_flip = false;
                    println!(
                        "[PIT] Channel 0 divisor set to {} (frequency: {:.2} Hz)",
                        if self.pit_ch0_divisor == 0 { 65536u32 } else { self.pit_ch0_divisor as u32 },
                        self.pit_ch0_frequency()
                    );
                }
            }
            0x42 => {
                // PIT Channel 2 (Speaker frequency divisor)
                if !self.pit_ch2_flip {
                    self.pit_ch2_divisor = (self.pit_ch2_divisor & 0xFF00) | (val as u16);
                    self.pit_ch2_flip = true;
                } else {
                    self.pit_ch2_divisor = (self.pit_ch2_divisor & 0x00FF) | ((val as u16) << 8);
                    self.pit_ch2_flip = false;
                    self.update_speaker();
                }
            }
            0x43 => {
                // PIT Command register
                let channel = val >> 6;
                if channel == 0 {
                    self.pit_ch0_flip = false;
                } else if channel == 2 {
                    self.pit_ch2_flip = false;
                }
            }
            0x61 => {
                self.speaker_port = val;
                self.update_speaker();
            }
            0x3C4 => {
                // Sequencer Index
                self.ega.seq_index = val;
            }
            0x3C5 => {
                // Sequencer Data
                if self.ega.seq_index == 2 {
                    self.ega.map_mask = val & 0x0F;
                }
            }
            0x3CE => {
                // Graphics Controller Index
                self.ega.gc_index = val;
            }
            0x3CF => {
                // Graphics Controller Data
                match self.ega.gc_index {
                    0 => self.ega.set_reset = val & 0x0F,
                    1 => self.ega.enable_set_reset = val & 0x0F,
                    2 => self.ega.color_compare = val & 0x0F,
                    3 => self.ega.data_rotate = val, // Rotate & Logic Operation (Copy, AND, OR, XOR)
                    4 => self.ega.read_map_select = val & 3,
                    5 => self.ega.mode = val, // Write mode & Read mode
                    6 => self.ega.misc = val,
                    7 => self.ega.color_dont_care = val & 0x0F,
                    8 => self.ega.bit_mask = val, // Bit mask
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn update_speaker(&mut self) {
        let is_on = (self.speaker_port & 3) == 3;
        let freq = if self.pit_ch2_divisor > 0 {
            1_193_182 / (self.pit_ch2_divisor as u32)
        } else {
            0
        };
        if let Some(ref speaker) = self.speaker {
            speaker.set_state(is_on, freq);
        }
    }

    #[inline(always)]
    pub fn phys_addr(seg: u16, off: u16) -> usize {
        (((seg as usize) << 4) + (off as usize)) & 0xFFFFF
    }

    pub fn read_u8(&mut self, seg: u16, off: u16) -> u8 {
        let addr = Self::phys_addr(seg, off);
        if (EGA_BASE..=EGA_END).contains(&addr) {
            let ega_off = addr - EGA_BASE;
            // Update EGA internal latches from all 4 planes on every read
            for plane in 0..4 {
                self.ega.latches[plane] = self.ega.planes[plane][ega_off];
            }
            let read_mode = (self.ega.mode >> 3) & 1;
            if read_mode == 0 {
                // Read Mode 0: read from selected plane
                let plane = (self.ega.read_map_select & 3) as usize;
                self.ega.planes[plane][ega_off]
            } else {
                // Read Mode 1: Color Compare across 4 planes
                let mut result = 0xFFu8;
                for plane in 0..4 {
                    let c_bit = (self.ega.color_compare >> plane) & 1;
                    let c_dont_care = (self.ega.color_dont_care >> plane) & 1;
                    if c_dont_care != 0 {
                        let plane_byte = self.ega.latches[plane];
                        let comp = if c_bit != 0 { plane_byte } else { !plane_byte };
                        result &= comp;
                    }
                }
                result
            }
        } else {
            self.ram[addr]
        }
    }

    pub fn read_u16(&mut self, seg: u16, off: u16) -> u16 {
        let low = self.read_u8(seg, off) as u16;
        let high = self.read_u8(seg, off.wrapping_add(1)) as u16;
        low | (high << 8)
    }

    pub fn write_u8(&mut self, seg: u16, off: u16, val: u8) {
        let addr = Self::phys_addr(seg, off);
        if (EGA_BASE..=EGA_END).contains(&addr) {
            let ega_off = addr - EGA_BASE;
            self.ega_dirty = true;
            let write_mode = self.ega.mode & 3;
            let logic_op = (self.ega.data_rotate >> 3) & 3;
            let rotate = (self.ega.data_rotate & 7) as u32;
            let bit_mask = self.ega.bit_mask;
            let inv_bit_mask = !bit_mask;

            for plane in 0..4 {
                if (self.ega.map_mask & (1 << plane)) == 0 {
                    continue;
                }
                let latch = self.ega.latches[plane];

                let mut plane_data = match write_mode {
                    0 => {
                        // Write Mode 0:
                        let rotated = val.rotate_right(rotate);
                        let esr_bit = (self.ega.enable_set_reset >> plane) & 1;
                        let sr_bit = (self.ega.set_reset >> plane) & 1;
                        if esr_bit != 0 {
                            if sr_bit != 0 { 0xFF } else { 0x00 }
                        } else {
                            rotated
                        }
                    }
                    1 => {
                        // Write Mode 1: write latches directly (fast plane copying/blitting)
                        latch
                    }
                    2 => {
                        // Write Mode 2: CPU data bit N fills plane N (color blit)
                        let bit = (val >> plane) & 1;
                        if bit != 0 { 0xFF } else { 0x00 }
                    }
                    3 => {
                        // Write Mode 3: Set/Reset value with rotated CPU data as bitmask
                        let sr_bit = (self.ega.set_reset >> plane) & 1;
                        if sr_bit != 0 { 0xFF } else { 0x00 }
                    }
                    _ => val,
                };

                // Apply Logical Operation: 0 = Copy, 1 = AND, 2 = OR, 3 = XOR (for highlight/inversion!)
                if write_mode != 1 {
                    plane_data = match logic_op {
                        0 => plane_data,
                        1 => plane_data & latch,
                        2 => plane_data | latch,
                        3 => plane_data ^ latch, // XOR!
                        _ => plane_data,
                    };
                }

                // Apply Bit Mask
                let new_val = (latch & inv_bit_mask) | (plane_data & bit_mask);
                self.ega.planes[plane][ega_off] = new_val;
            }
        } else {
            self.ram[addr] = val;
        }
    }

    pub fn write_u16(&mut self, seg: u16, off: u16, val: u16) {
        self.write_u8(seg, off, val as u8);
        self.write_u8(seg, off.wrapping_add(1), (val >> 8) as u8);
    }

    pub fn push_u16(&mut self, cpu: &mut CpuContext, val: u16) {
        cpu.sp = cpu.sp.wrapping_sub(2);
        self.write_u16(cpu.ss, cpu.sp, val);
    }

    pub fn pop_u16(&mut self, cpu: &mut CpuContext) -> u16 {
        let val = self.read_u16(cpu.ss, cpu.sp);
        cpu.sp = cpu.sp.wrapping_add(2);
        val
    }

    /// Read an interrupt vector (segment, offset) from the IVT.
    pub fn get_ivt_vector(&self, int_num: u8) -> (u16, u16) {
        let ivt_phys = (int_num as usize) * 4;
        let off = u16::from_le_bytes([self.ram[ivt_phys], self.ram[ivt_phys + 1]]);
        let seg = u16::from_le_bytes([self.ram[ivt_phys + 2], self.ram[ivt_phys + 3]]);
        (seg, off)
    }

    /// Invoke a hardware/software interrupt vector from the IVT.
    pub fn trigger_interrupt(&mut self, cpu: &mut CpuContext, int_num: u8) -> bool {
        if !cpu.flags.if_ {
            return false;
        }
        let (seg, off) = self.get_ivt_vector(int_num);
        // Only invoke if vector points to loaded code (segment >= 0x1010)
        if seg >= 0x1010 {
            let flags = cpu.flags.to_u16();
            let cs = cpu.cs;
            let ip = cpu.ip;
            self.push_u16(cpu, flags);
            self.push_u16(cpu, cs);
            self.push_u16(cpu, ip);
            cpu.flags.if_ = false;
            cpu.flags.tf = false;
            cpu.cs = seg;
            cpu.ip = off;
            true
        } else {
            false
        }
    }

    /// Called when the game engine finishes rendering a frame to EGA memory.
    pub fn on_engine_frame_boundary(&mut self) {
        if !self.ega_dirty {
            return;
        }
        self.ega_dirty = false;

        // Decode new EGA planar memory into temporary buffer
        let mut new_frame = vec![0u32; 320 * 200];
        let mut pixel_idx = 0;
        for y in 0..200 {
            let row_off = y * 40;
            for byte_idx in 0..40 {
                let off = row_off + byte_idx;
                let b0 = self.ega.planes[0][off];
                let b1 = self.ega.planes[1][off];
                let b2 = self.ega.planes[2][off];
                let b3 = self.ega.planes[3][off];

                for bit in (0..8).rev() {
                    let c0 = (b0 >> bit) & 1;
                    let c1 = (b1 >> bit) & 1;
                    let c2 = (b2 >> bit) & 1;
                    let c3 = (b3 >> bit) & 1;
                    let raw_color = (c3 << 3) | (c2 << 2) | (c1 << 1) | c0;
                    let mapped_color = self.ega.palette_lut[raw_color as usize] as usize;
                    let color = match self.mode {
                        VideoAudioMode::Dos => EGA_DEFAULT_PALETTE[mapped_color & 0x0F],
                        VideoAudioMode::C64 => EGA_TO_C64_MAP[mapped_color & 0x0F],
                    };
                    new_frame[pixel_idx] = color;
                    pixel_idx += 1;
                }
            }
        }

        // Scene Cut Detection: If >40% pixels changed, don't interpolate (e.g. menu change, crash, qualify)
        let mut diff_count = 0usize;
        for i in 0..64000 {
            if new_frame[i] != self.curr_frame[i] {
                diff_count += 1;
            }
        }

        let is_scene_cut = diff_count > 25_000;

        if is_scene_cut {
            self.motion_dx.fill(0);
            self.motion_dy.fill(0);
        } else {
            // Optical Flow / Scanline Motion Estimation
            // 1. Sky & Horizon (y = 0..48): Global horizontal shift dx_sky
            let mut best_sky_dx = 0i16;
            let mut min_sky_diff = usize::MAX;
            for dx in -16isize..=16isize {
                let mut diff = 0usize;
                for check_y in [16usize, 28, 40] {
                    let off = check_y * 320;
                    let (sc, sp, len) = if dx >= 0 {
                        (dx as usize, 0usize, 320 - dx as usize)
                    } else {
                        (0usize, (-dx) as usize, 320 - (-dx) as usize)
                    };
                    for x in 0..len {
                        if new_frame[off + sc + x] != self.curr_frame[off + sp + x] {
                            diff += 1;
                        }
                    }
                }
                let score = diff + (dx.abs() as usize) * 2;
                if score < min_sky_diff {
                    min_sky_diff = score;
                    best_sky_dx = dx as i16;
                }
            }

            for y in 0..49 {
                self.motion_dx[y] = best_sky_dx;
                self.motion_dy[y] = 0;
            }

            // 2. 3D Track & Road (y = 49..135): Per-scanline horizontal + vertical shift
            let mut prev_dx = best_sky_dx;
            for y in 49..136 {
                let mut best_dx = prev_dx;
                let mut best_dy = 0i16;
                let mut min_diff = usize::MAX;

                let min_search_dx = (-16isize).max((prev_dx as isize) - 4);
                let max_search_dx = (16isize).min((prev_dx as isize) + 4);

                for dy in 0..=2 {
                    if y < (dy as usize) {
                        continue;
                    }
                    let prev_y = y - (dy as usize);
                    let off_curr = y * 320;
                    let off_prev = prev_y * 320;

                    for dx in min_search_dx..=max_search_dx {
                        let (sc, sp, len) = if dx >= 0 {
                            (dx as usize, 0usize, 320 - dx as usize)
                        } else {
                            (0usize, (-dx) as usize, 320 - (-dx) as usize)
                        };

                        let mut diff = 0usize;
                        for x in 0..len {
                            if new_frame[off_curr + sc + x] != self.curr_frame[off_prev + sp + x] {
                                diff += 1;
                            }
                        }

                        let score = (diff * 320) / len + (dx.abs() as usize) + (dy as usize) * 3;
                        if score < min_diff {
                            min_diff = score;
                            best_dx = dx as i16;
                            best_dy = dy as i16;
                        }
                    }
                }

                self.motion_dx[y] = best_dx;
                self.motion_dy[y] = best_dy;
                prev_dx = best_dx;
            }

            // 3. Cockpit & Dashboard (y = 136..199): Stationary, pinned, sharp
            for y in 136..200 {
                self.motion_dx[y] = 0;
                self.motion_dy[y] = 0;
            }
        }

        self.prev_frame.copy_from_slice(&self.curr_frame);
        self.curr_frame.copy_from_slice(&new_frame);
        self.frame_instant = std::time::Instant::now();
    }

    /// Render 320x200 EGA 4-plane buffer to a 32-bit ARGB framebuffer with 60 FPS spatial motion interpolation.
    pub fn render_ega_to_rgb(&mut self, dest: &mut [u32]) {
        assert_eq!(dest.len(), 320 * 200);

        // Fallback: If EGA memory was updated but 0x681E wasn't reached within 45ms (e.g. menus), capture now
        if self.ega_dirty && self.frame_instant.elapsed().as_millis() >= 45 {
            self.on_engine_frame_boundary();
        }

        if self.smooth_mode {
            let elapsed = self.frame_instant.elapsed().as_secs_f32();
            let t = (elapsed / 0.050).clamp(0.0, 1.0);

            if t >= 1.0 {
                dest.copy_from_slice(&self.curr_frame);
            } else {
                for y in 0..200 {
                    let row_start = y * 320;
                    if y >= 136 {
                        // Cockpit / Dashboard: 100% stationary and razor sharp
                        dest[row_start..row_start + 320].copy_from_slice(&self.curr_frame[row_start..row_start + 320]);
                    } else {
                        // Viewport: Spatial Scanline Motion Displacement (Zero Blur!)
                        let dx_full = self.motion_dx[y] as f32;
                        let dy_full = self.motion_dy[y] as f32;

                        if t < 0.5 {
                            // Forward sample from prev_frame
                            let dx = (t * dx_full).round() as isize;
                            let dy = (t * dy_full).round() as isize;
                            let sy = (y as isize - dy).clamp(0, 135) as usize;
                            let src_row = sy * 320;

                            for x in 0..320 {
                                let sx = (x as isize - dx).clamp(0, 319) as usize;
                                dest[row_start + x] = self.prev_frame[src_row + sx];
                            }
                        } else {
                            // Backward sample from curr_frame
                            let dx = ((1.0 - t) * dx_full).round() as isize;
                            let dy = ((1.0 - t) * dy_full).round() as isize;
                            let sy = (y as isize + dy).clamp(0, 135) as usize;
                            let src_row = sy * 320;

                            for x in 0..320 {
                                let sx = (x as isize + dx).clamp(0, 319) as usize;
                                dest[row_start + x] = self.curr_frame[src_row + sx];
                            }
                        }
                    }
                }
            }
        } else {
            dest.copy_from_slice(&self.curr_frame);
        }

        // Draw on-screen mode indicator badge when recently switched
        if self.osd_frames > 0 {
            self.osd_frames -= 1;
            self.draw_osd_badge(dest);
        }
    }

    fn draw_osd_badge(&self, dest: &mut [u32]) {
        let text = if !self.osd_text.is_empty() {
            &self.osd_text
        } else {
            match self.mode {
                VideoAudioMode::C64 => "[ C64 MODE ]",
                VideoAudioMode::Dos => "[ DOS MODE ]",
            }
        };

        let (bg_color, fg_color, border_color) = match self.mode {
            VideoAudioMode::C64 => (0x002E2C9B, 0x0075CEC8, 0x00EDF171),
            VideoAudioMode::Dos => (0x00000000, 0x0055FF55, 0x00FFFFFF),
        };

        let width = text.len() * 6 + 6;
        let start_x = 316usize.saturating_sub(width);
        let start_y = 6usize;
        let height = 13usize;

        // Draw badge background & border
        for py in 0..height {
            for px in 0..width {
                let x = start_x + px;
                let y = start_y + py;
                if x < 320 && y < 200 {
                    let is_border = px == 0 || px == width - 1 || py == 0 || py == height - 1;
                    dest[y * 320 + x] = if is_border { border_color } else { bg_color };
                }
            }
        }

        // Draw 5x7 characters
        let mut cur_x = start_x + 4;
        let text_y = start_y + 3;
        for c in text.chars() {
            let glyph = get_glyph_5x7(c);
            for (row_idx, &row_bits) in glyph.iter().enumerate() {
                let y = text_y + row_idx;
                for col_idx in 0..5 {
                    if (row_bits & (1 << (4 - col_idx))) != 0 {
                        let x = cur_x + col_idx;
                        if x < 320 && y < 200 {
                            dest[y * 320 + x] = fg_color;
                        }
                    }
                }
            }
            cur_x += 6;
        }
    }
}

fn get_glyph_5x7(c: char) -> [u8; 7] {
    match c {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        '[' => [0b01110, 0b01000, 0b01000, 0b01000, 0b01000, 0b01000, 0b01110],
        ']' => [0b01110, 0b00010, 0b00010, 0b00010, 0b00010, 0b00010, 0b01110],
        _   => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
    }
}
