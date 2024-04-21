#![allow(unused_variables)]
#![allow(non_upper_case_globals)]

use std::fmt::{Debug, Formatter};
use std::num::Wrapping;
use crate::arch::{Nes, CpuBusAccessible, ClockDivider};
use bitflags::bitflags;
use log::trace;
use AddrMode::*;
use crate::TestState;


bitflags! {
    pub struct StatusReg: u8 {
        const Negative          = 0b10000000;
        const Overflow          = 0b01000000;
        const Unused            = 0b00100000;
        const Break             = 0b00010000;
        const Decimal           = 0b00001000;
        const InterruptDisable  = 0b00000100;
        const Zero              = 0b00000010;
        const Carry             = 0b00000001;
    }
}
impl Default for StatusReg {
    fn default() -> Self {
        StatusReg::Unused | StatusReg::Break | StatusReg::InterruptDisable
    }
}
impl std::fmt::Display for StatusReg {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut s = String::new();
        if self.intersects(StatusReg::Negative)           { s.push('N') } else { s.push('n') }
        if self.intersects(StatusReg::Overflow)           { s.push('V') } else { s.push('v') }
        s.push('-');
        if self.intersects(StatusReg::Break)              { s.push('B') } else { s.push('b') }
        if self.intersects(StatusReg::Decimal)            { s.push('D') } else { s.push('d') }
        if self.intersects(StatusReg::InterruptDisable)   { s.push('I') } else { s.push('i') }
        if self.intersects(StatusReg::Zero)               { s.push('Z') } else { s.push('z') }
        if self.intersects(StatusReg::Carry)              { s.push('C') } else { s.push('c') }
        
        write!(f, "{}", s)
    }
}



#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AddrMode {
    Accumulator,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    Immediate,
    Implied,
    Indirect,
    IndirectX,
    IndirectY,
    Relative,
    Zero,
    ZeroX,
    ZeroY,
    /// Mode is automatically handled by instruction (e.g. some instructions can only be used in one mode)
    Auto,
}


/// Describes the state of execution for an instruction.
/// 
/// To achive low-level cycle-stepping of CPU instructions, this struct holds
/// the relavant data of whatever instruction is currently being executed by the CPU. This can
/// also be thought of as the state of the CPU's "pipeline".
/// 
/// The temporary fields are _not_ based on any real storage within the CPU; rather they are there
/// for emulation purposes so that the emulator can exit and resume an instruction after each cycle.
#[derive(Copy, Clone)]
pub struct InstructionProcedure {
    pub done: bool,
    func: fn(&mut Nes),
    mode: AddrMode,
    pub(crate) cycle: u8,
    tmp0: u8,
    tmp1: u8,
    tmp_addr: u16,
}
impl Debug for InstructionProcedure {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstructionProcedure")
         .field("done", &self.done)
         .field("cycle", &self.cycle)
         .finish()
    }
}
impl InstructionProcedure {
    pub fn new(step_func: fn(&mut Nes), addr_mode: AddrMode) -> Self {
        Self {
            done: false,
            func: step_func,
            mode: addr_mode,
            cycle: 1,
            tmp0: 0,
            tmp1: 0,
            tmp_addr: 0
        }
    }
    
    pub fn step(nes: &mut Nes) {
        (nes.cpu.proc.func)(nes);
        nes.cpu.proc.cycle += 1;
    }
}


#[derive(Clone, Debug)]
pub struct Cpu {
    #[cfg(feature = "tomharte")]
    pub wram: [u8; 0x10000],
    #[cfg(not(feature = "tomharte"))]
    pub wram: [u8; 0x800],
    pub pc: u16,
    pub sp: Wrapping<u8>,
    pub status: StatusReg,
    pub acc: u8,
    pub x: u8,
    pub y: u8,
    /// active-low
    pub rdy: bool,
    /// active-low
    pub nmi: bool,
    /// Predecode Register (PD)
    pub(crate) predecode: u8,
    pub(crate) proc: InstructionProcedure,
    pub clock_divider: ClockDivider<12>,
    pub cyc: usize,
    pub last_state: Option<TestState>,
}
impl Default for Cpu {
    fn default() -> Self {
        let mut proc = InstructionProcedure::new(default_procedure, Implied);
        proc.done = true;
        
        Self {
            #[cfg(feature = "tomharte")]
            wram: [0u8; 0x10000],
            #[cfg(not(feature = "tomharte"))]
            wram: [0u8; 0x800],
            pc: 0,
            sp: Wrapping(0xFD), // actually this is potentialy random at power-on // software typically initializes this to 0xFF
            status: StatusReg::default(),
            acc: 0,
            x: 0,
            y: 0,
            rdy: true,
            nmi: true,
            predecode: 0,
            proc,
            clock_divider: ClockDivider::new(0), //todo: randomize
            cyc: 0,
            last_state: None,
        }
    }
}
fn default_procedure(_: &mut Nes) {}

impl Cpu {
    pub fn init_pc(nes: &mut Nes) {
        nes.cpu.pc = ((nes.cart.read_cpu(0xFFFD) as u16) << 8) | (nes.cart.read_cpu(0xFFFC) as u16);
        nes.read(nes.cpu.pc); // loads predecode register
    }
    
    #[inline(always)]
    pub fn tick(nes: &mut Nes) {
        if nes.cpu.clock_divider.tick() {
            Cpu::cycle(nes);
        }
    }
    
