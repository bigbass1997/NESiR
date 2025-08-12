use std::path::PathBuf;
use std::str::FromStr;
use clap::Parser;
use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};
use tracing_subscriber::filter::LevelFilter;
use crate::arch::mappers::RomFile;
use crate::arch::Nes;

pub mod arch;

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
    
    #[arg(long, short)]
    pub palette: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();
    
    {
        let builder = tracing_subscriber::fmt().with_env_filter("info,nesir=info");
        if let Some(level) = args.verbose {
            builder.with_max_level(level).init();
        } else {
            builder.init();
        }
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
    window.set_target_fps(60);
    
    let mut pattern_window = Window::new(
        "NESiR - Pattern Table",
        256,
        128,
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
    pattern_window.set_target_fps(0);
    pattern_window.set_position(2624 + 520, 200);
    let mut pattern_fb = [0x00555555u32; 256 * 128];
    
    let mut nametable_window = Window::new(
        "NESiR - Nametable",
        64,
        32,
        WindowOptions {
            borderless: false,
            title: true,
            resize: false,
            scale: Scale::X8,
            scale_mode: ScaleMode::AspectRatioStretch,
            topmost: false,
            transparency: false,
            none: false,
        },
    ).expect("failed to initialize window");
    nametable_window.set_target_fps(0);
    nametable_window.set_position(2624 + 520, 500);
    let mut nametable_fb = [0x00555555u32; 64 * 32];
    
    let mut palette_window = Window::new(
        "NESiR - Palette Memory",
        4,
        8,
        WindowOptions {
            borderless: false,
            title: true,
            resize: false,
            scale: Scale::X16,
            scale_mode: ScaleMode::AspectRatioStretch,
            topmost: false,
            transparency: false,
            none: false,
        },
    ).expect("failed to initialize window");
    palette_window.set_target_fps(0);
    palette_window.set_position(2624 + 520, 800);
    let mut palette_fb = [0x00555555u32; 4 * 8];
    
    
    let mut nes = Nes::new();
    nes.load_rom(RomFile::new(std::fs::read(args.rom).unwrap()));
    
    if let Some(path) = args.palette {
        let mut colors: Vec<u32> = std::fs::read(path).unwrap()
            .chunks_exact(3)
            .map(|chunk| (((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32)))
            .collect();
        colors.resize(nes.ppu.pal_values.len(), 0x00000000);
        
        nes.ppu.pal_values.iter_mut()
            .enumerate()
            .for_each(|(i, val)| *val = colors[i]);
    }
    
    while window.is_open() && pattern_window.is_open() && nametable_window.is_open() && palette_window.is_open()
        && !window.is_key_down(Key::Escape) && !pattern_window.is_key_down(Key::Escape) && !nametable_window.is_key_down(Key::Escape) && !palette_window.is_key_down(Key::Escape) {
        //let start = Instant::now();
        
        //for _ in 0..21477272 {
        for _ in 0..357654 {
            nes.tick();
        }
        
        let fb = &mut nes.ppu.fb;
        
        
        
        //let elapsed = start.elapsed();
        window.update_with_buffer(fb, 256, 240).unwrap();
        
        render_pattern_table(&mut nes, &mut pattern_fb);
        pattern_window.update_with_buffer(&pattern_fb, 256, 128).unwrap();
        
        render_nametable(&mut nes, &mut nametable_fb);
        nametable_window.update_with_buffer(&nametable_fb, 64, 32).unwrap();
        
        render_palette(&mut nes, &mut palette_fb);
        palette_window.update_with_buffer(&palette_fb, 4, 8).unwrap();
        //trace!("time to simulate 1 frame: {:.6}sec ({}us)", start.elapsed().as_secs_f64(), elapsed.as_micros());
    }
}

fn render_palette(nes: &mut Nes, fb: &mut [u32; 4 * 8]) {
    for i in 0..fb.len() {
        let index = nes.ppu.palettes[i] as usize;
        fb[i] = nes.ppu.pal_values[index];
    }
}

fn render_nametable(nes: &mut Nes, fb: &mut [u32; 64 * 32]) {
    for i in 0..1024 {
        let x = i % 32;
        let y = i / 32;
        
        let color = nes.ppu.ciram[i] as u32;
        fb[(y * 64) + x] = (color << 16) | (color << 8) | (color);
    }
    
    for i in 1024..2048 {
        let x = i % 32;
        let y = i / 32;
        
        let color = nes.ppu.ciram[i] as u32;
        fb[((y - 32) * 64) + (x + 32)] = (color << 16) | (color << 8) | (color);
    }
}

fn render_pattern_table(nes: &mut Nes, fb: &mut [u32; 256 * 128]) {
    for tile_index in (0..4096u16).step_by(16) {
        let tile_index = tile_index / 16;
        
        for row in 0..8 {
            let lsb = nes.cart.read_ppu((tile_index * 16) + row);
            let msb = nes.cart.read_ppu((tile_index * 16) + row + 8);
            
            for col in 0..8usize {
                let color_index = (((msb & (0x80 >> col)) >> (7 - col)) << 1) | ((lsb & (0x80 >> col)) >> (7 - col));
                let color = (color_index as u32 * 64) + 63;
                
                let x = ((tile_index as usize % 16) * 8) + col;
                let y = ((tile_index as usize / 16) * 8) + row as usize;
                fb[(y * 256) + x] = (color << 16) | (color << 8) | (color);
            }
        }
    }
    
    for tile_index in (4096..8192u16).step_by(16) {
        let tile_index = tile_index / 16;
        
        for row in 0..8 {
            let lsb = nes.cart.read_ppu((tile_index * 16) + row);
            let msb = nes.cart.read_ppu((tile_index * 16) + row + 8);
            
            for col in 0..8usize {
                let color_index = (((msb & (0x80 >> col)) >> (7 - col)) << 1) | ((lsb & (0x80 >> col)) >> (7 - col));
                let color = (color_index as u32 * 64) + 63;
                
                let x = ((tile_index as usize % 16) * 8) + col;
                let y = ((tile_index as usize / 16) * 8) + row as usize;
                fb[((y - 128) * 256) + (x + 128)] = (color << 16) | (color << 8) | (color);
            }
        }
    }
}











#[cfg(all(test, not(feature = "sst")))]
mod tests {
    use crate::arch::cpu::Cpu;
    use crate::arch::mappers::RomFile;
    use crate::arch::Nes;
    
    
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
                status: nes.cpu.status.0,
                acc: nes.cpu.acc,
                x: nes.cpu.x,
                y: nes.cpu.y,
                cyc: nes.cpu.cyc,
            }
        }
    }

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
        nes.cpu.predecode = nes.read(nes.cpu.pc);
        nes.cpu.cyc = 7;
        nes.ppu.pos = crate::arch::ppu::PixelPos { cycle: 19, scanline: 0, ..Default::default() };
        nes.cpu.status.0 = 0x24;
        
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

