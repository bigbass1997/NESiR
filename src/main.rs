use std::str::FromStr;
use std::time::Instant;
use log::{debug, LevelFilter};
use clap::Parser;
use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};
use crate::arch::mappers::RomFile;
use crate::arch::{CpuBusAccessible, Nes};

pub mod arch;
pub mod logger;

#[derive(Clone, Copy, Debug)]
enum ScaleArg {
    FitScreen,
    X1,
    X2,
    X4,
    X8,
    X16,
    X32,
}
impl FromStr for ScaleArg {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ScaleArg::*;
        match s.to_uppercase().as_str() {
            "FITSCREEN" | "FIT" => Ok(FitScreen),
            "X1" => Ok(X1),
            "X2" => Ok(X2),
            "X4" => Ok(X4),
            "X8" => Ok(X8),
            "X16" => Ok(X16),
            "X32" => Ok(X32),
            _ => Err("Expected fitscreen|x1|x2|x4|x8|x16|x32".to_string())
        }
    }
}
impl From<ScaleArg> for Scale {
    fn from(value: ScaleArg) -> Self {
        match value {
            ScaleArg::FitScreen => Scale::FitScreen,
            ScaleArg::X1 => Scale::X1,
            ScaleArg::X2 => Scale::X2,
            ScaleArg::X4 => Scale::X4,
            ScaleArg::X8 => Scale::X8,
            ScaleArg::X16 => Scale::X16,
            ScaleArg::X32 => Scale::X32,
        }
    }
}
impl Default for ScaleArg {
    fn default() -> Self {
        Self::X2
    }
}

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Args {
    pub rom: String,
    
    #[arg(long, short)]
    pub verbose: Option<LevelFilter>,
    
    #[arg(long, short)]
    pub scale: Option<ScaleArg>,
}

