use std::ops::{Deref, DerefMut};
use crate::arch::mappers::{DummyMapper, Mapper, RomFile};

#[derive(Debug, Clone)]
pub struct Cartridge {
    pub mapper: Box<dyn Mapper>,
}
impl Default for Cartridge {
    fn default() -> Self {
        Self {
            mapper: Box::new(DummyMapper::default()),
        }
    }
}
impl Deref for Cartridge {
    type Target = Box<dyn Mapper>;

    fn deref(&self) -> &Self::Target {
        &self.mapper
    }
}
impl DerefMut for Cartridge {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mapper
    }
}

impl Cartridge {
    pub fn new(&mut self, rom: RomFile) {
        self.mapper = rom.into_mapper();
    }
}