#[cfg(all(test, feature = "sst"))]
mod cputests {
    use std::collections::HashMap;
    use std::error::Error;
    use std::fs::File;
    use std::num::Wrapping;
    use tracing::trace;
    use serde::{Deserialize, Deserializer, Serialize};
    use crate::arch::cpu::Cpu;
    use crate::arch::{BusActivity, Nes};
    
    fn deserialize_test_ram<'de, D: Deserializer<'de>>(deserializer: D) -> Result<HashMap<u16, u8>, D::Error> {
        let ram: Vec<(u16, u8)> = Vec::deserialize(deserializer)?;
        
        let mut map = HashMap::with_capacity(ram.len());
        map.extend(ram);
        
        Ok(map)
    }
    
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct State {
        pc: u16,
        s: u8,
        a: u8,
        x: u8,
        y: u8,
        p: u8,
        #[serde(deserialize_with = "deserialize_test_ram")]
        ram: HashMap<u16, u8>,
    }
    impl From<&Cpu> for State {
        fn from(cpu: &Cpu) -> Self {
            Self {
                pc: cpu.pc,
                s: cpu.sp.0,
                a: cpu.acc,
                x: cpu.x,
                y: cpu.y,
                p: cpu.status.0,
                ram: cpu.wram.clone(),
            }
        }
    }
    impl PartialEq<Cpu> for State {
        fn eq(&self, cpu: &Cpu) -> bool {
            if self.pc != cpu.pc { return false; }
            if self.s != cpu.sp.0 { return false; }
            if self.a != cpu.acc { return false; }
            if self.x != cpu.x { return false; }
            if self.y != cpu.y { return false; }
            if self.p != cpu.status.0 { return false; }
            
            for (addr, data) in cpu.wram.iter() {
                match self.ram.get(addr) {
                    Some(s_data) if s_data != data => { return false; },
                    None if *data != 0 => { return false; },
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
    #[cfg(feature = "sst")]
    fn sst() -> Result<(), Box<dyn Error>> {
        tracing_subscriber::fmt().with_env_filter("info,nesir=info").init();
        
        let file_iterator = walkdir::WalkDir::new("sst")
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                let opcode = u8::from_str_radix(&e.path().file_stem().unwrap().to_string_lossy(), 16).unwrap();
                //if opcode != 0xDE { return false; }
                
                ![
                    0x02, 0x12, 0x22, 0x32, 0x42, 0x52, 0x62, 0x72, 0x92, 0xB2, 0xd2, 0xF2, // jams
                    0x0B, 0x2B, 0x4B, 0x6B, 0x8B, // ANC, ALR, ARR, ANE (illegals)
                    0x93, 0x9B, 0x9C, 0x9E, 0x9F, // SHA, SHS, SHY, SHX (illegals)
                    0xAB, 0xCB, // LXA, SBX (illegals)
                ].contains(&opcode)
            })
            .map(|e| {
                let file_name = e.file_name().to_string_lossy().to_string();
                let tests: Vec<TestData> = simd_json::from_reader(File::open(e.path()).unwrap()).unwrap();
                
                (file_name, tests)
            });
        
        //let mut handles = Vec::with_capacity(256);
        for (file_name, tests) in file_iterator {
            // TODO: Change to build.rs script to generate a test for each JSON file, letting cargo parallelize this automatically
            //handles.push(std::thread::spawn(move || {
                let mut nes = Nes::new();
                for test in tests {
                    trace!("testing: {} ({})", test.name, test.cycles.len());
                    
                    nes.cpu = Cpu::default();
                    nes.cpu.pc = test.initial.pc;
                    nes.cpu.sp = Wrapping(test.initial.s);
                    nes.cpu.acc = test.initial.a;
                    nes.cpu.x = test.initial.x;
                    nes.cpu.y = test.initial.y;
                    nes.cpu.status.0 = test.initial.p;
                    nes.cpu.wram = test.initial.ram.clone();
                    assert!(test.initial == nes.cpu, " left: {:X?}\nright: {:X?}", test.initial, State::from(&nes.cpu));
                    trace!("init: {:X?}", State::from(&nes.cpu));
                    
                    fn test_cycle(cyc: usize, test: &TestData, nes: &Nes) {
                        let BusActivity { addr, data, is_read } = nes.last_bus;
                        
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
                    
                    assert!(test.final_ == nes.cpu, "{}:\n\t left: {:X?}\n\tright: {:X?}", test.name, test.final_, State::from(&nes.cpu));
                }
                tracing::debug!("{file_name} complete");
            //}));
        }
        
        //for handle in handles {
        //    handle.join().unwrap();
        //}
        
        Ok(())
    }
}