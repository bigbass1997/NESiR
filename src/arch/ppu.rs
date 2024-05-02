use std::fmt::{Display, Formatter};
use log::trace;
use proc_bitfield::bitfield;
use crate::arch::{Nes, ClockDivider};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct PixelPos {
    pub cycle: u16,
    pub scanline: u16,
    //TODO: Implement even/odd frame dynamics
}
impl Default for PixelPos {
    fn default() -> Self { Self {
        cycle: 0,
        scanline: 261
    }}
}
impl Display for PixelPos {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:3}, {:3}", self.cycle, self.scanline)
    }
}
impl PixelPos {
    pub fn inc(&mut self) {
        self.cycle += 1;
        if self.cycle == 341 {
            self.cycle = 0;
            self.scanline += 1;
            
            if self.scanline == 262 {
                self.scanline = 0;
            }
        }
    }
}


bitfield! {
    #[derive(Copy, Clone, PartialEq, Eq)]
    pub struct CtrlReg(pub u8): Debug {
        pub base_nametable_addr: u8 @ 0..=1,
        pub vram_addr_inc: bool @ 2,
        pub sprite_pattern_addr: bool @ 3,
        pub background_pattern_addr: bool @ 4,
        pub sprite_size: bool @ 5,
        pub master_slave_select: bool @ 6,
        pub generate_nmi: bool @ 7,
    }
}

bitfield! {
    #[derive(Copy, Clone, PartialEq, Eq)]
    pub struct MaskReg(pub u8): Debug {
        pub greyscale: bool @ 0,
        pub show_background_left: bool @ 1,
        pub show_sprites_left: bool @ 2,
        pub show_background: bool @ 3,
        pub show_sprites: bool @ 4,
        /// green on PAL/Dendy
        pub emphasize_red: bool @ 5,
        /// red on PAL/Dendy
        pub emphasize_green: bool @ 6,
        pub emphasize_blue: bool @ 7,
    }
}

bitfield! {
    #[derive(Copy, Clone, PartialEq, Eq)]
    pub struct VramAddr(u16): Debug {
        pub coarse_x: u8 @ 0..=4,
        pub coarse_y: u8 @ 5..=9,
        pub fine_y: u8 @ 12..=14,
    }
}
impl VramAddr {
    pub fn read(self, nametable: CtrlReg) -> u16 {
        self.0 | ((nametable.base_nametable_addr() as u16) << 10)
    }
    
    pub fn write(&mut self, data: u16, nametable: &mut CtrlReg) {
        self.0 = data & 0x73FF;
        nametable.set_base_nametable_addr(((data & 0x0C00) >> 10) as u8);
    }
}


#[derive(Clone, Debug)]
pub struct Ppu {
    ports_latch: u8,
    cycles_since_pwrrst: usize,
    ctrl_unlocked: bool,
    write_toggle: bool,
    ctrl: CtrlReg,
    mask: MaskReg,
    oam_addr: u8,
    fine_x_scroll: u8,
    vram_addr: VramAddr,
    tmp_vram_addr: VramAddr,
    ppuaddr: u16,
    ppudata: u8,
    
    clock_divider: ClockDivider<4>,
    
    pub pos: PixelPos,
    /// aka nmi_occurred
    vblank: bool,
    //nmi_output: bool,
    
    pub fb: [u32; 256 * 240],
    
    /// Internal VRAM used for storing two nametables
    pub ciram: [u8; 0x800],
    pub palettes: [u8; 0x20],
    
    pub pal_values: [u32; 0x40],
}
impl Default for Ppu {
    fn default() -> Self { Self { //TODO: Research initial state of registers
        ports_latch: 0,
        cycles_since_pwrrst: 0,
        ctrl_unlocked: false,
        write_toggle: false,
        ctrl: CtrlReg(0),
        mask: MaskReg(0),
        oam_addr: 0,
        fine_x_scroll: 0,
        vram_addr: VramAddr(0),
        tmp_vram_addr: VramAddr(0),
        ppuaddr: 0,
        ppudata: 0,
        
        clock_divider: ClockDivider::new(0), //todo: randomize
        
        pos: PixelPos::default(),
        vblank: false,
        
        fb: [0u32; 256 * 240],
        
        ciram: [0u8; 0x800],
        palettes: [0u8; 0x20],
        
        pal_values: [0u32; 0x40],
    }}
}
impl Ppu {
    #[inline(always)]
    pub fn tick(nes: &mut Nes) {
        if nes.ppu.clock_divider.tick() {
            Ppu::cycle(nes);
        }
    }
    
    pub fn cycle(nes: &mut Nes) {
        match nes.ppu.pos.cycle {
            1 if nes.ppu.pos.scanline == 241 => {
                nes.ppu.vblank = true;
                Ppu::update_nmi_output(nes);
            },
            1 if nes.ppu.pos.scanline == 261 => {
                nes.ppu.vblank = false;
                //TODO: clear sprite overflow and sprite 0 hit bits
            },
            _ => ()
        }
        
        nes.ppu.pos.inc();
        nes.ppu.cycles_since_pwrrst += 1;
        if !nes.ppu.ctrl_unlocked && nes.ppu.cycles_since_pwrrst >= 30000 {
            nes.ppu.ctrl_unlocked = true;
        }
    }
    
