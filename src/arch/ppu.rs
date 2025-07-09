use std::fmt::{Display, Formatter};
use proc_bitfield::bitfield;
use tracing::trace;
use crate::arch::{Nes, ClockDivider};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct PixelPos {
    pub cycle: u16,
    pub scanline: u16,
    pub is_odd: bool,
}
impl Default for PixelPos {
    fn default() -> Self { Self {
        cycle: 0,
        scanline: 261,
        is_odd: true, //TODO: Check what the initial state should be
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
                self.is_odd = !self.is_odd;
                
                if self.is_odd {
                    self.cycle = 1;
                }
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
    
    pub fn increment_coarse_x(&mut self) {
        if self.coarse_x() == 31 {
            self.set_coarse_x(0);
            self.0 ^= 0x0400;
        } else {
            self.0 += 1;
        }
    }
    
    pub fn increment_fine_y(&mut self) {
        if self.fine_y() == 7 {
            self.set_fine_y(0);
            
            if self.coarse_y() == 29 {
                self.set_coarse_y(0);
                self.0 ^= 0x0800;
            } else if self.coarse_y() == 31 {
                self.set_coarse_y(0);
            } else {
                self.set_coarse_y(self.coarse_y() + 1);
            }
        } else {
            self.set_fine_y(self.fine_y() + 1);
        }
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
    /// The `v` register
    vram_addr: VramAddr,
    /// The `t` register
    tmp_vram_addr: VramAddr,
    
    internal_bus_addr: u16,
    nametable_lat: u8,
    attribute_lat: u8,
    pattern_lower_lat: u8,
    pattern_upper_lat: u8,
    
    shift_attrib: u8,
    shift_lower: u16,
    shift_upper: u16,
    
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
        
        internal_bus_addr: 0,
        nametable_lat: 0,
        attribute_lat: 0,
        pattern_lower_lat: 0,
        pattern_upper_lat: 0,
        
        shift_attrib: 0,
        shift_lower: 0, //TODO: Check the initial state; maybe it's 0xFFFF?
        shift_upper: 0, //TODO: Check the initial state; maybe it's 0xFFFF?
        
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
        let cycle = nes.ppu.pos.cycle;
        let line = nes.ppu.pos.scanline;
        match line {
            0..=239 | 261 => {
                if line == 261 && cycle == 1 {
                    nes.ppu.vblank = false;
                    //TODO: clear sprite overflow and sprite 0 hit bits
                }
                if line == 261 && (280..=304).contains(&cycle) && (nes.ppu.mask.show_background() || nes.ppu.mask.show_sprites()) {
                    nes.ppu.vram_addr.0 = (nes.ppu.vram_addr.0 & !0x7BE0) | (nes.ppu.tmp_vram_addr.0 & !0x7BE0);
                }
                
                if nes.ppu.mask.show_background() || nes.ppu.mask.show_sprites() {
                    if cycle == 0 {
                        nes.ppu.internal_bus_addr = ((nes.ppu.ctrl.background_pattern_addr() as u16) << 12) | ((nes.ppu.nametable_lat as u16) << 4) | (nes.ppu.vram_addr.fine_y() as u16);
                        //TODO: Determine what more needs to be done here, if anything
                    }
                    
                    if (1..=256).contains(&cycle) {
                        match cycle % 8 {
                            1 => nes.ppu.internal_bus_addr = 0x2000 | (nes.ppu.vram_addr.0 & 0x0FFF),
                            2 => nes.ppu.nametable_lat = Ppu::read(nes, nes.ppu.internal_bus_addr),
                            
                            3 => nes.ppu.internal_bus_addr = 0x23C0 | (nes.ppu.vram_addr.0 & 0x0C00) | ((nes.ppu.vram_addr.0 >> 4) & 0x38) | ((nes.ppu.vram_addr.0 >> 2) & 0x07),
                            4 => nes.ppu.attribute_lat = Ppu::read(nes, nes.ppu.internal_bus_addr),
                            
                            5 => nes.ppu.internal_bus_addr = ((nes.ppu.ctrl.background_pattern_addr() as u16) << 12) | ((nes.ppu.nametable_lat as u16) << 4) | (nes.ppu.vram_addr.fine_y() as u16),
                            6 => nes.ppu.pattern_lower_lat = Ppu::read(nes, nes.ppu.internal_bus_addr),
                            
                            7 => nes.ppu.internal_bus_addr = ((nes.ppu.ctrl.background_pattern_addr() as u16) << 12) | ((nes.ppu.nametable_lat as u16) << 4) | 0b1000 | (nes.ppu.vram_addr.fine_y() as u16),
                            0 => {
                                nes.ppu.pattern_upper_lat = Ppu::read(nes, nes.ppu.internal_bus_addr);
                                
                                nes.ppu.vram_addr.increment_coarse_x();
                                
                                if cycle != 1 {
                                    nes.ppu.shift_attrib = nes.ppu.attribute_lat;
                                    nes.ppu.shift_lower = (nes.ppu.shift_lower & 0xFF00) | (nes.ppu.pattern_lower_lat as u16);
                                    nes.ppu.shift_upper = (nes.ppu.shift_upper & 0xFF00) | (nes.ppu.pattern_upper_lat as u16);
                                }
                            },
                            
                            _ => unreachable!()
                        }
                    }
                    
                    if cycle == 256 {
                        nes.ppu.vram_addr.increment_fine_y();
                    }
                    
                    if cycle == 257 {
                        nes.ppu.vram_addr.0 = (nes.ppu.vram_addr.0 & !0x041F) | (nes.ppu.tmp_vram_addr.0 & !0x041F);
                    }
                    
                    if (321..=340).contains(&cycle) {
                        //TODO
                    }
                }
                
                if (2..=337).contains(&cycle) {
                    nes.ppu.shift_lower <<= 1;
                    nes.ppu.shift_lower |= 1;
                    
                    nes.ppu.shift_upper <<= 1;
                    nes.ppu.shift_upper |= 1;
                }
            },
            241 if cycle == 1 => {
                nes.ppu.vblank = true;
                Ppu::update_nmi_output(nes);
            },
            _ => ()
        }
        
        if nes.ppu.mask.show_background() || nes.ppu.mask.show_sprites() {
            nes.ppu.draw_pixel();
        }
        
        nes.ppu.pos.inc();
        nes.ppu.cycles_since_pwrrst += 1;
        if !nes.ppu.ctrl_unlocked && nes.ppu.cycles_since_pwrrst >= 30000 { //TODO: Confirm the precise number of cycles here
            nes.ppu.ctrl_unlocked = true;
        }
    }
    
    //Taken from https://github.com/sarchar/RetroDisassemblerStudio/blob/0508182f1ae0ef26477b3b918c41bdd39db52133/src/systems/nes/ppu.cpp#L734
    fn draw_pixel(&mut self) {
        let bit_lower = ((self.shift_lower >> (15 - (self.fine_x_scroll as u16))) & 1) as u8;
        let bit_upper = ((self.shift_upper >> (15 - (self.fine_x_scroll as u16))) & 1) as u8;
        
        //TODO: Figure out what is actually happening below
        
        let x_pos = ((self.vram_addr.coarse_x() << 3) | self.fine_x_scroll) + (self.pos.cycle as u8 & 0x07);
        let attr_x = (x_pos - 17) & 0x1F;
        
        let actual_attrib = if (((self.pos.cycle - 1) & 0x07) + self.fine_x_scroll as u16) >= 8 { self.attribute_lat } else { self.shift_attrib };
        
        let y_pos = (self.vram_addr.coarse_y() << 3) | self.vram_addr.fine_y();
        let y_shift = (y_pos & 0x10) >> 2;
        
        let x_shift = (attr_x & 0x10) >> 3;
        let attr = (actual_attrib >> (y_shift + x_shift)) & 0x03;
        let pal_index = (attr << 2) | (bit_upper << 1) | bit_lower;
        
        //TODO: Add sprite selection
        
        let color = self.pal_values[pal_index as usize];
        if let Some(pixel) = self.fb.get_mut(((self.pos.scanline as usize * 256) + self.pos.cycle as usize) - 15) {
            *pixel = color;
        }
    }
    
    fn update_nmi_output(nes: &mut Nes) {
        if nes.ppu.ctrl.generate_nmi() && nes.ppu.vblank {
            nes.cpu.nmi = false; // set LOW (NMI is active-low)
        }
    }
    
    /// Read from PPU memory map (may read into the cartridge)
    fn read(nes: &mut Nes, addr: u16) -> u8 {
        match addr & 0x3FFF { // address bus is only 14 bits wide
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
    
    /// Write to PPU memory map (may write into the cartridge)
    fn write(nes: &mut Nes, addr: u16, data: u8) {
        match addr & 0x3FFF { // address bus is only 14 bits wide
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