fn main() {
    let args = Args::parse();
    
    // Setup program-wide logger format
    {
        let mut logbuilder = logger::builder();
        logbuilder.filter_level(args.verbose.unwrap_or(LevelFilter::Debug));
        logbuilder.init();
    }
    
    let mut window = Window::new(
        "NESiR",
        256,
        240,
        WindowOptions {
            borderless: false,
            title: true,
            resize: false,
            scale: args.scale.unwrap_or_default().into(),
            scale_mode: ScaleMode::AspectRatioStretch,
            topmost: false,
            transparency: false,
            none: false,
        },
    ).expect("failed to initialize window");
    window.limit_update_rate(None);
    
    let mut nes = Nes::new();
    nes.load_rom(RomFile::new(std::fs::read(args.rom).unwrap()));
    
    let fb = [0x00555555u32; 256 * 240];
    
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let start = Instant::now();
        
        //for _ in 0..21477272 {
        for _ in 0..357654 {
            nes.tick();
        }
        
        let elapsed = start.elapsed();
        window.update_with_buffer(&fb, 256, 240).unwrap();
        debug!("time to simulate 1 frame: {:.6}sec ({}us)", start.elapsed().as_secs_f64(), elapsed.as_micros());
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
#[cfg(not(feature = "tomharte"))]
mod tests {
    use crate::arch::cpu::{Cpu, StatusReg};
    use crate::arch::mappers::RomFile;
    use crate::arch::{CpuBusAccessible, Nes};
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
        /*{
            let mut logbuilder = logger::builder();
            logbuilder.filter_level(LevelFilter::Trace);
            logbuilder.init();
        }*/
        
        let rom = RomFile::new(include_bytes!("../testroms/nestest.nes"));
        
        let log = include_str!("../testroms/nestest.log");
        let log: Vec<TestState> = log.lines().map(|line| TestState::new(line)).collect();
        let mut log_iter = log.iter();
        
        let mut nes = Nes::new();
        
        nes.cart.mapper = rom.into_mapper();
        nes.cpu.pc = 0xC000;
        nes.cpu.predecode = nes.read(nes.cpu.pc);
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

#[cfg(test)]
#[cfg(feature = "tomharte")]
mod cputests {
    use std::error::Error;
    use std::fs::File;
    use std::num::Wrapping;
    use log::trace;
    use serde::{Deserialize, Serialize};
    use crate::arch::cpu::{Cpu, StatusReg};
    use crate::arch::Nes;
    
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct State {
        pc: u16,
        s: u8,
        a: u8,
        x: u8,
        y: u8,
        p: u8,
        ram: Vec<(u16, u8)>,
    }
    impl From<&Cpu> for State {
        fn from(cpu: &Cpu) -> Self {
            let mut ram = Vec::with_capacity(8);
            for (addr, data) in cpu.wram.into_iter().enumerate() {
                if data != 0 {
                    ram.push((addr as u16, data));
                }
            }
            
            Self {
                pc: cpu.pc,
                s: cpu.sp.0,
                a: cpu.acc,
                x: cpu.x,
                y: cpu.y,
                p: cpu.status.bits(),
                ram,
            }
        }
    }
    impl PartialEq<Cpu> for State {
        fn eq(&self, other: &Cpu) -> bool {
            if self.pc != other.pc { return false; }
            if self.s != other.sp.0 { return false; }
            if self.a != other.acc { return false; }
            if self.x != other.x { return false; }
            if self.y != other.y { return false; }
            if self.p != other.status.bits() { return false; }
            
            for (addr, data) in other.wram.into_iter().enumerate() {
                match self.ram.iter().find(|(s_addr, _)| *s_addr as usize == addr) {
                    Some((_, s_data)) if *s_data != data => { return false; },
                    None if data != 0 => { return false; },
                    _ => ()
                }
            }
            
            true
        }
    }
    
    #[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
    enum ReadWrite {
        #[serde(rename = "read")]
        Read,
        #[serde(rename = "write")]
        Write,
    }
    impl From<bool> for ReadWrite {
        fn from(value: bool) -> Self {
            if value { Self::Read } else { Self::Write }
        }
    }
    
    #[derive(Debug, Serialize, Deserialize)]
    struct TestData {
        name: String,
        initial: State,
        #[serde(rename = "final")]
        final_: State,
        cycles: Vec<(u16, u8, ReadWrite)>,
    }
    
    #[test]
    #[cfg(feature = "tomharte")]
    fn tomharte() -> Result<(), Box<dyn Error>> {
        let mut handles = Vec::with_capacity(256);
        for entry in walkdir::WalkDir::new("tomharte")
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                let opcode = u8::from_str_radix(&e.path().file_stem().unwrap().to_string_lossy(), 16).unwrap();
                //if opcode != 0x00 { return false; }
                
                ![
                    0x02, 0x12, 0x22, 0x32, 0x42, 0x52, 0x62, 0x72, 0x92, 0xB2, 0xd2, 0xF2, // jams
                    0x0B, 0x2B, 0x4B, 0x6B, 0x8B, // ANC, ALR, ARR, ANE (illegals)
                    0x93, 0x9B, 0x9C, 0x9E, 0x9F, // SHA, SHS, SHY, SHX (illegals)
                    0xAB, 0xCB, // LXA, SBX (illegals)
                ].contains(&opcode)
            }){
            
            // TODO: Change to build.rs script to generate individual tests, letting cargo parallelize this automatically
            handles.push(std::thread::spawn(|| {
                let tests: Vec<TestData> = simd_json::from_reader(File::open(entry.into_path()).unwrap()).unwrap();
                for test in tests {
                    trace!("testing: {} ({})", test.name, test.cycles.len());
                    let mut nes = Nes::new();
                    
                    nes.cpu.pc = test.initial.pc;
                    nes.cpu.sp = Wrapping(test.initial.s);
                    nes.cpu.acc = test.initial.a;
                    nes.cpu.x = test.initial.x;
                    nes.cpu.y = test.initial.y;
                    nes.cpu.status = StatusReg::from_bits_truncate(test.initial.p);
                    for (addr, data) in &test.initial.ram {
                        nes.cpu.wram[*addr as usize] = *data;
                    }
                    assert!(test.initial == nes.cpu, " left: {:X?}\nright: {:X?}", test.initial, State::from(&nes.cpu));
                    trace!("init: {:X?}", State::from(&nes.cpu));
                    
                    fn test_cycle(cyc: usize, test: &TestData, nes: &Nes) {
                        let (addr, data, is_read) = nes.last_bus;
                        
                        trace!("({addr:04X}, {data:02X}, {:?})", if is_read { "read" } else { "write" });
                        assert!(test.cycles[cyc] == (addr, data, ReadWrite::from(is_read)), " left: {:X?}\nright: {:X?}", test.cycles[cyc], (addr, data, ReadWrite::from(is_read)));
                    }
                    
                    Cpu::cycle(&mut nes);
                    test_cycle(0, &test, &nes);
                    
                    let mut cyc = 1;
                    while !nes.cpu.proc.done {
                        Cpu::cycle(&mut nes);
                        test_cycle(cyc, &test, &nes);
                        cyc += 1;
                        
                        assert!(nes.cpu.proc.cycle < 10, "cycle runaway! instruction may be stuck in a loop");
                    }
                    
                    assert!(test.final_ == nes.cpu, " left: {:X?}\nright: {:X?}", test.final_, State::from(&nes.cpu));
                }
            }));
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        Ok(())
    }
}