
//! iNES 000

use crate::arch::mappers::{Mapper, RomFile};

/// Alias for mapper number 000
pub type NRom = Mapper000;

#[derive(Debug, Copy, Clone)]
pub struct Mapper000 {
    /// Family Basic only, but seems most emus provide 8 KiB?
    pub prg_ram: [u8; 0x2000],
    pub prg_rom: [u8; 0x8000],
    pub chr_rom: [u8; 0x2000],
    //pub ciram_a10: bool,
    //pub ciram__ce: bool,
}
impl Mapper for Mapper000 {
    fn new(rom: RomFile) -> Box<dyn Mapper> {
        Box::new(Mapper000 {
            prg_ram: [0u8; 0x2000],
            prg_rom: if rom.prg.len() == 0x4000 {
                let mut data = rom.prg.to_vec();
                data.extend_from_slice(&rom.prg);
                
                data.try_into().unwrap()
            } else {
                rom.prg.try_into().unwrap()
            },
            chr_rom: {
                let mut data = rom.chr.clone();
                data.resize(0x2000, 0);
                
                data.try_into().unwrap()
            },
        })
    }
    
    fn read_cpu(&mut self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr & 0x1FFF) as usize],
            0x8000..=0xFFFF => self.prg_rom[(addr & 0x7FFF) as usize],
            _ => panic!("Read attempt to invalid address {:#06X}", addr),
        }
    }
    
    fn write_cpu(&mut self, addr: u16, data: u8) {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr & 0x1FFF) as usize] = data,
            0x8000..=0xFFFF => (),
            _ => panic!("Read attempt to invalid address {:#06X}", addr),
        }
    }
    
    fn read_ppu(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_rom[addr as usize],
            _ => unimplemented!()
        }
    }
    
    fn write_ppu(&mut self, _addr: u16, _data: u8) {
        // do nothing
    }
}