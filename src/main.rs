use std::time::Instant;
use argh::FromArgs;
use crate::arch::mappers::RomFile;
use crate::arch::{CpuBusAccessible, Nes};

pub mod arch;

#[derive(FromArgs)]
/// Reach new heights
struct Args {
    /// path to NES ROM
    #[argh(positional)]
    rom: String,
}

fn main() {
    let args: Args = argh::from_env();
    
    let mut nes = Nes::new();
    nes.load_rom(RomFile::new(std::fs::read(args.rom).unwrap()));
    
    loop {
        let start = Instant::now();
        
        for _ in 0..21477272 {
            nes.tick();
        }
        
        let elapsed = start.elapsed();
        println!("time to simulate 1 second: {:.6}sec ({}us)", start.elapsed().as_secs_f64(), elapsed.as_micros());
    }
}



#[derive(Debug, Default, Copy, Clone)]
pub struct TestState {
    pub pc: u16,
    pub opcode: u8,
    pub sp: u8,
    pub status: u8,
    pub acc: u8,
    pub x: u8,
    pub y: u8,
    pub cyc: usize,
}
impl TestState {
    pub fn from_nes(mut nes: Nes) -> Self {
        Self {
            pc: nes.cpu.pc - 1,
            opcode: nes.read(nes.cpu.pc - 1),
            sp: nes.cpu.sp.0,
            status: nes.cpu.status.bits(),
            acc: nes.cpu.acc,
            x: nes.cpu.x,
            y: nes.cpu.y,
            cyc: nes.cpu.cyc,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::arch::cpu::{Cpu, StatusReg};
    use crate::arch::mappers::RomFile;
    use crate::arch::Nes;
    use crate::TestState;

    #[derive(Debug, Eq, PartialEq)]
    pub enum TestError {
        Pc(u16, u16),
        Opcode(u8, u8),
        Sp(u8, u8),
        Status(u8, u8),
        A(u8, u8),
        X(u8, u8),
        Y(u8, u8),
        Cyc(usize, usize),
    }
    
    impl TestState {
        pub fn new(line: &str) -> Self {
            let (line, ppucyc) = line.split_once("PPU:").unwrap();
            let parts: Vec<&str> = line.split_whitespace().collect();
            let last = parts.len() - 1;
            
            Self {
                pc: u16::from_str_radix(parts[0], 16).unwrap(),
                opcode: u8::from_str_radix(parts[1], 16).unwrap(),
                sp: u8::from_str_radix(parts[last].split_at(3).1, 16).unwrap(),
                status: u8::from_str_radix(parts[last - 1].split_at(2).1, 16).unwrap(),
                acc: u8::from_str_radix(parts[last - 4].split_at(2).1, 16).unwrap(),
                x: u8::from_str_radix(parts[last - 3].split_at(2).1, 16).unwrap(),
                y: u8::from_str_radix(parts[last - 2].split_at(2).1, 16).unwrap(),
                cyc: ppucyc.split_once("CYC:").unwrap().1.parse().unwrap(),
            }
        }
        
        pub fn cmp(&self, other: &Self) -> Option<TestError> {
            use TestError::*;
            if self.pc != other.pc {
                Some(Pc(self.pc, other.pc))
            } else if self.opcode != other.opcode {
                Some(Opcode(self.opcode, other.opcode))
            } else if self.sp != other.sp {
                Some(Sp(self.sp, other.sp))
            } else if self.status != other.status {
                Some(Status(self.status, other.status))
            } else if self.acc != other.acc {
                Some(A(self.acc, other.acc))
            } else if self.x != other.x {
                Some(X(self.x, other.x))
            } else if self.y != other.y {
                Some(Y(self.y, other.y))
            } else if self.cyc != other.cyc {
                Some(Cyc(self.cyc, other.cyc))
            } else {
                None
            }
        }
    }
    impl PartialEq for TestState {
        fn eq(&self, other: &Self) -> bool {
            (self.pc == other.pc) &&
                (self.opcode == other.opcode) &&
                (self.sp == other.sp) &&
                (self.status == other.status) &&
                (self.acc == other.acc) &&
                (self.x == other.x) &&
                (self.y == other.y) &&
                (self.cyc == other.cyc)
        }
    }
    
    #[test]
    fn nestest() {
        let rom = RomFile::new(include_bytes!("../testroms/nestest.nes"));
        
        let log = include_str!("../testroms/nestest.log");
        let log: Vec<TestState> = log.lines().map(|line| TestState::new(line)).collect();
        let mut log_iter = log.iter();
        
        let mut nes = Nes::new();
        
        nes.cart.mapper = rom.into_mapper();
        nes.cpu.pc = 0xC000;
        nes.cpu.prefetch = Some(Cpu::fetch(&mut nes));
        nes.cpu.cyc = 7;
        nes.ppu.pos = crate::arch::ppu::PixelPos { cycle: 19, scanline: 0 };
        nes.cpu.status = StatusReg::from_bits_truncate(0x24);
        
        loop {
            Cpu::tick(&mut nes);
            //nes.ppu.tick(&mut nes);
            
            if let Some(state) = nes.cpu.last_state {
                if let Some(log) = log_iter.next() {
                    if let Some(err) = log.cmp(&state) {
                        println!("Failed! {:X?}", err);
                        
                        return;
                    }
                    
                    nes.cpu.last_state = None;
                } else {
                    println!("nestest log complete");
                    return;
                }
            }
        }
    }
}