    pub fn cycle(nes: &mut Nes) {
        if !nes.cpu.rdy {
            return;
        }
        
        //#[cfg(feature = "tomharte")]
        //println!("PC: {:04X}, Op: {:02X}, Status: {}, ACC: {:02X}, X: {:02X}, Y: {:02X}, SP: {:02X}, PPU: {}, CYC: {}", nes.cpu.pc - 1, nes.cpu.predecode, nes.cpu.status, nes.cpu.acc, nes.cpu.x, nes.cpu.y, nes.cpu.sp, nes.ppu.pos, nes.cpu.cyc);
        
        if nes.cpu.proc.done {
            Cpu::fetch(nes);
            
            nes.cpu.proc = match nes.cpu.predecode {
                0x00 => InstructionProcedure::new(brk, Auto),
                0x01 => InstructionProcedure::new(ora, IndirectX),
                0x03 => InstructionProcedure::new(slo, IndirectX),
                0x04 => InstructionProcedure::new(nop, Zero),
                0x05 => InstructionProcedure::new(ora, Zero),
                0x06 => InstructionProcedure::new(asl, Zero),
                0x07 => InstructionProcedure::new(slo, Zero),
                0x08 => InstructionProcedure::new(php, Implied),
                0x09 => InstructionProcedure::new(ora, Immediate),
                0x0A => InstructionProcedure::new(asl, Accumulator),
                0x0B => InstructionProcedure::new(anc, Auto),
                0x0C => InstructionProcedure::new(nop, Absolute),
                0x0D => InstructionProcedure::new(ora, Absolute),
                0x0E => InstructionProcedure::new(asl, Absolute),
                0x0F => InstructionProcedure::new(slo, Absolute),
                
                0x10 => InstructionProcedure::new(bpl, Relative),
                0x11 => InstructionProcedure::new(ora, IndirectY),
                0x13 => InstructionProcedure::new(slo, IndirectY),
                0x14 => InstructionProcedure::new(nop, ZeroX),
                0x15 => InstructionProcedure::new(ora, ZeroX),
                0x16 => InstructionProcedure::new(asl, ZeroX),
                0x17 => InstructionProcedure::new(slo, ZeroX),
                0x18 => InstructionProcedure::new(clc, Implied),
                0x19 => InstructionProcedure::new(ora, AbsoluteY),
                0x1A => InstructionProcedure::new(nop, Implied),
                0x1B => InstructionProcedure::new(slo, AbsoluteY),
                0x1C => InstructionProcedure::new(nop, AbsoluteX),
                0x1D => InstructionProcedure::new(ora, AbsoluteX),
                0x1E => InstructionProcedure::new(asl, AbsoluteX),
                0x1F => InstructionProcedure::new(slo, AbsoluteX),
                
                0x20 => InstructionProcedure::new(jsr, Auto),
                0x21 => InstructionProcedure::new(and, IndirectX),
                0x23 => InstructionProcedure::new(rla, IndirectX),
                0x24 => InstructionProcedure::new(bit, Zero),
                0x25 => InstructionProcedure::new(and, Zero),
                0x26 => InstructionProcedure::new(rol, Zero),
                0x27 => InstructionProcedure::new(rla, Zero),
                0x28 => InstructionProcedure::new(plp, Implied),
                0x29 => InstructionProcedure::new(and, Immediate),
                0x2A => InstructionProcedure::new(rol, Accumulator),
                0x2B => InstructionProcedure::new(anc, Auto),
                0x2C => InstructionProcedure::new(bit, Absolute),
                0x2D => InstructionProcedure::new(and, Absolute),
                0x2E => InstructionProcedure::new(rol, Absolute),
                0x2F => InstructionProcedure::new(rla, Absolute),
                
                0x30 => InstructionProcedure::new(bmi, Relative),
                0x31 => InstructionProcedure::new(and, IndirectY),
                0x33 => InstructionProcedure::new(rla, IndirectY),
                0x34 => InstructionProcedure::new(nop, ZeroX),
                0x35 => InstructionProcedure::new(and, ZeroX),
                0x36 => InstructionProcedure::new(rol, ZeroX),
                0x37 => InstructionProcedure::new(rla, ZeroX),
                0x38 => InstructionProcedure::new(sec, Implied),
                0x39 => InstructionProcedure::new(and, AbsoluteY),
                0x3A => InstructionProcedure::new(nop, Implied),
                0x3B => InstructionProcedure::new(rla, AbsoluteY),
                0x3C => InstructionProcedure::new(nop, AbsoluteX),
                0x3D => InstructionProcedure::new(and, AbsoluteX),
                0x3E => InstructionProcedure::new(rol, AbsoluteX),
                0x3F => InstructionProcedure::new(rla, AbsoluteX),
                
                0x40 => InstructionProcedure::new(rti, Auto),
                0x41 => InstructionProcedure::new(eor, IndirectX),
                0x43 => InstructionProcedure::new(sre, IndirectX),
                0x44 => InstructionProcedure::new(nop, Zero),
                0x45 => InstructionProcedure::new(eor, Zero),
                0x46 => InstructionProcedure::new(lsr, Zero),
                0x47 => InstructionProcedure::new(sre, Zero),
                0x48 => InstructionProcedure::new(pha, Implied),
                0x49 => InstructionProcedure::new(eor, Immediate),
                0x4A => InstructionProcedure::new(lsr, Accumulator),
                0x4B => InstructionProcedure::new(asr, Auto),
                0x4C => InstructionProcedure::new(jmp, Absolute),
                0x4D => InstructionProcedure::new(eor, Absolute),
                0x4E => InstructionProcedure::new(lsr, Absolute),
                0x4F => InstructionProcedure::new(sre, Absolute),
                
                0x50 => InstructionProcedure::new(bvc, Relative),
                0x51 => InstructionProcedure::new(eor, IndirectY),
                0x53 => InstructionProcedure::new(sre, IndirectY),
                0x54 => InstructionProcedure::new(nop, ZeroX),
                0x55 => InstructionProcedure::new(eor, ZeroX),
                0x56 => InstructionProcedure::new(lsr, ZeroX),
                0x57 => InstructionProcedure::new(sre, ZeroX),
                0x58 => InstructionProcedure::new(cli, Auto),
                0x59 => InstructionProcedure::new(eor, AbsoluteY),
                0x5A => InstructionProcedure::new(nop, Implied),
                0x5B => InstructionProcedure::new(sre, AbsoluteY),
                0x5C => InstructionProcedure::new(nop, AbsoluteX),
                0x5D => InstructionProcedure::new(eor, AbsoluteX),
                0x5E => InstructionProcedure::new(lsr, AbsoluteX),
                0x5F => InstructionProcedure::new(sre, AbsoluteX),
                
                0x60 => InstructionProcedure::new(rts, Implied),
                0x61 => InstructionProcedure::new(adc, IndirectX),
                0x63 => InstructionProcedure::new(rra, IndirectX),
                0x64 => InstructionProcedure::new(nop, Zero),
                0x65 => InstructionProcedure::new(adc, Zero),
                0x66 => InstructionProcedure::new(ror, Zero),
                0x67 => InstructionProcedure::new(rra, Zero),
                0x68 => InstructionProcedure::new(pla, Implied),
                0x69 => InstructionProcedure::new(adc, Immediate),
                0x6A => InstructionProcedure::new(ror, Accumulator),
                0x6B => InstructionProcedure::new(arr, Auto),
                0x6C => InstructionProcedure::new(jmp, Indirect),
                0x6D => InstructionProcedure::new(adc, Absolute),
                0x6E => InstructionProcedure::new(ror, Absolute),
                0x6F => InstructionProcedure::new(rra, Absolute),
                
                0x70 => InstructionProcedure::new(bvs, Relative),
                0x71 => InstructionProcedure::new(adc, IndirectY),
                0x73 => InstructionProcedure::new(rra, IndirectY),
                0x74 => InstructionProcedure::new(nop, ZeroX),
                0x75 => InstructionProcedure::new(adc, ZeroX),
                0x76 => InstructionProcedure::new(ror, ZeroX),
                0x77 => InstructionProcedure::new(rra, ZeroX),
                0x78 => InstructionProcedure::new(sei, Auto),
                0x79 => InstructionProcedure::new(adc, AbsoluteY),
                0x7A => InstructionProcedure::new(nop, Implied),
                0x7B => InstructionProcedure::new(rra, AbsoluteY),
                0x7C => InstructionProcedure::new(nop, AbsoluteX),
                0x7D => InstructionProcedure::new(adc, AbsoluteX),
                0x7E => InstructionProcedure::new(ror, AbsoluteX),
                0x7F => InstructionProcedure::new(rra, AbsoluteX),
                
                0x80 => InstructionProcedure::new(nop, Immediate),
                0x81 => InstructionProcedure::new(sta, IndirectX),
                0x82 => InstructionProcedure::new(nop, Immediate),
                0x83 => InstructionProcedure::new(sax, IndirectX),
                0x84 => InstructionProcedure::new(sty, Zero),
                0x85 => InstructionProcedure::new(sta, Zero),
                0x86 => InstructionProcedure::new(stx, Zero),
                0x87 => InstructionProcedure::new(sax, Zero),
                0x88 => InstructionProcedure::new(dey, Implied),
                0x89 => InstructionProcedure::new(nop, Immediate),
                0x8A => InstructionProcedure::new(txa, Implied),
                0x8B => InstructionProcedure::new(ane, Auto),
                0x8C => InstructionProcedure::new(sty, Absolute),
                0x8D => InstructionProcedure::new(sta, Absolute),
                0x8E => InstructionProcedure::new(stx, Absolute),
                0x8F => InstructionProcedure::new(sax, Absolute),
                
                0x90 => InstructionProcedure::new(bcc, Relative),
                0x91 => InstructionProcedure::new(sta, IndirectY),
                0x93 => InstructionProcedure::new(sha, IndirectY),
                0x94 => InstructionProcedure::new(sty, ZeroX),
                0x95 => InstructionProcedure::new(sta, ZeroX),
                0x96 => InstructionProcedure::new(stx, ZeroY),
                0x97 => InstructionProcedure::new(sax, ZeroY),
                0x98 => InstructionProcedure::new(tya, Implied),
                0x99 => InstructionProcedure::new(sta, AbsoluteY),
                0x9A => InstructionProcedure::new(txs, Implied),
                0x9B => InstructionProcedure::new(shs, Auto),
                0x9C => InstructionProcedure::new(shy, Auto),
                0x9D => InstructionProcedure::new(sta, AbsoluteX),
                0x9E => InstructionProcedure::new(shx, Auto),
                0x9F => InstructionProcedure::new(sha, AbsoluteY),
                
                0xA0 => InstructionProcedure::new(ldy, Immediate),
                0xA1 => InstructionProcedure::new(lda, IndirectX),
                0xA2 => InstructionProcedure::new(ldx, Immediate),
                0xA3 => InstructionProcedure::new(lax, IndirectX),
                0xA4 => InstructionProcedure::new(ldy, Zero),
                0xA5 => InstructionProcedure::new(lda, Zero),
                0xA6 => InstructionProcedure::new(ldx, Zero),
                0xA7 => InstructionProcedure::new(lax, Zero),
                0xA8 => InstructionProcedure::new(tay, Implied),
                0xA9 => InstructionProcedure::new(lda, Immediate),
                0xAA => InstructionProcedure::new(tax, Implied),
                0xAB => InstructionProcedure::new(lxa, Auto),
                0xAC => InstructionProcedure::new(ldy, Absolute),
                0xAD => InstructionProcedure::new(lda, Absolute),
                0xAE => InstructionProcedure::new(ldx, Absolute),
                0xAF => InstructionProcedure::new(lax, Absolute),
                
                0xB0 => InstructionProcedure::new(bcs, Relative),
                0xB1 => InstructionProcedure::new(lda, IndirectY),
                0xB3 => InstructionProcedure::new(lax, IndirectY),
                0xB4 => InstructionProcedure::new(ldy, ZeroX),
                0xB5 => InstructionProcedure::new(lda, ZeroX),
                0xB6 => InstructionProcedure::new(ldx, ZeroY),
                0xB7 => InstructionProcedure::new(lax, ZeroY),
                0xB8 => InstructionProcedure::new(clv, Implied),
                0xB9 => InstructionProcedure::new(lda, AbsoluteY),
                0xBA => InstructionProcedure::new(tsx, Implied),
                0xBB => InstructionProcedure::new(las, AbsoluteY),
                0xBC => InstructionProcedure::new(ldy, AbsoluteX),
                0xBD => InstructionProcedure::new(lda, AbsoluteX),
                0xBE => InstructionProcedure::new(ldx, AbsoluteY),
                0xBF => InstructionProcedure::new(lax, AbsoluteY),
                
                0xC0 => InstructionProcedure::new(cpy, Immediate),
                0xC1 => InstructionProcedure::new(cmp, IndirectX),
                0xC2 => InstructionProcedure::new(nop, Immediate),
                0xC3 => InstructionProcedure::new(dcp, IndirectX),
                0xC4 => InstructionProcedure::new(cpy, Zero),
                0xC5 => InstructionProcedure::new(cmp, Zero),
                0xC6 => InstructionProcedure::new(dec, Zero),
                0xC7 => InstructionProcedure::new(dcp, Zero),
                0xC8 => InstructionProcedure::new(iny, Implied),
                0xC9 => InstructionProcedure::new(cmp, Immediate),
                0xCA => InstructionProcedure::new(dex, Implied),
                0xCB => InstructionProcedure::new(sbx, Auto),
                0xCC => InstructionProcedure::new(cpy, Absolute),
                0xCD => InstructionProcedure::new(cmp, Absolute),
                0xCE => InstructionProcedure::new(dec, Absolute),
                0xCF => InstructionProcedure::new(dcp, Absolute),
                
                0xD0 => InstructionProcedure::new(bne, Relative),
                0xD1 => InstructionProcedure::new(cmp, IndirectY),
                0xD3 => InstructionProcedure::new(dcp, IndirectY),
                0xD4 => InstructionProcedure::new(nop, ZeroX),
                0xD5 => InstructionProcedure::new(cmp, ZeroX),
                0xD6 => InstructionProcedure::new(dec, ZeroX),
                0xD7 => InstructionProcedure::new(dcp, ZeroX),
                0xD8 => InstructionProcedure::new(cld, Auto),
                0xD9 => InstructionProcedure::new(cmp, AbsoluteY),
                0xDA => InstructionProcedure::new(nop, Implied),
                0xDB => InstructionProcedure::new(dcp, AbsoluteY),
                0xDC => InstructionProcedure::new(nop, AbsoluteX),
                0xDD => InstructionProcedure::new(cmp, AbsoluteX),
                0xDE => InstructionProcedure::new(dec, AbsoluteX),
                0xDF => InstructionProcedure::new(dcp, AbsoluteX),
                
                0xE0 => InstructionProcedure::new(cpx, Immediate),
                0xE1 => InstructionProcedure::new(sbc, IndirectX),
                0xE2 => InstructionProcedure::new(nop, Immediate),
                0xE3 => InstructionProcedure::new(isb, IndirectX),
                0xE4 => InstructionProcedure::new(cpx, Zero),
                0xE5 => InstructionProcedure::new(sbc, Zero),
                0xE6 => InstructionProcedure::new(inc, Zero),
                0xE7 => InstructionProcedure::new(isb, Zero),
                0xE8 => InstructionProcedure::new(inx, Implied),
                0xE9 => InstructionProcedure::new(sbc, Immediate),
                0xEA => InstructionProcedure::new(nop, Implied),
                0xEB => InstructionProcedure::new(sbc, Immediate),
                0xEC => InstructionProcedure::new(cpx, Absolute),
                0xED => InstructionProcedure::new(sbc, Absolute),
                0xEE => InstructionProcedure::new(inc, Absolute),
                0xEF => InstructionProcedure::new(isb, Absolute),
                
                0xF0 => InstructionProcedure::new(beq, Relative),
                0xF1 => InstructionProcedure::new(sbc, IndirectY),
                0xF3 => InstructionProcedure::new(isb, IndirectY),
                0xF4 => InstructionProcedure::new(nop, ZeroX),
                0xF5 => InstructionProcedure::new(sbc, ZeroX),
                0xF6 => InstructionProcedure::new(inc, ZeroX),
                0xF7 => InstructionProcedure::new(isb, ZeroX),
                0xF8 => InstructionProcedure::new(sed, Auto),
                0xF9 => InstructionProcedure::new(sbc, AbsoluteY),
                0xFA => InstructionProcedure::new(nop, Implied),
                0xFB => InstructionProcedure::new(isb, AbsoluteY),
                0xFC => InstructionProcedure::new(nop, AbsoluteX),
                0xFD => InstructionProcedure::new(sbc, AbsoluteX),
                0xFE => InstructionProcedure::new(inc, AbsoluteX),
                0xFF => InstructionProcedure::new(isb, AbsoluteX),
                
                _ => panic!("Attempt to run invalid/unimplemented opcode! PC: {:#06X}, Op: {:#06X}", nes.cpu.pc, nes.cpu.predecode)
            };
            
            #[cfg(test)]
            {
                nes.cpu.last_state = Some(TestState::from_nes(nes.clone()));
                trace!("         PC: {:04X}, Op: {:02X}, Status: {}, ACC: {:02X}, X: {:02X}, Y: {:02X}, SP: {:02X}, PPU: {}, CYC: {}", nes.cpu.pc - 1, nes.cpu.predecode, nes.cpu.status, nes.cpu.acc, nes.cpu.x, nes.cpu.y, nes.cpu.sp, nes.ppu.pos, nes.cpu.cyc);
            }
            
            nes.cpu.proc.cycle = 2; // the decode above costs 1 cycle
        } else {
            InstructionProcedure::step(nes);
        }
        
        nes.cpu.cyc += 1;
    }
    
