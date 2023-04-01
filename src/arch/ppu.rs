use std::fmt::{Display, Formatter};
use crate::arch::{Nes, CpuBusAccessible, ClockDivider};
use crate::util::InfCell;

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



#[derive(Clone, Debug)]
pub struct Ppu {
    ports_latch: u8,
    ports_unlocked: bool,
    clock_divider: ClockDivider<4>,
    
    pub pos: PixelPos,
    vblank: bool,
}
impl Default for Ppu {
    fn default() -> Self { Self {
        ports_latch: 0,
        ports_unlocked: false,
        clock_divider: ClockDivider::new(0), //todo: randomize

        pos: PixelPos::default(),
        vblank: false,
    }}
}
impl Ppu {
    pub fn tick(&mut self, nes_cell: &InfCell<Nes>) {
        if self.clock_divider.tick() {
            self.cycle(nes_cell);
        }
    }
    
    pub fn cycle(&mut self, nes_cell: &InfCell<Nes>) {
        let _bus = nes_cell.get_mut();
        
        match self.pos.cycle {
            1 if self.pos.scanline == 241 => self.vblank = true,
            1 if self.pos.scanline == 261 => {
                self.vblank = false;
                //TODO: clear sprite overflow and sprite 0 hit bits
            },
            _ => ()
        }
        
        self.pos.inc();
    }
    
    fn port_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x2000 => (),
            0x2001 => (),
            0x2002 => self.ports_latch |= (self.vblank as u8) << 7, //TODO: add sprite overflow and sprite 0 hit detection to status register
            0x2003 => (),
            0x2004 => unimplemented!("PPU read from {:#06X}", addr),
            0x2005 => (),
            0x2006 => (),
            0x2007 => unimplemented!("PPU read from {:#06X}", addr),
            
            _ => unreachable!()
        }
        
        self.ports_latch
    }
    
    fn port_write(&mut self, addr: u16, data: u8) {
        self.ports_latch = data;
        
        unimplemented!("PPU write {:#04X} to {:#06X}", data, addr);
    }
}

impl CpuBusAccessible for Ppu {
    fn write(&mut self, addr: u16, data: u8) {
        match addr {
            0x2000..=0x3FFF => self.port_write(addr & 0x2007, data),
            _ => panic!("Write attempt to invalid address {:#06X} ({:#04X})", addr, data),
        }
    }
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x2000..=0x3FFF => self.port_read(addr & 0x2007),
            _ => panic!("Read attempt to invalid address {:#06X}", addr),
        }
    }
}