use std::fmt::Debug;
use dyn_clone::DynClone;
use crate::arch::mappers::mapper000::Mapper000;

pub mod mapper000;

#[allow(unused_variables)]
pub trait Mapper: DynClone + Debug {
    /// Populate cartridge data from ROM file.
    fn new(rom: RomFile) -> Box<dyn Mapper> where Self: Sized;
    
    /// Read access on PRG bus.
    fn read_cpu(&mut self, addr: u16) -> u8 {
        0 //todo: open bus behavior
    }
    
    /// Write access on PRG bus.
    fn write_cpu(&mut self, addr: u16, data: u8) {}
    
    /// Read access on CHR bus.
    fn read_ppu(&mut self, addr: u16) -> u8 {
        0 //todo: open bus behavior
    }
    
    /// Write access on CHR bus.
    fn write_ppu(&mut self, addr: u16, data: u8) {}
    
    //fn ciram_a10(&self) -> bool {}
    //fn ciram__ce(&self) -> bool {}
}
dyn_clone::clone_trait_object!(Mapper);

#[derive(Debug, Copy, Clone, Default)]
pub struct DummyMapper {}
impl Mapper for DummyMapper {
    fn new(_: RomFile) -> Box<dyn Mapper> where Self: Sized {
        Box::new(Self {})
    }
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct RomFile {
    pub header: [u8; 16],
    pub trainer: Option<[u8; 512]>,
    pub prg: Vec<u8>,
    pub chr: Vec<u8>,
    pub inst_rom: Option<[u8; 8192]>,
    pub prom: Option<([u8; 16], [u8; 16])>,
}
impl RomFile {
    pub fn new<T: AsRef<[u8]>>(data: T) -> Self {
        let data = data.as_ref();
        let mut rom = Self::default();
        
        rom.header = data[0..16].try_into().unwrap();
        let mut ptr = 16;
        if data[6] & 0x04 != 0 {
            rom.trainer = Some(data[ptr..(ptr + 512)].try_into().unwrap());
            ptr += 512;
        }
        
        let units = data[4] as usize;
        rom.prg = data[ptr..(ptr + (16384 * units))].to_vec();
        ptr += rom.prg.len();
        
        let units = data[5] as usize;
        rom.chr = data[ptr..(ptr + (8192 * units))].to_vec();
        ptr += rom.chr.len();
        
        if data[7] & 0x02 != 0 {
            rom.inst_rom = Some(data[ptr..(ptr + 8192)].try_into().unwrap());
            ptr += 8192;
            rom.prom = Some((
                data[ptr..(ptr + 16)].try_into().unwrap(),
                data[(ptr + 16)..(ptr + 32)].try_into().unwrap()
            ));
        }
        
        rom
    }
    
    #[inline(always)]
    pub fn is_ines2(&self) -> bool {
        ((self.header[7] >> 2) & 0b11) == 2
    }
    
    #[inline(always)]
    pub fn mapper_number(&self) -> u16 {
        if self.is_ines2() {
            (((self.header[8] & 0x0F) as u16) << 8) | ((self.header[7] & 0xF0) as u16) | ((self.header[6] >> 4) as u16)
        } else {
            ((self.header[7] & 0xF0) | (self.header[6] >> 4)) as u16
        }
    }
    
    pub fn into_mapper(self) -> Box<dyn Mapper> {
        match self.mapper_number() {
            000 => Mapper000::new(self),
            
            _ => panic!("Failed to detect ROM mapper type! Possibly unsupported.")
        }
    }
}