    fn fetch(nes: &mut Nes) -> u8 {
        let fetch = nes.read(nes.cpu.pc);
        nes.cpu.pc += 1;
        
        fetch
    }
    
    fn stack_push(nes: &mut Nes, data: u8) {
        nes.write(0x100 + nes.cpu.sp.0 as u16, data);
        nes.cpu.sp -= Wrapping(1);
    }
    
    fn stack_pull(nes: &mut Nes) -> u8 {
        nes.cpu.sp += Wrapping(1);
        nes.read(0x100 + nes.cpu.sp.0 as u16)
    }
}
#[cfg(not(feature = "tomharte"))]
impl CpuBusAccessible for Cpu {
    fn write(&mut self, addr: u16, data: u8) {
        match addr {
            0x0000..=0x1FFF => self.wram[(addr & 0x07FF) as usize] = data,
            _ => panic!("Write attempt to invalid address {:#06X} ({:#04X})", addr, data),
        }
    }

    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.wram[(addr & 0x07FF) as usize],
            _ => panic!("Read attempt to invalid address {:#06X}", addr),
        }
    }
}

#[cfg(feature = "tomharte")]
impl CpuBusAccessible for Cpu {
    fn write(&mut self, addr: u16, data: u8) {
        self.wram[addr as usize] = data;
    }

