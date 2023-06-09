use crate::arch::cartridge::Cartridge;
use crate::arch::cpu::Cpu;
use crate::arch::mappers::RomFile;
use crate::arch::ppu::Ppu;

pub mod cartridge;
pub mod cpu;
pub mod mappers;
pub mod ppu;

pub trait CpuBusAccessible {
    fn write(&mut self, addr: u16, data: u8);
    fn read(&mut self, addr: u16) -> u8;
}

/// Collection of major components found within the NES.
/// 
/// To simplify the access of data from different system components, this struct holds all the major
/// parts, technically giving all components access to all other components. It's up to each
/// component to accurately restrict access.
/// 
/// For example, the real PPU only has access to the cartridge's CHR RAM/ROM. But to split the
/// cartridge into separate data structures would increase code complexity for little advantage. So
/// here it is represented as a single entity.
/// 
/// The [`Nes`] and all components in it, implement the [`CpuBusAccessible`] trait. Methods exposed
/// by this trait, are accessed with respect to the CPU's memory map. The PPU's own memory map is
/// NOT directly accessible through implementations of this trait.
#[derive(Debug, Default, Clone)]
pub struct Nes {
    pub cpu: Cpu,
    pub ppu: Ppu,
    pub cart: Cartridge,
}
impl Nes {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn tick(&mut self) {
        Cpu::tick(self);
        Ppu::tick(self);
    }
    
    pub fn load_rom(&mut self, rom: RomFile) {
        self.cart.mapper = rom.into_mapper();
        Cpu::init_pc(self);
    }
}
impl CpuBusAccessible for Nes {
    fn write(&mut self, addr: u16, data: u8) {
        if addr == 0x0647 {
            println!("### Wrote {:#04X} to {:#06X}", data, addr);
        } else {
            println!("    Wrote {:#04X} to {:#06X}", data, addr);
        }
        match addr {
            0x0000..=0x1FFF => self.cpu.write(addr, data),
            0x2000..=0x3FFF => self.ppu.write(addr, data),
            0x4000..=0x4017 => (),
            0x4018..=0x401F => panic!("Write attempt to CPU Test Mode at address {:#06X} ({:#04X})", addr, data),
            0x4020..=0xFFFF => self.cart.write_cpu(addr, data),
            //_ => panic!("Write attempt to invalid address {:#06X} ({:#04X})", addr, data),
        }
    }

    fn read(&mut self, addr: u16) -> u8 {
        let val = match addr {
            0x0000..=0x1FFF => self.cpu.read(addr),
            0x2000..=0x3FFF => self.ppu.read(addr),
            0x4000..=0x4017 => 0,
            0x4018..=0x401F => panic!("Read attempt to CPU Test Mode at address {:#06X}", addr),
            0x4020..=0xFFFF => self.cart.read_cpu(addr),
            //_ => panic!("Read attempt to invalid address {:#06X}", addr),
        };
        
        if addr == 0x0647 {
            println!("### Read {:#04X} from {:#06X}", val, addr);
        } else {
            //println!("    Read {:#04X} from {:#06X}", val, addr);
        }
        val
    }
}


#[derive(Clone, Debug)]
pub struct ClockDivider<const N: usize> {
    pub counter: usize,
}
impl<const N: usize> ClockDivider<N> {
    pub fn new(initial: usize) -> Self { Self {
        counter: initial
    }}
    
    pub fn tick(&mut self) -> bool {
        self.counter += 1;
        if self.counter == N {
            self.counter = 0;
            
            true
        } else {
            false
        }
    }
}