    fn update_nmi_output(nes: &mut Nes) {
        if nes.ppu.ctrl.generate_nmi() && nes.ppu.vblank {
            nes.cpu.nmi = false; // set LOW (NMI is active-low)
        }
    }
    
    /// Read from PPU memory map (may read into the cartridge)
    fn read(nes: &mut Nes, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => nes.cart.read_ppu(addr),
            0x2000..=0x3EFF => {
                nes.ppu.ciram[(addr & 0x7FF) as usize] //TODO: Implement nametable mirroring
            }
            0x3F00..=0x3FFF => {
                let addr = (addr & 0x1F) as usize;
                
                if nes.ppu.vblank {
                    unimplemented!("PPU read from palette RAM indexes during VBLANK")
                } else {
                    let addr = match addr {
                        0x10 | 0x14 | 0x18 | 0x1C => addr & 0x0F,
                        addr => addr,
                    };
                    nes.ppu.palettes[addr]
                }
            },
            
            _ => unreachable!()
        }
    }
    
    /// Read to PPU memory map (may write into the cartridge)
    fn write(nes: &mut Nes, addr: u16, data: u8) {
        match addr {
            0x0000..=0x1FFF => {
                nes.cart.write_ppu(addr, data);
                
                trace!("PPU Pattern write {data:#04X} to {addr:#06X}");
            },
            0x2000..=0x3EFF => {
                nes.ppu.ciram[(addr & 0x7FF) as usize] = data; //TODO: Implement nametable mirroring
                
                trace!("PPU CIRAM write {data:#04X} to {addr:#06X}");
            },
            0x3F00..=0x3FFF => {
                let addr = (addr & 0x1F) as usize;
                let addr = match addr {
                    0x10 | 0x14 | 0x18 | 0x1C => addr & 0x0F,
                    addr => addr,
                };
                nes.ppu.palettes[addr] = data;
                
                trace!("PPU Palette write {data:#04X} to {addr:#06X}");
            },
            
            _ => unreachable!()
        }
    }
    
    pub fn port_read(nes: &mut Nes, addr: u16) -> u8 {
        match addr {
            0x2000 => (),
            0x2001 => (),
            0x2002 => {
                nes.ppu.ports_latch = ((nes.ppu.vblank as u8) << 7) | (nes.ppu.ports_latch & 0b00011111);
                nes.ppu.vblank = false;
                nes.ppu.write_toggle = false;
            }, //TODO: add sprite overflow and sprite 0 hit detection to status register
            0x2003 => (),
            0x2004 => unimplemented!("PPU read from {:#06X}", addr),
            0x2005 => (),
            0x2006 => (),
            0x2007 => {
                Ppu::read(nes, nes.ppu.vram_addr.0);
                
                let inc = if !nes.ppu.ctrl.vram_addr_inc() {
                    1
                } else {
                    32
                };
                nes.ppu.vram_addr.0 += inc;
            },
            
            _ => unreachable!()
        }
        
        nes.ppu.ports_latch //TODO: add latch decay over time
    }
    
    pub fn port_write(nes: &mut Nes, addr: u16, data: u8) {
        nes.ppu.ports_latch = data;
        
        match addr {
            0x2000 => if nes.ppu.ctrl_unlocked {
                nes.ppu.ctrl.0 = data;
                
                Ppu::update_nmi_output(nes);
            },
            0x2001 => nes.ppu.mask.0 = data,
            0x2002 => (),
            0x2003 => nes.ppu.oam_addr = data, //TODO: Add feature flag for 2C02G's OAM corruption
            0x2004 => {
                unimplemented!("PPU write {:#04X} to {:#06X}", data, addr);
            },
            0x2005 => {
                if !nes.ppu.write_toggle { // w = 0
                    nes.ppu.tmp_vram_addr.set_coarse_x(data >> 3);
                    nes.ppu.fine_x_scroll = data & 0b111;
                } else { // w = 1
                    nes.ppu.tmp_vram_addr.set_coarse_y(data >> 3);
                    nes.ppu.tmp_vram_addr.set_fine_y(data & 0b111);
                }
                
                nes.ppu.write_toggle = !nes.ppu.write_toggle;
            },
            0x2006 => {
                if !nes.ppu.write_toggle { // w = 0
                    nes.ppu.tmp_vram_addr.0 = ((data as u16 & 0x003F) << 8) | (nes.ppu.tmp_vram_addr.0 & 0x00FF);
                } else { // w = 1
                    nes.ppu.tmp_vram_addr.0 = (nes.ppu.tmp_vram_addr.0 & 0xFF00) | (data as u16);
                    nes.ppu.vram_addr = nes.ppu.tmp_vram_addr;
                }
                
                nes.ppu.write_toggle = !nes.ppu.write_toggle;
            },
            0x2007 => {
                Ppu::write(nes, nes.ppu.vram_addr.0, data);
                
                let inc = if !nes.ppu.ctrl.vram_addr_inc() {
                    1
                } else {
                    32
                };
                nes.ppu.vram_addr.0 += inc;
            },
            
            _ => unreachable!()
        }
    }
}