    fn read(&mut self, addr: u16) -> u8 {
        self.wram[addr as usize]
    }
}



fn adc(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        let data = nes.read(addr);
        
        let result = (nes.cpu.acc as u16).wrapping_add(data as u16).wrapping_add(nes.cpu.status.contains(StatusReg::Carry) as u16);
        
        nes.cpu.status.set(StatusReg::Carry, result & 0x100 != 0);
        nes.cpu.status.set(StatusReg::Overflow, (!(nes.cpu.acc ^ data) & (nes.cpu.acc ^ result as u8) & 0x80) != 0);
        nes.cpu.status.set(StatusReg::Zero, (result as u8) == 0);
        nes.cpu.status.set(StatusReg::Negative, result & 0x80 > 0);
        nes.cpu.acc = result as u8;
        
        nes.cpu.proc.done = true;
    }
}
fn anc(nes: &mut Nes) { unimplemented!() }
fn and(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        nes.cpu.acc &= nes.read(addr);
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn ane(nes: &mut Nes) { unimplemented!() }
fn arr(nes: &mut Nes) { unimplemented!() }
fn asl(nes: &mut Nes) {
    match nes.cpu.proc.mode {
        Accumulator => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.status.set(StatusReg::Carry, nes.cpu.acc & 0x80 != 0);
                    nes.cpu.acc <<= 1;
                    
                    nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
                    nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
                    
                    nes.read(nes.cpu.pc);
                    nes.cpu.proc.done = true;
                },
                _ => ()
            }
        },
        _ => {
            if let Some(addr) = read_modify_write(nes) {
                nes.cpu.status.set(StatusReg::Carry, nes.cpu.proc.tmp0 & 0x80 != 0);
                nes.cpu.proc.tmp0 <<= 1;
                
                nes.cpu.status.set(StatusReg::Zero, nes.cpu.proc.tmp0 == 0);
                nes.cpu.status.set(StatusReg::Negative, nes.cpu.proc.tmp0 & 0x80 > 0);
                nes.write(addr, nes.cpu.proc.tmp0);
                
                nes.cpu.proc.done = true;
            }
        }
    }
}
fn asr(nes: &mut Nes) { unimplemented!() }
fn bcc(nes: &mut Nes) {
    branch(nes, !nes.cpu.status.contains(StatusReg::Carry));
}
fn bcs(nes: &mut Nes) {
    branch(nes, nes.cpu.status.contains(StatusReg::Carry));
}
fn beq(nes: &mut Nes) {
    branch(nes, nes.cpu.status.contains(StatusReg::Zero));
}
fn bit(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        let tmp = nes.read(addr);
        
        nes.cpu.status.set(StatusReg::Zero, tmp & nes.cpu.acc == 0);
        nes.cpu.status.set(StatusReg::Overflow, tmp & 0x40 > 0);
        nes.cpu.status.set(StatusReg::Negative, tmp & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn bmi(nes: &mut Nes) {
    branch(nes, nes.cpu.status.contains(StatusReg::Negative));
}
fn bne(nes: &mut Nes) {
    branch(nes, !nes.cpu.status.contains(StatusReg::Zero));
}
fn bpl(nes: &mut Nes) {
    branch(nes, !nes.cpu.status.contains(StatusReg::Negative));
}
fn brk(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => { Cpu::fetch(nes); }, // TODO: do NOT increment on hardware interrupts
        3 => Cpu::stack_push(nes, (nes.cpu.pc >> 8) as u8),
        4 => Cpu::stack_push(nes, nes.cpu.pc as u8),
        5 => Cpu::stack_push(nes, nes.cpu.status.bits | 0b00010000),
        6 => nes.cpu.proc.tmp0 = nes.read(0xFFFE),
        7 => {
            nes.cpu.proc.tmp1 = nes.read(0xFFFF);
            
            nes.cpu.pc = ((nes.cpu.proc.tmp1 as u16) << 8) | (nes.cpu.proc.tmp0 as u16);
            nes.cpu.status.set(StatusReg::InterruptDisable, true); // masswerk has conflicting statements, and 6502_cpu.txt says the I flag should be set here
            
            nes.cpu.proc.done = true;
        }
        _ => ()
    }
}
fn bvc(nes: &mut Nes) {
    branch(nes, !nes.cpu.status.contains(StatusReg::Overflow));
}
fn bvs(nes: &mut Nes) {
    branch(nes, nes.cpu.status.contains(StatusReg::Overflow));
}

fn branch(nes: &mut Nes, to_branch: bool) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.proc.tmp0 = Cpu::fetch(nes);
            if !to_branch { // if to_branch is false, do not branch
                nes.cpu.proc.done = true;
            }
        },
        3 => {
            nes.read(nes.cpu.pc);
            nes.cpu.proc.tmp_addr = (nes.cpu.pc as i16 + nes.cpu.proc.tmp0 as i8 as i16) as u16;
            if (nes.cpu.pc & 0xFF00) == (nes.cpu.proc.tmp_addr & 0xFF00) { // branch to same page
                nes.cpu.pc = nes.cpu.proc.tmp_addr;
                
                nes.cpu.proc.done = true;
            }
        },
        4 => {
            nes.read((nes.cpu.pc & 0xFF00) + (nes.cpu.proc.tmp_addr & 0x00FF)); // read from PC + 2 + offset (without carry)
            
            nes.cpu.pc = nes.cpu.proc.tmp_addr;
            
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}

