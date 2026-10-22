/// x86 16-bit CPU context for MS-DOS real-mode execution.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flags {
    pub cf: bool, // Carry
    pub pf: bool, // Parity
    pub af: bool, // Auxiliary Carry
    pub zf: bool, // Zero
    pub sf: bool, // Sign
    pub tf: bool, // Trap
    pub if_: bool, // Interrupt Enable
    pub df: bool, // Direction (0 = increment, 1 = decrement)
    pub of: bool, // Overflow
}

impl Default for Flags {
    fn default() -> Self {
        Self {
            cf: false,
            pf: false,
            af: false,
            zf: false,
            sf: false,
            tf: false,
            if_: true, // Interrupts enabled by default in DOS
            df: false,
            of: false,
        }
    }
}

impl Flags {
    pub fn to_u16(&self) -> u16 {
        let mut val = 0x0002; // Bit 1 is always 1 in 8086 flags
        if self.cf { val |= 1 << 0; }
        if self.pf { val |= 1 << 2; }
        if self.af { val |= 1 << 4; }
        if self.zf { val |= 1 << 6; }
        if self.sf { val |= 1 << 7; }
        if self.tf { val |= 1 << 8; }
        if self.if_ { val |= 1 << 9; }
        if self.df { val |= 1 << 10; }
        if self.of { val |= 1 << 11; }
        val
    }

    pub fn from_u16(&mut self, val: u16) {
        self.cf = (val & (1 << 0)) != 0;
        self.pf = (val & (1 << 2)) != 0;
        self.af = (val & (1 << 4)) != 0;
        self.zf = (val & (1 << 6)) != 0;
        self.sf = (val & (1 << 7)) != 0;
        self.tf = (val & (1 << 8)) != 0;
        self.if_ = (val & (1 << 9)) != 0;
        self.df = (val & (1 << 10)) != 0;
        self.of = (val & (1 << 11)) != 0;
    }
}

#[derive(Debug, Clone, Default)]
pub struct CpuContext {
    // 16-bit general registers
    pub ax: u16,
    pub cx: u16,
    pub dx: u16,
    pub bx: u16,
    pub sp: u16,
    pub bp: u16,
    pub si: u16,
    pub di: u16,

    // Segment registers
    pub es: u16,
    pub cs: u16,
    pub ss: u16,
    pub ds: u16,

    // Instruction pointer
    pub ip: u16,

    // Flags
    pub flags: Flags,

    // Execution control
    pub halted: bool,
}

impl CpuContext {
    pub fn new() -> Self {
        Self::default()
    }

    // 8-bit register accessors
    #[inline]
    pub fn al(&self) -> u8 { self.ax as u8 }
    #[inline]
    pub fn set_al(&mut self, val: u8) { self.ax = (self.ax & 0xFF00) | (val as u16); }

    #[inline]
    pub fn ah(&self) -> u8 { (self.ax >> 8) as u8 }
    #[inline]
    pub fn set_ah(&mut self, val: u8) { self.ax = (self.ax & 0x00FF) | ((val as u16) << 8); }

    #[inline]
    pub fn cl(&self) -> u8 { self.cx as u8 }
    #[inline]
    pub fn set_cl(&mut self, val: u8) { self.cx = (self.cx & 0xFF00) | (val as u16); }

    #[inline]
    pub fn ch(&self) -> u8 { (self.cx >> 8) as u8 }
    #[inline]
    pub fn set_ch(&mut self, val: u8) { self.cx = (self.cx & 0x00FF) | ((val as u16) << 8); }

    #[inline]
    pub fn dl(&self) -> u8 { self.dx as u8 }
    #[inline]
    pub fn set_dl(&mut self, val: u8) { self.dx = (self.dx & 0xFF00) | (val as u16); }

    #[inline]
    pub fn dh(&self) -> u8 { (self.dx >> 8) as u8 }
    #[inline]
    pub fn set_dh(&mut self, val: u8) { self.dx = (self.dx & 0x00FF) | ((val as u16) << 8); }

    #[inline]
    pub fn bl(&self) -> u8 { self.bx as u8 }
    #[inline]
    pub fn set_bl(&mut self, val: u8) { self.bx = (self.bx & 0xFF00) | (val as u16); }

    #[inline]
    pub fn bh(&self) -> u8 { (self.bx >> 8) as u8 }
    #[inline]
    pub fn set_bh(&mut self, val: u8) { self.bx = (self.bx & 0x00FF) | ((val as u16) << 8); }

    // Flag update helpers for ALU operations
    #[inline]
    pub fn update_szp_u8(&mut self, res: u8) {
        self.flags.zf = res == 0;
        self.flags.sf = (res & 0x80) != 0;
        self.flags.pf = res.count_ones() % 2 == 0;
    }

    #[inline]
    pub fn update_szp_u16(&mut self, res: u16) {
        self.flags.zf = res == 0;
        self.flags.sf = (res & 0x8000) != 0;
        self.flags.pf = (res as u8).count_ones() % 2 == 0;
    }
}
