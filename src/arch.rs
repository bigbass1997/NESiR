use crate::arch::cartridge::Cartridge;
use crate::arch::cpu::Cpu;
use crate::arch::mappers::RomFile;
use crate::arch::ppu::Ppu;

pub mod cartridge;
pub mod cpu;
pub mod mappers;
pub mod ppu;

/// Collection of major components found within the NES.
/// 
/// To simplify the access of data from different system components, this struct holds all the major
/// parts, technically giving all components access to all other components. It's up to each
/// component to accurately restrict access.
/// 
/// For example, the real PPU only has access to the cartridge's CHR RAM/ROM. But to split the
/// cartridge into separate data structures would increase code complexity for little advantage. So
/// here it is represented as a single entity.
#[derive(Debug, Default, Clone)]
pub struct Nes {
    pub cpu: Cpu,
    pub ppu: Ppu,
    pub cart: Cartridge,
    
    pub last_bus: BusActivity,
}
impl Nes {
    pub fn new() -> Self {
        Self::default()
    }
    
    #[inline(always)]
    pub fn tick(&mut self) {
        Cpu::tick(self);
        Ppu::tick(self);
    }
    
    pub fn load_rom(&mut self, rom: RomFile) {
        self.cart.mapper = rom.into_mapper();
        Cpu::init_pc(self);
    }
    
    /// Write to the CPU's external bus.
    /// 
    /// This bus is connected to the 2A03 CPU (including the APU and other internal components), PPU, and the cartridge.
    #[cfg(not(feature = "sst"))]
    pub fn write(&mut self, addr: u16, data: u8) {
        self.cpu.predecode = data;
        
        match addr {
            0x0000..=0x1FFF | 0x4014 => self.cpu.internal_write(addr, data),
            0x2000..=0x3FFF => Ppu::port_write(self, addr, data),
            0x4000..=0x4017 => (),
            0x4018..=0x401F => panic!("Write attempt to CPU Test Mode at address {:#06X} ({:#04X})", addr, data),
            0x4020..=0xFFFF => self.cart.write_cpu(addr, data),
        }
        
        self.last_bus = BusActivity { addr, data, is_read: false };
    }
    
    /// Read from the CPU's external bus.
    /// 
    /// This bus is connected to the 2A03 CPU (including the APU and other internal components), PPU, and the cartridge.
    #[cfg(not(feature = "sst"))]
    pub fn read(&mut self, addr: u16) -> u8 {
        let data = match addr {
            0x0000..=0x1FFF => self.cpu.internal_read(addr),
            0x2000..=0x3FFF => Ppu::port_read(self, addr),
            0x4000..=0x4017 => 0,
            0x4018..=0x401F => panic!("Read attempt to CPU Test Mode at address {:#06X}", addr),
            0x4020..=0xFFFF => self.cart.read_cpu(addr),
        };
        
        self.cpu.predecode = data;
        
        self.last_bus = BusActivity { addr, data, is_read: true };
        
        data
    }
    
    #[cfg(feature = "sst")]
    pub fn write(&mut self, addr: u16, data: u8) {
        self.cpu.predecode = data;
        
        self.cpu.internal_write(addr, data);
        
        self.last_bus = BusActivity { addr, data, is_read: false };
    }
    
    #[cfg(feature = "sst")]
    pub fn read(&mut self, addr: u16) -> u8 {
        let data = self.cpu.internal_read(addr);
        
        self.cpu.predecode = data;
        
        self.last_bus = BusActivity { addr, data, is_read: true };
        
        data
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BusActivity {
    pub addr: u16,
    pub data: u8,
    pub is_read: bool,
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