fn clc(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.status.set(StatusReg::Carry, false);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn cld(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.status.set(StatusReg::Decimal, false);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn cli(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.status.set(StatusReg::InterruptDisable, false);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn clv(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.status.set(StatusReg::Overflow, false);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn cmp(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        let data = nes.read(addr);
        
        nes.cpu.status.set(StatusReg::Carry, nes.cpu.acc >= data);
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == data);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc.wrapping_sub(data) & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn cpx(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        let data = nes.read(addr);
        
        nes.cpu.status.set(StatusReg::Carry, nes.cpu.x >= data);
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.x == data);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.x.wrapping_sub(data) & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn cpy(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        let data = nes.read(addr);
        
        nes.cpu.status.set(StatusReg::Carry, nes.cpu.y >= data);
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.y == data);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.y.wrapping_sub(data) & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn dcp(nes: &mut Nes) {
    if let Some(addr) = read_modify_write(nes) {
        nes.cpu.proc.tmp0 = nes.cpu.proc.tmp0.wrapping_sub(1);
        
        nes.cpu.status.set(StatusReg::Carry, nes.cpu.acc >= nes.cpu.proc.tmp0);
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == nes.cpu.proc.tmp0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc.wrapping_sub(nes.cpu.proc.tmp0) & 0x80 > 0);
        nes.write(addr, nes.cpu.proc.tmp0);
        
        nes.cpu.proc.done = true;
    }
}
fn dec(nes: &mut Nes) {
    if let Some(addr) = read_modify_write(nes) {
        nes.cpu.proc.tmp0 = nes.cpu.proc.tmp0.wrapping_sub(1);
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.proc.tmp0 == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.proc.tmp0 & 0x80 > 0);
        nes.write(addr, nes.cpu.proc.tmp0);
        
        nes.cpu.proc.done = true;
    }
}
fn dex(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.x = nes.cpu.x.wrapping_sub(1);
            nes.cpu.status.set(StatusReg::Zero, nes.cpu.x == 0);
            nes.cpu.status.set(StatusReg::Negative, nes.cpu.x & 0x80 > 0);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn dey(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.y = nes.cpu.y.wrapping_sub(1);
            nes.cpu.status.set(StatusReg::Zero, nes.cpu.y == 0);
            nes.cpu.status.set(StatusReg::Negative, nes.cpu.y & 0x80 > 0);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn eor(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        nes.cpu.acc ^= nes.read(addr);
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn inc(nes: &mut Nes) {
    if let Some(addr) = read_modify_write(nes) {
        nes.cpu.proc.tmp0 = nes.cpu.proc.tmp0.wrapping_add(1);
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.proc.tmp0 == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.proc.tmp0 & 0x80 > 0);
        nes.write(addr, nes.cpu.proc.tmp0);
        
        nes.cpu.proc.done = true;
    }
}
fn inx(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.x = nes.cpu.x.wrapping_add(1);
            nes.cpu.status.set(StatusReg::Zero, nes.cpu.x == 0);
            nes.cpu.status.set(StatusReg::Negative, nes.cpu.x & 0x80 > 0);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn iny(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.y = nes.cpu.y.wrapping_add(1);
            nes.cpu.status.set(StatusReg::Zero, nes.cpu.y == 0);
            nes.cpu.status.set(StatusReg::Negative, nes.cpu.y & 0x80 > 0);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn isb(nes: &mut Nes) {
    if let Some(addr) = read_modify_write(nes) {
        nes.cpu.proc.tmp0 = nes.cpu.proc.tmp0.wrapping_add(1);
        
        let data = !nes.cpu.proc.tmp0; //TODO: Check if we should use tmp0 POST-increment or PRE-increment
        
        let result = (nes.cpu.acc as u16).wrapping_add(data as u16).wrapping_add(nes.cpu.status.contains(StatusReg::Carry) as u16);
        
        nes.cpu.status.set(StatusReg::Carry, result & 0x100 != 0);
        nes.cpu.status.set(StatusReg::Overflow, (!(nes.cpu.acc ^ data) & (nes.cpu.acc ^ result as u8) & 0x80) != 0);
        nes.cpu.status.set(StatusReg::Zero, (result as u8) == 0);
        nes.cpu.status.set(StatusReg::Negative, result & 0x80 > 0);
        nes.cpu.acc = result as u8;
        
        nes.write(addr, nes.cpu.proc.tmp0);
        
        nes.cpu.proc.done = true;
    }
}
fn jmp(nes: &mut Nes) {
    match nes.cpu.proc.mode {
        Absolute => {
            match nes.cpu.proc.cycle {
                2 => nes.cpu.proc.tmp0 = Cpu::fetch(nes),
                3 => {
                    let pch = nes.read(nes.cpu.pc) as u16;
                    nes.cpu.pc = (pch << 8) | (nes.cpu.proc.tmp0 as u16);
                    
                    nes.cpu.proc.done = true;
                },
                _ => ()
            }
        },
        Indirect => {
            match nes.cpu.proc.cycle {
                2 => nes.cpu.proc.tmp0 = Cpu::fetch(nes),
                3 => nes.cpu.proc.tmp1 = Cpu::fetch(nes),
                4 => {
                    nes.cpu.proc.tmp_addr = ((nes.cpu.proc.tmp1 as u16) << 8) | (nes.cpu.proc.tmp0 as u16);
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                },
                5 => {
                    nes.cpu.proc.tmp1 = nes.read(((nes.cpu.proc.tmp_addr + 1) & 0x00FF) + (nes.cpu.proc.tmp_addr & 0xFF00));
                    
                    nes.cpu.pc = ((nes.cpu.proc.tmp1 as u16) << 8) | (nes.cpu.proc.tmp0 as u16);
                    
                    nes.cpu.proc.done = true;
                }
                _ => ()
            }
        },
        _ => panic!("Invalid mode!")
    }
}
fn jsr(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => nes.cpu.proc.tmp0 = Cpu::fetch(nes),
        3 => {nes.read(0x100 + nes.cpu.sp.0 as u16);}, // discarded read, may be useful later for monitoring bus activity
        4 => Cpu::stack_push(nes, (nes.cpu.pc >> 8) as u8),
        5 => Cpu::stack_push(nes, (nes.cpu.pc & 0xFF) as u8),
        6 => {
            nes.cpu.proc.tmp1 = Cpu::fetch(nes);
            
            nes.cpu.pc = ((nes.cpu.proc.tmp1 as u16) << 8) | (nes.cpu.proc.tmp0 as u16);
            
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn las(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        let tmp = nes.read(addr) & nes.cpu.sp.0;
        nes.cpu.acc = tmp;
        nes.cpu.x = tmp;
        nes.cpu.sp.0 = tmp;
        
        nes.cpu.status.set(StatusReg::Zero, tmp == 0);
        nes.cpu.status.set(StatusReg::Negative, tmp & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn lax(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        let tmp = nes.read(addr);
        nes.cpu.acc = tmp;
        nes.cpu.x = tmp;
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.x == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.x & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn lda(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        nes.cpu.acc = nes.read(addr);
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn ldx(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        nes.cpu.x = nes.read(addr);
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.x == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.x & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn ldy(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        nes.cpu.y = nes.read(addr);
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.y == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.y & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn lsr(nes: &mut Nes) {
    match nes.cpu.proc.mode {
        Accumulator => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.status.set(StatusReg::Carry, nes.cpu.acc & 0x01 != 0);
                    nes.cpu.acc >>= 1;
                    
                    nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
                    nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
                    
                    nes.read(nes.cpu.pc);
                    nes.cpu.proc.done = true;
                },
                _ => ()
            }
        },
        _ => {
            if let Some(addr) = read_modify_write(nes) {
                nes.cpu.status.set(StatusReg::Carry, nes.cpu.proc.tmp0 & 0x01 != 0);
                nes.cpu.proc.tmp0 >>= 1;
                
                nes.cpu.status.set(StatusReg::Zero, nes.cpu.proc.tmp0 == 0);
                nes.cpu.status.set(StatusReg::Negative, nes.cpu.proc.tmp0 & 0x80 > 0);
                nes.write(addr, nes.cpu.proc.tmp0);
                
                nes.cpu.proc.done = true;
            }
        }
    }
}
fn lxa(nes: &mut Nes) { unimplemented!() }
fn nop(nes: &mut Nes) {
    if nes.cpu.proc.mode == Implied {
        if nes.cpu.proc.cycle == 2 {
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        }
    } else if let Some(addr) = effective_addr(nes) {
        nes.read(addr);
        nes.cpu.proc.done = true;
    }
}
fn ora(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        nes.cpu.acc |= nes.read(addr);
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
        
        nes.cpu.proc.done = true;
    }
}
fn pha(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => { nes.read(nes.cpu.pc); },
        3 => {
            Cpu::stack_push(nes, nes.cpu.acc);
            
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn php(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => { nes.read(nes.cpu.pc); },
        3 => {
            Cpu::stack_push(nes, nes.cpu.status.bits | 0b00110000); //TODO: Verify bits [5:4] are supposed to be set by PHP
            
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn pla(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => { nes.read(nes.cpu.pc); },
        3 => { nes.read(nes.cpu.sp.0 as u16 + 0x100u16); }
        4 => {
            nes.cpu.acc = Cpu::stack_pull(nes);
            
            nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
            nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
            
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn plp(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => { nes.read(nes.cpu.pc); },
        3 => { nes.read(nes.cpu.sp.0 as u16 + 0x100u16); }
        4 => {
            nes.cpu.status.bits = Cpu::stack_pull(nes) & 0b11001111; //TODO: Verify bits [5:4] are supposed to be ignored by PLP
            nes.cpu.status.bits |= 0b00100000; // Apparently, bit 5 should always be set
            
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn rla(nes: &mut Nes) {
    if let Some(addr) = read_modify_write(nes) {
        let c = nes.cpu.status.contains(StatusReg::Carry) as u8;
        nes.cpu.status.set(StatusReg::Carry, nes.cpu.proc.tmp0 & 0x80 != 0);
        nes.cpu.proc.tmp0 = ((nes.cpu.proc.tmp0 << 1) & 0xFE) | c;
        
        nes.cpu.acc &= nes.cpu.proc.tmp0;
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
        nes.write(addr, nes.cpu.proc.tmp0);
        
        nes.cpu.proc.done = true;
    }
}
fn rra(nes: &mut Nes) {
    if let Some(addr) = read_modify_write(nes) {
        let c = nes.cpu.status.contains(StatusReg::Carry) as u8;
        nes.cpu.status.set(StatusReg::Carry, nes.cpu.proc.tmp0 & 0x01 != 0);
        nes.cpu.proc.tmp0 = (c << 7) | ((nes.cpu.proc.tmp0 >> 1) & 0x7F);
        let data = nes.cpu.proc.tmp0;
        
        let result = (nes.cpu.acc as u16).wrapping_add(data as u16).wrapping_add(nes.cpu.status.contains(StatusReg::Carry) as u16);
        
        nes.cpu.status.set(StatusReg::Carry, result & 0x100 != 0);
        nes.cpu.status.set(StatusReg::Overflow, (!(nes.cpu.acc ^ data) & (nes.cpu.acc ^ result as u8) & 0x80) != 0);
        nes.cpu.status.set(StatusReg::Zero, (result as u8) == 0);
        nes.cpu.status.set(StatusReg::Negative, result & 0x80 > 0);
        nes.cpu.acc = result as u8;
        nes.write(addr, nes.cpu.proc.tmp0);
        
        nes.cpu.proc.done = true;
    }
}
fn rol(nes: &mut Nes) {
    match nes.cpu.proc.mode {
        Accumulator => {
            match nes.cpu.proc.cycle {
                2 => {
                    let c = nes.cpu.status.contains(StatusReg::Carry) as u8;
                    nes.cpu.status.set(StatusReg::Carry, nes.cpu.acc & 0x80 != 0);
                    nes.cpu.acc = ((nes.cpu.acc << 1) & 0xFE) | c;
                    
                    nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
                    nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
                    
                    nes.read(nes.cpu.pc);
                    nes.cpu.proc.done = true;
                },
                _ => ()
            }
        },
        _ => {
            if let Some(addr) = read_modify_write(nes) {
                let c = nes.cpu.status.contains(StatusReg::Carry) as u8;
                nes.cpu.status.set(StatusReg::Carry, nes.cpu.proc.tmp0 & 0x80 != 0);
                nes.cpu.proc.tmp0 = ((nes.cpu.proc.tmp0 << 1) & 0xFE) | c;
                
                nes.cpu.status.set(StatusReg::Zero, nes.cpu.proc.tmp0 == 0);
                nes.cpu.status.set(StatusReg::Negative, nes.cpu.proc.tmp0 & 0x80 > 0);
                nes.write(addr, nes.cpu.proc.tmp0);
                
                nes.cpu.proc.done = true;
            }
        }
    }
}
fn ror(nes: &mut Nes) {
    match nes.cpu.proc.mode {
        Accumulator => {
            match nes.cpu.proc.cycle {
                2 => {
                    let c = nes.cpu.status.contains(StatusReg::Carry) as u8;
                    nes.cpu.status.set(StatusReg::Carry, nes.cpu.acc & 0x01 != 0);
                    nes.cpu.acc = (c << 7) | ((nes.cpu.acc >> 1) & 0x7F);
                    
                    nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
                    nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
                    
                    nes.read(nes.cpu.pc);
                    nes.cpu.proc.done = true;
                },
                _ => ()
            }
        },
        _ => {
            if let Some(addr) = read_modify_write(nes) {
                let c = nes.cpu.status.contains(StatusReg::Carry) as u8;
                nes.cpu.status.set(StatusReg::Carry, nes.cpu.proc.tmp0 & 0x01 != 0);
                nes.cpu.proc.tmp0 = (c << 7) | ((nes.cpu.proc.tmp0 >> 1) & 0x7F);
                
                nes.cpu.status.set(StatusReg::Zero, nes.cpu.proc.tmp0 == 0);
                nes.cpu.status.set(StatusReg::Negative, nes.cpu.proc.tmp0 & 0x80 > 0);
                nes.write(addr, nes.cpu.proc.tmp0);
                
                nes.cpu.proc.done = true;
            }
        }
    }
}
fn rti(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => { Cpu::fetch(nes); },
        3 => { nes.read(0x100 + nes.cpu.sp.0 as u16); }
        4 => {
            nes.cpu.status.bits = Cpu::stack_pull(nes) & 0b11001111; //TODO: Verify bits [5:4] are supposed to be ignored by PLP
            nes.cpu.status.bits |= 0b00100000; // Apparently, bit 5 should always be set
        },
        5 => nes.cpu.proc.tmp0 = Cpu::stack_pull(nes),
        6 => {
            nes.cpu.proc.tmp1 = Cpu::stack_pull(nes);
            
            nes.cpu.pc = addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0);
            
            nes.cpu.proc.done = true;
        }
        _ => (),
    }
}
fn rts(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => { Cpu::fetch(nes); },
        3 => { nes.read(0x100 + nes.cpu.sp.0 as u16); }
        4 => nes.cpu.proc.tmp0 = Cpu::stack_pull(nes),
        5 => nes.cpu.proc.tmp1 = Cpu::stack_pull(nes),
        6 => {
            nes.read(addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0));
            nes.cpu.pc = addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0) + 1;
            
            nes.cpu.proc.done = true;
        }
        _ => (),
    }
}
fn sax(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        nes.write(addr, nes.cpu.acc & nes.cpu.x);
        
        nes.cpu.proc.done = true;
    }
}
fn sbc(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        let data = !nes.read(addr);
        
        let result = (nes.cpu.acc as u16).wrapping_add(data as u16).wrapping_add(nes.cpu.status.contains(StatusReg::Carry) as u16);
        
        nes.cpu.status.set(StatusReg::Carry, result & 0x100 != 0);
        nes.cpu.status.set(StatusReg::Overflow, (!(nes.cpu.acc ^ data) & (nes.cpu.acc ^ result as u8) & 0x80) != 0);
        nes.cpu.status.set(StatusReg::Zero, (result as u8) == 0);
        nes.cpu.status.set(StatusReg::Negative, result & 0x80 > 0);
        nes.cpu.acc = result as u8;
        
        nes.cpu.proc.done = true;
    }
}
fn sbx(nes: &mut Nes) { unimplemented!() }
fn sec(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.status.set(StatusReg::Carry, true);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn sed(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.status.set(StatusReg::Decimal, true);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn sei(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.status.set(StatusReg::InterruptDisable, true);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn sha(nes: &mut Nes) { unimplemented!() } // Reminder: consume extra cycle write-instruction using AbsoluteX, AbsoluteY, or IndirectY
fn shs(nes: &mut Nes) { unimplemented!() }
fn shx(nes: &mut Nes) { unimplemented!() } // Reminder: consume extra cycle write-instruction using AbsoluteX or AbsoluteY
fn shy(nes: &mut Nes) { unimplemented!() } // Reminder: consume extra cycle write-instruction using AbsoluteX or AbsoluteY
fn slo(nes: &mut Nes) {
    if let Some(addr) = read_modify_write(nes) {
        nes.cpu.status.set(StatusReg::Carry, nes.cpu.proc.tmp0 & 0x80 != 0);
        nes.cpu.proc.tmp0 <<= 1;
        
        nes.cpu.acc |= nes.cpu.proc.tmp0;
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
        nes.write(addr, nes.cpu.proc.tmp0);
        
        nes.cpu.proc.done = true;
    }
}
fn sre(nes: &mut Nes) {
    if let Some(addr) = read_modify_write(nes) {
        nes.cpu.status.set(StatusReg::Carry, nes.cpu.proc.tmp0 & 0x01 != 0);
        nes.cpu.proc.tmp0 >>= 1;
        
        nes.cpu.acc ^= nes.cpu.proc.tmp0;
        
        nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
        nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
        nes.write(addr, nes.cpu.proc.tmp0);
        
        nes.cpu.proc.done = true;
    }
}
fn sta(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        if ((nes.cpu.proc.mode == AbsoluteX || nes.cpu.proc.mode == AbsoluteY) && nes.cpu.proc.cycle == 4) || (nes.cpu.proc.mode == IndirectY && nes.cpu.proc.cycle == 5) {
            nes.read(addr);
            return; // consume extra cycle write-instruction using AbsoluteX, AbsoluteY, or IndirectY
        }
        nes.write(addr, nes.cpu.acc);
        
        nes.cpu.proc.done = true;
    }
}
fn stx(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        if (nes.cpu.proc.mode == AbsoluteX || nes.cpu.proc.mode == AbsoluteY) && nes.cpu.proc.cycle == 4 {
            return; // consume extra cycle write-instruction using AbsoluteX or AbsoluteY
        }
        nes.write(addr, nes.cpu.x);
        
        nes.cpu.proc.done = true;
    }
}
fn sty(nes: &mut Nes) {
    if let Some(addr) = effective_addr(nes) {
        if (nes.cpu.proc.mode == AbsoluteX || nes.cpu.proc.mode == AbsoluteY) && nes.cpu.proc.cycle == 4 {
            return; // consume extra cycle write-instruction using AbsoluteX or AbsoluteY
        }
        nes.write(addr, nes.cpu.y);
        
        nes.cpu.proc.done = true;
    }
}
fn tax(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.x = nes.cpu.acc;
            
            nes.cpu.status.set(StatusReg::Zero, nes.cpu.x == 0);
            nes.cpu.status.set(StatusReg::Negative, nes.cpu.x & 0x80 > 0);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn tay(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.y = nes.cpu.acc;
            
            nes.cpu.status.set(StatusReg::Zero, nes.cpu.y == 0);
            nes.cpu.status.set(StatusReg::Negative, nes.cpu.y & 0x80 > 0);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn tsx(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.x = nes.cpu.sp.0;
            
            nes.cpu.status.set(StatusReg::Zero, nes.cpu.x == 0);
            nes.cpu.status.set(StatusReg::Negative, nes.cpu.x & 0x80 > 0);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn txa(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.acc = nes.cpu.x;
            
            nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
            nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn txs(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.sp.0 = nes.cpu.x;
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}
fn tya(nes: &mut Nes) {
    match nes.cpu.proc.cycle {
        2 => {
            nes.cpu.acc = nes.cpu.y;
            
            nes.cpu.status.set(StatusReg::Zero, nes.cpu.acc == 0);
            nes.cpu.status.set(StatusReg::Negative, nes.cpu.acc & 0x80 > 0);
            
            nes.read(nes.cpu.pc);
            nes.cpu.proc.done = true;
        },
        _ => ()
    }
}

fn effective_addr(nes: &mut Nes) -> Option<u16> {
    match nes.cpu.proc.mode {
        Immediate => {
            match nes.cpu.proc.cycle {
                2 => {
                    let pc = nes.cpu.pc;
                    nes.cpu.pc += 1;
                    
                    Some(pc)
                },
                _ => None
            }
        },
        Zero => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp0 = Cpu::fetch(nes);
                    None
                },
                3 => {
                    Some(addr_concat(0x00, nes.cpu.proc.tmp0))
                },
                _ => None
            }
        },
        Absolute => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp0 = Cpu::fetch(nes);
                    None
                },
                3 => {
                    nes.cpu.proc.tmp1 = Cpu::fetch(nes);
                    None
                },
                4 => {
                    Some(addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0))
                },
                _ => None
            }
        },
        IndirectX => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp0 = Cpu::fetch(nes);
                    None
                },
                3 => {
                    nes.read(addr_concat(0x00, nes.cpu.proc.tmp0));
                    None
                },
                4 => {
                    nes.cpu.proc.tmp_addr = addr_concat(0x00, nes.cpu.proc.tmp0.wrapping_add(nes.cpu.x));
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                5 => {
                    nes.cpu.proc.tmp1 = nes.read((nes.cpu.proc.tmp_addr + 1) & 0x00FF);
                    None
                }
                6 => Some(addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0)),
                _ => None,
            }
        },
        AbsoluteX | AbsoluteY => { // All write instructions should make sure they use 5 cycles for this mode
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp0 = Cpu::fetch(nes);
                    None
                },
                3 => {
                    nes.cpu.proc.tmp1 = Cpu::fetch(nes);
                    None
                },
                4 => {
                    let index = if nes.cpu.proc.mode == AbsoluteX {
                        nes.cpu.x
                    } else {
                        nes.cpu.y
                    };
                    
                    let (result, carry) = nes.cpu.proc.tmp0.overflowing_add(index);
                    nes.cpu.proc.tmp_addr = addr_concat(nes.cpu.proc.tmp1, result);
                    nes.cpu.proc.tmp1 = carry as u8;
                    
                    if !carry {
                        Some(nes.cpu.proc.tmp_addr)
                    } else {
                        nes.read(nes.cpu.proc.tmp_addr);
                        None
                    }
                },
                5 => Some(nes.cpu.proc.tmp_addr + ((nes.cpu.proc.tmp1 as u16) << 8)),
                _ => None
            }
        },
        ZeroX | ZeroY => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp0 = Cpu::fetch(nes);
                    None
                },
                3 => {
                    nes.read(addr_concat(0x00, nes.cpu.proc.tmp0));
                    None
                }
                4 => {
                    if nes.cpu.proc.mode == ZeroX {
                        Some(((nes.cpu.proc.tmp0 as u16) + (nes.cpu.x as u16)) & 0x00FF) // TODO: Change to `(nes.cpu.proc.tmp0 + nes.cpu.x) as u16`
                    } else {
                        Some(((nes.cpu.proc.tmp0 as u16) + (nes.cpu.y as u16)) & 0x00FF) // TODO: Change to `(nes.cpu.proc.tmp0 + nes.cpu.y) as u16`
                    }
                }
                _ => None
            }
        },
        IndirectY => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp_addr = addr_concat(0x00, Cpu::fetch(nes));
                    None
                },
                3 => {
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                4 => {
                    nes.cpu.proc.tmp1 = nes.read((nes.cpu.proc.tmp_addr + 1) & 0x00FF);
                    None
                },
                5 => {
                    let (result, carry) = nes.cpu.proc.tmp0.overflowing_add(nes.cpu.y);
                    nes.cpu.proc.tmp_addr = addr_concat(nes.cpu.proc.tmp1, result);
                    nes.cpu.proc.tmp1 = carry as u8;
                    
                    if !carry {
                        Some(nes.cpu.proc.tmp_addr)
                    } else {
                        nes.read(nes.cpu.proc.tmp_addr);
                        None
                    }
                },
                6 => Some(nes.cpu.proc.tmp_addr + ((nes.cpu.proc.tmp1 as u16) << 8)),
                _ => None
            }
        },
        _ => unimplemented!()
    }
}

#[inline(always)]
fn addr_concat(high: u8, low: u8) -> u16 {
    ((high as u16) << 8) | (low as u16)
}

fn read_modify_write(nes: &mut Nes) -> Option<u16> {
    match nes.cpu.proc.mode {
        Zero => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp_addr = addr_concat(0x00, Cpu::fetch(nes));
                    None
                },
                3 => {
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                4 => {
                    nes.write(nes.cpu.proc.tmp_addr, nes.cpu.proc.tmp0);
                    None
                },
                5 => Some(nes.cpu.proc.tmp_addr),
                _ => None
            }
        },
        Absolute => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp0 = Cpu::fetch(nes);
                    None
                },
                3 => {
                    nes.cpu.proc.tmp1 = Cpu::fetch(nes);
                    None
                },
                4 => {
                    nes.cpu.proc.tmp_addr = addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0);
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                5 => {
                    nes.write(nes.cpu.proc.tmp_addr, nes.cpu.proc.tmp0);
                    None
                },
                6 => Some(nes.cpu.proc.tmp_addr),
                _ => None
            }
        },
        ZeroX => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp0 = Cpu::fetch(nes);
                    nes.cpu.proc.tmp_addr = addr_concat(0x00, nes.cpu.proc.tmp0);
                    None
                },
                3 => {
                    nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                4 => {
                    nes.cpu.proc.tmp_addr = ((nes.cpu.proc.tmp0 as u16) + (nes.cpu.x as u16)) & 0x00FF;
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                5 => {
                    nes.write(nes.cpu.proc.tmp_addr, nes.cpu.proc.tmp0);
                    None
                },
                6 => Some(nes.cpu.proc.tmp_addr),
                _ => None
            }
        },
        AbsoluteX => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp0 = Cpu::fetch(nes);
                    None
                },
                3 => {
                    nes.cpu.proc.tmp1 = Cpu::fetch(nes);
                    None
                },
                4 => {
                    nes.read(addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0 + nes.cpu.x));
                    nes.cpu.proc.tmp_addr = addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0) + nes.cpu.x as u16;
                    None
                },
                5 => {
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                6 => {
                    nes.write(nes.cpu.proc.tmp_addr, nes.cpu.proc.tmp0);
                    None
                },
                7 => Some(nes.cpu.proc.tmp_addr),
                _ => None
            }
        },
        IndirectX => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp0 = Cpu::fetch(nes);
                    None
                },
                3 => {
                    nes.cpu.proc.tmp_addr = addr_concat(0x00, nes.cpu.proc.tmp0);
                    nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                4 => {
                    nes.cpu.proc.tmp0 = nes.read((nes.cpu.proc.tmp_addr + nes.cpu.x as u16) & 0x00FF);
                    None
                },
                5 => {
                    nes.cpu.proc.tmp1 = nes.read((nes.cpu.proc.tmp_addr + nes.cpu.x as u16 + 1) & 0x00FF);
                    None
                },
                6 => {
                    nes.cpu.proc.tmp_addr = addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0);
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                7 => {
                    nes.write(nes.cpu.proc.tmp_addr, nes.cpu.proc.tmp0);
                    None
                },
                8 => Some(nes.cpu.proc.tmp_addr),
                _ => None
            }
        },
        IndirectY => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp_addr = addr_concat(0x00, Cpu::fetch(nes));
                    None
                },
                3 => {
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                4 => {
                    nes.cpu.proc.tmp1 = nes.read((nes.cpu.proc.tmp_addr + 1) & 0x00FF);
                    None
                },
                5 => {
                    nes.read(addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0 + nes.cpu.y));
                    nes.cpu.proc.tmp_addr = addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0) + nes.cpu.y as u16;
                    None
                },
                6 => {
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                7 => {
                    nes.write(nes.cpu.proc.tmp_addr, nes.cpu.proc.tmp0);
                    None
                },
                8 => Some(nes.cpu.proc.tmp_addr),
                _ => None
            }
        },
        AbsoluteY => {
            match nes.cpu.proc.cycle {
                2 => {
                    nes.cpu.proc.tmp0 = Cpu::fetch(nes);
                    None
                },
                3 => {
                    nes.cpu.proc.tmp1 = Cpu::fetch(nes);
                    None
                },
                4 => {
                    nes.read(addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0 + nes.cpu.y));
                    nes.cpu.proc.tmp_addr = addr_concat(nes.cpu.proc.tmp1, nes.cpu.proc.tmp0) + nes.cpu.y as u16;
                    None
                },
                5 => {
                    nes.cpu.proc.tmp0 = nes.read(nes.cpu.proc.tmp_addr);
                    None
                },
                6 => {
                    nes.write(nes.cpu.proc.tmp_addr, nes.cpu.proc.tmp0);
                    None
                },
                7 => Some(nes.cpu.proc.tmp_addr),
                _ => None
            }
        },
        _ => unimplemented!("mode: {:?}", nes.cpu.proc.mode)
    }
}