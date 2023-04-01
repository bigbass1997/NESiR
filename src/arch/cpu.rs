#![allow(unused_variables)]
#![allow(non_upper_case_globals)]

use std::cell::RefCell;
use std::fmt::{Debug, Formatter};
use std::num::Wrapping;
use std::rc::Rc;
use crate::arch::{Nes, CpuBusAccessible, ClockDivider};
use crate::util::InfCell;
use crate::TestState;
use bitflags::bitflags;
use AddrMode::*;
use crate::arch::cartridge::Cartridge;



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
    func: fn(&mut Self, &mut Cpu, &mut Nes),
    mode: AddrMode,
    cycle: u8,
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
    pub fn new(step_func: fn(&mut InstructionProcedure, &mut Cpu, &mut Nes), addr_mode: AddrMode) -> Self {
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
    
    pub fn step(&mut self, cpu: &mut Cpu, bus: &mut Nes) {
        (self.func)(self, cpu, bus);
        self.cycle += 1;
    }
}


#[derive(Clone, Debug)]
pub struct Cpu {
    pub wram: [u8; 0x800],
    pub pc: u16,
    pub sp: Wrapping<u8>,
    pub status: StatusReg,
    pub acc: u8,
    pub x: u8,
    pub y: u8,
    pub rdy: bool,
    pub(crate) prefetch: Option<u8>,
    fetch_needed: bool,
    procedure: Option<InstructionProcedure>,
    pub clock_divider: ClockDivider<12>,
    pub cyc: usize,
    pub last_state: Option<TestState>,
}
impl Default for Cpu {
    fn default() -> Self {
        Self {
            wram: [0u8; 0x800],
            pc: 0,
            sp: Wrapping(0xFD), // actually this is potentialy random at power-on // software typically initializes this to 0xFF
            status: StatusReg::default(),
            acc: 0,
            x: 0,
            y: 0,
            rdy: true,
            prefetch: None,
            fetch_needed: false, // used for debugging
            procedure: None,
            clock_divider: ClockDivider::new(0), //todo: randomize
            cyc: 0,
            last_state: None,
        }
    }
}

impl Cpu {
    pub fn init_pc(&mut self, nes: &mut Nes) {
        self.pc = ((nes.cart.read_cpu(0xFFFD) as u16) << 8) | (nes.cart.read_cpu(0xFFFC) as u16);
        self.prefetch = Some(self.fetch(nes));
    }
    
    pub fn tick(&mut self, nes_cell: &InfCell<Nes>) {
        if self.clock_divider.tick() {
            self.cycle(nes_cell);
        }
    }
    
    pub fn cycle(&mut self, nes_cell: &InfCell<Nes>) {
        if !self.rdy {
            return;
        }
        
        let nes = nes_cell.get_mut();
        //let nes_ref = nes_cell.get_mut();
        
        if self.procedure.is_none() {
            if self.prefetch.is_none() { // if next instruction wasn't prefetched at end of previous, we must fetch now (this is considered the first cycle of procedure)
                panic!("prefetch should always occur");
                /*self.prefetch = Some(self.fetch(nes));
                
                self.last_state = Some(TestState::from_cpu(nes_cell));
                println!("Fetched! PC: {:04X}, Op: {:02X}, Status: {}, ACC: {:02X}, X: {:02X}, Y: {:02X}, SP: {:02X}, PPU: {}, CYC: {}", self.pc - 1, self.prefetch.unwrap(), self.status, self.acc, self.x, self.y, self.sp, nes.ppu.pos, self.cyc);
                //println!("Fetched! {:04X} {:02X}        Status: {}, ACC: {:02X}, X: {:02X}, Y: {:02X}, SP: {:02X}, CYC:{}", self.pc - 1, self.prefetch.unwrap(), self.status, self.acc, self.x, self.y, self.sp, self.cyc);
                self.fetch_needed = true;*/
            }
            
            let opcode = self.prefetch.unwrap();
            self.prefetch = None;
            
            self.procedure = Some(match opcode {
            
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
            
            _ => panic!("Attempt to run invalid/unimplemented opcode! PC: {:#06X}, Op: {:#06X}", self.pc, opcode)
        }); // decode opcode into an instruction procedure (this doesn't consume cycles)
            
            // debugging
            if !self.fetch_needed {
                self.last_state = Some(TestState::from_cpu(nes_cell));
                //println!("         {:04X} {:02X}        Status: {}, ACC: {:02X}, X: {:02X}, Y: {:02X}, SP: {:02X}, CYC:{}", self.pc - 1, opcode, self.status, self.acc, self.x, self.y, self.sp, self.cyc);
                println!("         PC: {:04X}, Op: {:02X}, Status: {}, ACC: {:02X}, X: {:02X}, Y: {:02X}, SP: {:02X}, PPU: {}, CYC: {}", self.pc - 1, opcode, self.status, self.acc, self.x, self.y, self.sp, nes.ppu.pos, self.cyc);
            }
            self.fetch_needed = false;
        }
        
        let mut proc = self.procedure.unwrap();
        proc.step(self, nes);
        
        if proc.done {
            self.procedure = None;
        } else {
            self.procedure = Some(proc);
        }
        
        self.cyc += 1;
    }
    
    pub(crate) fn fetch(&mut self, bus: &mut Nes) -> u8 {
        let fetch = bus.read(self.pc);
        self.pc += 1;
        
        fetch
    }
    
    fn stack_push(&mut self, bus: &mut Nes, data: u8) {
        bus.write(0x100 + self.sp.0 as u16, data);
        self.sp -= Wrapping(1);
    }
    
    fn stack_pull(&mut self, bus: &mut Nes) -> u8 {
        self.sp += Wrapping(1);
        bus.read(0x100 + self.sp.0 as u16)
    }
}
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

fn adc(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        let data = bus.read(addr);
        
        let result = (cpu.acc as u16).wrapping_add(data as u16).wrapping_add(cpu.status.contains(StatusReg::Carry) as u16);
        
        cpu.status.set(StatusReg::Carry, result & 0x100 != 0);
        cpu.status.set(StatusReg::Overflow, (!(cpu.acc ^ data) & (cpu.acc ^ result as u8) & 0x80) != 0);
        cpu.status.set(StatusReg::Zero, (result as u8) == 0);
        cpu.status.set(StatusReg::Negative, result & 0x80 > 0);
        cpu.acc = result as u8;
        
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn anc(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) { unimplemented!() }
fn and(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        cpu.acc &= bus.read(addr);
        
        cpu.status.set(StatusReg::Zero, cpu.acc == 0);
        cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn ane(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) { unimplemented!() }
fn arr(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) { unimplemented!() }
fn asl(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.mode {
        Accumulator => {
            match procedure.cycle {
                2 => {
                    cpu.status.set(StatusReg::Carry, cpu.acc & 0x80 != 0);
                    cpu.acc <<= 1;
                    
                    cpu.status.set(StatusReg::Zero, cpu.acc == 0);
                    cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
                    cpu.prefetch = Some(cpu.fetch(bus));
                    procedure.done = true;
                },
                _ => ()
            }
        },
        _ => {
            if let Some(addr) = read_modify_write(procedure, cpu, bus) {
                cpu.status.set(StatusReg::Carry, procedure.tmp0 & 0x80 != 0);
                procedure.tmp0 <<= 1;
                
                cpu.status.set(StatusReg::Zero, procedure.tmp0 == 0);
                cpu.status.set(StatusReg::Negative, procedure.tmp0 & 0x80 > 0);
                bus.write(addr, procedure.tmp0);
                cpu.prefetch = Some(cpu.fetch(bus));
                
                procedure.done = true;
            }
        }
    }
}
fn asr(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) { unimplemented!() }
fn bcc(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    branch(procedure, cpu, bus, !cpu.status.contains(StatusReg::Carry));
}
fn bcs(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    branch(procedure, cpu, bus, cpu.status.contains(StatusReg::Carry));
}
fn beq(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    branch(procedure, cpu, bus, cpu.status.contains(StatusReg::Zero));
}
fn bit(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        let tmp = bus.read(addr);
        
        cpu.status.set(StatusReg::Zero, tmp & cpu.acc == 0);
        cpu.status.set(StatusReg::Overflow, tmp & 0x40 > 0);
        cpu.status.set(StatusReg::Negative, tmp & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn bmi(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    branch(procedure, cpu, bus, cpu.status.contains(StatusReg::Negative));
}
fn bne(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    branch(procedure, cpu, bus, !cpu.status.contains(StatusReg::Zero));
}
fn bpl(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    branch(procedure, cpu, bus, !cpu.status.contains(StatusReg::Negative));
}
fn brk(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => { cpu.fetch(bus); },
        3 => cpu.stack_push(bus, (cpu.pc >> 8) as u8),
        4 => cpu.stack_push(bus, cpu.pc as u8),
        5 => cpu.stack_push(bus, cpu.status.bits | 0b00010000),
        6 => procedure.tmp0 = bus.read(0xFFFE),
        7 => {
            procedure.tmp1 = bus.read(0xFFFF);
            
            cpu.pc = ((procedure.tmp1 as u16) << 8) | (procedure.tmp0 as u16);
            cpu.prefetch = Some(cpu.fetch(bus));
            
            procedure.done = true;
        }
        _ => ()
    }
}
fn bvc(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    branch(procedure, cpu, bus, !cpu.status.contains(StatusReg::Overflow));
}
fn bvs(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    branch(procedure, cpu, bus, cpu.status.contains(StatusReg::Overflow));
}

fn branch(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes, to_branch: bool) {
    match procedure.cycle {
        2 => {
            procedure.tmp0 = cpu.fetch(bus);
            if !to_branch { // if to_branch is false, do not branch
                cpu.prefetch = Some(cpu.fetch(bus));
                procedure.done = true;
            }
        },
        3 => {
            procedure.tmp_addr = (cpu.pc as i16 + procedure.tmp0 as i8 as i16) as u16;
            if (cpu.pc & 0xFF00) == (procedure.tmp_addr & 0xFF00) { // branch to same page
                cpu.pc = procedure.tmp_addr;
                cpu.prefetch = Some(cpu.fetch(bus));
                procedure.done = true;
            }
        },
        4 => {
            cpu.pc = procedure.tmp_addr;
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}

fn clc(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.status.set(StatusReg::Carry, false);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn cld(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.status.set(StatusReg::Decimal, false);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn cli(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.status.set(StatusReg::InterruptDisable, false);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn clv(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.status.set(StatusReg::Overflow, false);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn cmp(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        let data = bus.read(addr);
        
        cpu.status.set(StatusReg::Carry, cpu.acc >= data);
        cpu.status.set(StatusReg::Zero, cpu.acc == data);
        cpu.status.set(StatusReg::Negative, cpu.acc.wrapping_sub(data) & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn cpx(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        let data = bus.read(addr);
        
        cpu.status.set(StatusReg::Carry, cpu.x >= data);
        cpu.status.set(StatusReg::Zero, cpu.x == data);
        cpu.status.set(StatusReg::Negative, cpu.x.wrapping_sub(data) & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn cpy(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        let data = bus.read(addr);
        
        cpu.status.set(StatusReg::Carry, cpu.y >= data);
        cpu.status.set(StatusReg::Zero, cpu.y == data);
        cpu.status.set(StatusReg::Negative, cpu.y.wrapping_sub(data) & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn dcp(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = read_modify_write(procedure, cpu, bus) {
        procedure.tmp0 = procedure.tmp0.wrapping_sub(1);
        
        cpu.status.set(StatusReg::Carry, cpu.acc >= procedure.tmp0);
        cpu.status.set(StatusReg::Zero, cpu.acc == procedure.tmp0);
        cpu.status.set(StatusReg::Negative, cpu.acc.wrapping_sub(procedure.tmp0) & 0x80 > 0);
        bus.write(addr, procedure.tmp0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn dec(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = read_modify_write(procedure, cpu, bus) {
        procedure.tmp0 = procedure.tmp0.wrapping_sub(1);
        
        cpu.status.set(StatusReg::Zero, procedure.tmp0 == 0);
        cpu.status.set(StatusReg::Negative, procedure.tmp0 & 0x80 > 0);
        bus.write(addr, procedure.tmp0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn dex(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.x = cpu.x.wrapping_sub(1);
            cpu.status.set(StatusReg::Zero, cpu.x == 0);
            cpu.status.set(StatusReg::Negative, cpu.x & 0x80 > 0);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn dey(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.y = cpu.y.wrapping_sub(1);
            cpu.status.set(StatusReg::Zero, cpu.y == 0);
            cpu.status.set(StatusReg::Negative, cpu.y & 0x80 > 0);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn eor(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        cpu.acc ^= bus.read(addr);
        
        cpu.status.set(StatusReg::Zero, cpu.acc == 0);
        cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn inc(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = read_modify_write(procedure, cpu, bus) {
        procedure.tmp0 = procedure.tmp0.wrapping_add(1);
        
        cpu.status.set(StatusReg::Zero, procedure.tmp0 == 0);
        cpu.status.set(StatusReg::Negative, procedure.tmp0 & 0x80 > 0);
        bus.write(addr, procedure.tmp0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn inx(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.x = cpu.x.wrapping_add(1);
            cpu.status.set(StatusReg::Zero, cpu.x == 0);
            cpu.status.set(StatusReg::Negative, cpu.x & 0x80 > 0);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn iny(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.y = cpu.y.wrapping_add(1);
            cpu.status.set(StatusReg::Zero, cpu.y == 0);
            cpu.status.set(StatusReg::Negative, cpu.y & 0x80 > 0);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn isb(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = read_modify_write(procedure, cpu, bus) {
        procedure.tmp0 = procedure.tmp0.wrapping_add(1);
        
        let data = !procedure.tmp0; //TODO: Check if we should use tmp0 POST-increment or PRE-increment
        
        let result = (cpu.acc as u16).wrapping_add(data as u16).wrapping_add(cpu.status.contains(StatusReg::Carry) as u16);
        
        cpu.status.set(StatusReg::Carry, result & 0x100 != 0);
        cpu.status.set(StatusReg::Overflow, (!(cpu.acc ^ data) & (cpu.acc ^ result as u8) & 0x80) != 0);
        cpu.status.set(StatusReg::Zero, (result as u8) == 0);
        cpu.status.set(StatusReg::Negative, result & 0x80 > 0);
        cpu.acc = result as u8;
        
        bus.write(addr, procedure.tmp0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn jmp(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.mode {
        Absolute => {
            match procedure.cycle {
                2 => procedure.tmp0 = cpu.fetch(bus),
                3 => {
                    let pch = bus.read(cpu.pc) as u16;
                    cpu.pc = (pch << 8) | (procedure.tmp0 as u16);
                    cpu.prefetch = Some(cpu.fetch(bus));
                    procedure.done = true;
                },
                _ => ()
            }
        },
        Indirect => {
            match procedure.cycle {
                2 => procedure.tmp0 = cpu.fetch(bus),
                3 => procedure.tmp1 = cpu.fetch(bus),
                4 => {
                    procedure.tmp_addr = ((procedure.tmp1 as u16) << 8) | (procedure.tmp0 as u16);
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                },
                5 => {
                    procedure.tmp1 = bus.read(((procedure.tmp_addr + 1) & 0x00FF) + (procedure.tmp_addr & 0xFF00));
                    
                    cpu.pc = ((procedure.tmp1 as u16) << 8) | (procedure.tmp0 as u16);
                    cpu.prefetch = Some(cpu.fetch(bus));
                    procedure.done = true;
                }
                _ => ()
            }
        },
        _ => panic!("Invalid mode!")
    }
}
fn jsr(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => procedure.tmp0 = cpu.fetch(bus),
        3 => {bus.read(0x100 + cpu.sp.0 as u16);}, // discarded read, may be useful later for monitoring bus activity
        4 => cpu.stack_push(bus, (cpu.pc >> 8) as u8),
        5 => cpu.stack_push(bus, (cpu.pc & 0xFF) as u8),
        6 => {
            procedure.tmp1 = cpu.fetch(bus);
            
            cpu.pc = ((procedure.tmp1 as u16) << 8) | (procedure.tmp0 as u16);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn las(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        let tmp = bus.read(addr) & cpu.sp.0;
        cpu.acc = tmp;
        cpu.x = tmp;
        cpu.sp.0 = tmp;
        
        cpu.status.set(StatusReg::Zero, tmp == 0);
        cpu.status.set(StatusReg::Negative, tmp & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn lax(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        cpu.acc = bus.read(addr);
        cpu.x = bus.read(addr);
        
        cpu.status.set(StatusReg::Zero, cpu.x == 0);
        cpu.status.set(StatusReg::Negative, cpu.x & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn lda(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        cpu.acc = bus.read(addr);
        
        cpu.status.set(StatusReg::Zero, cpu.acc == 0);
        cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn ldx(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        cpu.x = bus.read(addr);
        
        cpu.status.set(StatusReg::Zero, cpu.x == 0);
        cpu.status.set(StatusReg::Negative, cpu.x & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn ldy(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        cpu.y = bus.read(addr);
        
        cpu.status.set(StatusReg::Zero, cpu.y == 0);
        cpu.status.set(StatusReg::Negative, cpu.y & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn lsr(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.mode {
        Accumulator => {
            match procedure.cycle {
                2 => {
                    cpu.status.set(StatusReg::Carry, cpu.acc & 0x01 != 0);
                    cpu.acc >>= 1;
                    
                    cpu.status.set(StatusReg::Zero, cpu.acc == 0);
                    cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
                    cpu.prefetch = Some(cpu.fetch(bus));
                    procedure.done = true;
                },
                _ => ()
            }
        },
        _ => {
            if let Some(addr) = read_modify_write(procedure, cpu, bus) {
                cpu.status.set(StatusReg::Carry, procedure.tmp0 & 0x01 != 0);
                procedure.tmp0 >>= 1;
                
                cpu.status.set(StatusReg::Zero, procedure.tmp0 == 0);
                cpu.status.set(StatusReg::Negative, procedure.tmp0 & 0x80 > 0);
                bus.write(addr, procedure.tmp0);
                cpu.prefetch = Some(cpu.fetch(bus));
                
                procedure.done = true;
            }
        }
    }
}
fn lxa(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) { unimplemented!() }
fn nop(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if procedure.mode == Implied {
        if procedure.cycle == 2 {
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        }
    } else if let Some(_) = effective_addr(procedure, cpu, bus) {
        cpu.prefetch = Some(cpu.fetch(bus));
        procedure.done = true;
    }
}
fn ora(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        cpu.acc |= bus.read(addr);
        
        cpu.status.set(StatusReg::Zero, cpu.acc == 0);
        cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn pha(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => { bus.read(cpu.pc); },
        3 => {
            cpu.stack_push(bus, cpu.acc);
            cpu.prefetch = Some(cpu.fetch(bus));
            
            procedure.done = true;
        },
        _ => ()
    }
}
fn php(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => { bus.read(cpu.pc); },
        3 => {
            cpu.stack_push(bus, cpu.status.bits | 0b00110000); //TODO: Verify bits [5:4] are supposed to be set by PHP
            cpu.prefetch = Some(cpu.fetch(bus));
            
            procedure.done = true;
        },
        _ => ()
    }
}
fn pla(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => { bus.read(cpu.pc); },
        3 => { bus.read(cpu.sp.0 as u16 + 0x100u16); }
        4 => {
            cpu.acc = cpu.stack_pull(bus);
            
            cpu.status.set(StatusReg::Zero, cpu.acc == 0);
            cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
            cpu.prefetch = Some(cpu.fetch(bus));
            
            procedure.done = true;
        },
        _ => ()
    }
}
fn plp(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => { bus.read(cpu.pc); },
        3 => { bus.read(cpu.sp.0 as u16 + 0x100u16); }
        4 => {
            cpu.status.bits = cpu.stack_pull(bus) & 0b11001111; //TODO: Verify bits [5:4] are supposed to be ignored by PLP
            cpu.status.bits |= 0b00100000; // Apparently, bit 5 should always be set
            
            cpu.prefetch = Some(cpu.fetch(bus));
            
            procedure.done = true;
        },
        _ => ()
    }
}
fn rla(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = read_modify_write(procedure, cpu, bus) {
        let c = cpu.status.contains(StatusReg::Carry) as u8;
        cpu.status.set(StatusReg::Carry, procedure.tmp0 & 0x80 != 0);
        procedure.tmp0 = ((procedure.tmp0 << 1) & 0xFE) | c;
        
        cpu.acc &= procedure.tmp0;
        
        cpu.status.set(StatusReg::Zero, cpu.acc == 0);
        cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
        bus.write(addr, procedure.tmp0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn rra(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = read_modify_write(procedure, cpu, bus) {
        let c = cpu.status.contains(StatusReg::Carry) as u8;
        cpu.status.set(StatusReg::Carry, procedure.tmp0 & 0x01 != 0);
        procedure.tmp0 = (c << 7) | ((procedure.tmp0 >> 1) & 0x7F);
        let data = procedure.tmp0;
        
        let result = (cpu.acc as u16).wrapping_add(data as u16).wrapping_add(cpu.status.contains(StatusReg::Carry) as u16);
        
        cpu.status.set(StatusReg::Carry, result & 0x100 != 0);
        cpu.status.set(StatusReg::Overflow, (!(cpu.acc ^ data) & (cpu.acc ^ result as u8) & 0x80) != 0);
        cpu.status.set(StatusReg::Zero, (result as u8) == 0);
        cpu.status.set(StatusReg::Negative, result & 0x80 > 0);
        cpu.acc = result as u8;
        bus.write(addr, procedure.tmp0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn rol(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.mode {
        Accumulator => {
            match procedure.cycle {
                2 => {
                    let c = cpu.status.contains(StatusReg::Carry) as u8;
                    cpu.status.set(StatusReg::Carry, cpu.acc & 0x80 != 0);
                    cpu.acc = ((cpu.acc << 1) & 0xFE) | c;
                    
                    cpu.status.set(StatusReg::Zero, cpu.acc == 0);
                    cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
                    cpu.prefetch = Some(cpu.fetch(bus));
                    procedure.done = true;
                },
                _ => ()
            }
        },
        _ => {
            if let Some(addr) = read_modify_write(procedure, cpu, bus) {
                let c = cpu.status.contains(StatusReg::Carry) as u8;
                cpu.status.set(StatusReg::Carry, procedure.tmp0 & 0x80 != 0);
                procedure.tmp0 = ((procedure.tmp0 << 1) & 0xFE) | c;
                
                cpu.status.set(StatusReg::Zero, procedure.tmp0 == 0);
                cpu.status.set(StatusReg::Negative, procedure.tmp0 & 0x80 > 0);
                bus.write(addr, procedure.tmp0);
                cpu.prefetch = Some(cpu.fetch(bus));
                
                procedure.done = true;
            }
        }
    }
}
fn ror(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.mode {
        Accumulator => {
            match procedure.cycle {
                2 => {
                    let c = cpu.status.contains(StatusReg::Carry) as u8;
                    cpu.status.set(StatusReg::Carry, cpu.acc & 0x01 != 0);
                    cpu.acc = (c << 7) | ((cpu.acc >> 1) & 0x7F);
                    
                    cpu.status.set(StatusReg::Zero, cpu.acc == 0);
                    cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
                    cpu.prefetch = Some(cpu.fetch(bus));
                    procedure.done = true;
                },
                _ => ()
            }
        },
        _ => {
            if let Some(addr) = read_modify_write(procedure, cpu, bus) {
                let c = cpu.status.contains(StatusReg::Carry) as u8;
                cpu.status.set(StatusReg::Carry, procedure.tmp0 & 0x01 != 0);
                procedure.tmp0 = (c << 7) | ((procedure.tmp0 >> 1) & 0x7F);
                
                cpu.status.set(StatusReg::Zero, procedure.tmp0 == 0);
                cpu.status.set(StatusReg::Negative, procedure.tmp0 & 0x80 > 0);
                bus.write(addr, procedure.tmp0);
                cpu.prefetch = Some(cpu.fetch(bus));
                
                procedure.done = true;
            }
        }
    }
}
fn rti(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {cpu.fetch(bus);},
        3 => {bus.read(0x100 + cpu.sp.0 as u16);}
        4 => {
            cpu.status.bits = cpu.stack_pull(bus) & 0b11001111; //TODO: Verify bits [5:4] are supposed to be ignored by PLP
            cpu.status.bits |= 0b00100000; // Apparently, bit 5 should always be set
        },
        5 => procedure.tmp0 = cpu.stack_pull(bus),
        6 => {
            procedure.tmp1 = cpu.stack_pull(bus);
            
            cpu.pc = addr_concat(procedure.tmp1, procedure.tmp0);
            cpu.prefetch = Some(cpu.fetch(bus));
            
            procedure.done = true;
        }
        _ => (),
    }
}
fn rts(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {cpu.fetch(bus);},
        3 => {bus.read(0x100 + cpu.sp.0 as u16);}
        4 => procedure.tmp0 = cpu.stack_pull(bus),
        5 => procedure.tmp1 = cpu.stack_pull(bus),
        6 => {
            cpu.pc = addr_concat(procedure.tmp1, procedure.tmp0) + 1;
            bus.read(cpu.pc);
            cpu.prefetch = Some(cpu.fetch(bus));
            
            procedure.done = true;
        }
        _ => (),
    }
}
fn sax(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        bus.write(addr, cpu.acc & cpu.x);
        cpu.prefetch = Some(cpu.fetch(bus));
        procedure.done = true;
    }
}
fn sbc(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        let data = !bus.read(addr);
        
        let result = (cpu.acc as u16).wrapping_add(data as u16).wrapping_add(cpu.status.contains(StatusReg::Carry) as u16);
        
        cpu.status.set(StatusReg::Carry, result & 0x100 != 0);
        cpu.status.set(StatusReg::Overflow, (!(cpu.acc ^ data) & (cpu.acc ^ result as u8) & 0x80) != 0);
        cpu.status.set(StatusReg::Zero, (result as u8) == 0);
        cpu.status.set(StatusReg::Negative, result & 0x80 > 0);
        cpu.acc = result as u8;
        
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn sbx(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) { unimplemented!() }
fn sec(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.status.set(StatusReg::Carry, true);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn sed(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.status.set(StatusReg::Decimal, true);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn sei(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.status.set(StatusReg::InterruptDisable, true);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn sha(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) { unimplemented!() } // Reminder: consume extra cycle write-instruction using AbsoluteX, AbsoluteY, or IndirectY
fn shs(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) { unimplemented!() }
fn shx(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) { unimplemented!() } // Reminder: consume extra cycle write-instruction using AbsoluteX or AbsoluteY
fn shy(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) { unimplemented!() } // Reminder: consume extra cycle write-instruction using AbsoluteX or AbsoluteY
fn slo(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = read_modify_write(procedure, cpu, bus) {
        cpu.status.set(StatusReg::Carry, procedure.tmp0 & 0x80 != 0);
        procedure.tmp0 <<= 1;
        
        cpu.acc |= procedure.tmp0;
        
        cpu.status.set(StatusReg::Zero, cpu.acc == 0);
        cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
        bus.write(addr, procedure.tmp0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn sre(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = read_modify_write(procedure, cpu, bus) {
        cpu.status.set(StatusReg::Carry, procedure.tmp0 & 0x01 != 0);
        procedure.tmp0 >>= 1;
        
        cpu.acc ^= procedure.tmp0;
        
        cpu.status.set(StatusReg::Zero, cpu.acc == 0);
        cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
        bus.write(addr, procedure.tmp0);
        cpu.prefetch = Some(cpu.fetch(bus));
        
        procedure.done = true;
    }
}
fn sta(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        if ((procedure.mode == AbsoluteX || procedure.mode == AbsoluteY) && procedure.cycle == 4) || (procedure.mode == IndirectY && procedure.cycle == 5) {
            return; // consume extra cycle write-instruction using AbsoluteX, AbsoluteY, or IndirectY
        }
        bus.write(addr, cpu.acc);
        cpu.prefetch = Some(cpu.fetch(bus));
        procedure.done = true;
    }
}
fn stx(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        if (procedure.mode == AbsoluteX || procedure.mode == AbsoluteY) && procedure.cycle == 4 {
            return; // consume extra cycle write-instruction using AbsoluteX or AbsoluteY
        }
        bus.write(addr, cpu.x);
        cpu.prefetch = Some(cpu.fetch(bus));
        procedure.done = true;
    }
}
fn sty(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    if let Some(addr) = effective_addr(procedure, cpu, bus) {
        if (procedure.mode == AbsoluteX || procedure.mode == AbsoluteY) && procedure.cycle == 4 {
            return; // consume extra cycle write-instruction using AbsoluteX or AbsoluteY
        }
        bus.write(addr, cpu.y);
        cpu.prefetch = Some(cpu.fetch(bus));
        procedure.done = true;
    }
}
fn tax(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.x = cpu.acc;
            
            cpu.status.set(StatusReg::Zero, cpu.x == 0);
            cpu.status.set(StatusReg::Negative, cpu.x & 0x80 > 0);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn tay(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.y = cpu.acc;
            
            cpu.status.set(StatusReg::Zero, cpu.y == 0);
            cpu.status.set(StatusReg::Negative, cpu.y & 0x80 > 0);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn tsx(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.x = cpu.sp.0;
            
            cpu.status.set(StatusReg::Zero, cpu.x == 0);
            cpu.status.set(StatusReg::Negative, cpu.x & 0x80 > 0);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn txa(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.acc = cpu.x;
            
            cpu.status.set(StatusReg::Zero, cpu.acc == 0);
            cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn txs(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.sp.0 = cpu.x;
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}
fn tya(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) {
    match procedure.cycle {
        2 => {
            cpu.acc = cpu.y;
            
            cpu.status.set(StatusReg::Zero, cpu.acc == 0);
            cpu.status.set(StatusReg::Negative, cpu.acc & 0x80 > 0);
            cpu.prefetch = Some(cpu.fetch(bus));
            procedure.done = true;
        },
        _ => ()
    }
}

fn effective_addr(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) -> Option<u16> {
    match procedure.mode {
        Immediate => {
            match procedure.cycle {
                2 => {
                    let pc = cpu.pc;
                    cpu.pc += 1;
                    
                    Some(pc)
                },
                _ => None
            }
        },
        Zero => {
            match procedure.cycle {
                2 => {
                    procedure.tmp0 = cpu.fetch(bus);
                    None
                },
                3 => {
                    Some(addr_concat(0x00, procedure.tmp0))
                },
                _ => None
            }
        },
        Absolute => {
            match procedure.cycle {
                2 => {
                    procedure.tmp0 = cpu.fetch(bus);
                    None
                },
                3 => {
                    procedure.tmp1 = cpu.fetch(bus);
                    None
                },
                4 => {
                    Some(addr_concat(procedure.tmp1, procedure.tmp0))
                },
                _ => None
            }
        },
        IndirectX => {
            match procedure.cycle {
                2 => {
                    procedure.tmp0 = cpu.fetch(bus);
                    None
                },
                3 => {
                    bus.read(addr_concat(0x00, procedure.tmp0));
                    None
                },
                4 => {
                    procedure.tmp_addr = addr_concat(0x00, procedure.tmp0.wrapping_add(cpu.x));
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                    None
                },
                5 => {
                    procedure.tmp1 = bus.read((procedure.tmp_addr + 1) & 0x00FF);
                    None
                }
                6 => Some(addr_concat(procedure.tmp1, procedure.tmp0)),
                _ => None,
            }
        },
        AbsoluteX | AbsoluteY => { // All write instructions should make sure they use 5 cycles for this mode
            match procedure.cycle {
                2 => {
                    procedure.tmp0 = cpu.fetch(bus);
                    None
                },
                3 => {
                    procedure.tmp1 = cpu.fetch(bus);
                    None
                },
                4 => {
                    let index = if procedure.mode == AbsoluteX {
                        cpu.x
                    } else {
                        cpu.y
                    };
                    
                    let (result, carry) = procedure.tmp0.overflowing_add(index);
                    procedure.tmp_addr = addr_concat(procedure.tmp1 + carry as u8, result);
                    
                    if !carry {
                        Some(procedure.tmp_addr)
                    } else {
                        bus.read(procedure.tmp_addr);
                        None
                    }
                },
                5 => Some(procedure.tmp_addr),
                _ => None
            }
        },
        ZeroX | ZeroY => {
            match procedure.cycle {
                2 => {
                    procedure.tmp0 = cpu.fetch(bus);
                    None
                },
                3 => {
                    bus.read(addr_concat(0x00, procedure.tmp0));
                    None
                }
                4 => {
                    if procedure.mode == ZeroX {
                        Some(((procedure.tmp0 as u16) + (cpu.x as u16)) & 0x00FF) // TODO: Change to `(procedure.tmp0 + cpu.x) as u16`
                    } else {
                        Some(((procedure.tmp0 as u16) + (cpu.y as u16)) & 0x00FF) // TODO: Change to `(procedure.tmp0 + cpu.y) as u16`
                    }
                }
                _ => None
            }
        },
        IndirectY => {
            match procedure.cycle {
                2 => {
                    procedure.tmp_addr = addr_concat(0x00, cpu.fetch(bus));
                    None
                },
                3 => {
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                    None
                },
                4 => {
                    procedure.tmp1 = bus.read((procedure.tmp_addr + 1) & 0x00FF);
                    None
                },
                5 => {
                    let (result, carry) = procedure.tmp0.overflowing_add(cpu.y);
                    procedure.tmp_addr = addr_concat(procedure.tmp1 + carry as u8, result);
                    
                    if !carry {
                        Some(procedure.tmp_addr)
                    } else {
                        bus.read(procedure.tmp_addr);
                        None
                    }
                },
                6 => Some(procedure.tmp_addr),
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

fn read_modify_write(procedure: &mut InstructionProcedure, cpu: &mut Cpu, bus: &mut Nes) -> Option<u16> {
    match procedure.mode {
        Zero => {
            match procedure.cycle {
                2 => {
                    procedure.tmp_addr = addr_concat(0x00, cpu.fetch(bus));
                    None
                },
                3 => {
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                    None
                },
                4 => {
                    bus.write(procedure.tmp_addr, procedure.tmp0);
                    None
                },
                5 => Some(procedure.tmp_addr),
                _ => None
            }
        },
        Absolute => {
            match procedure.cycle {
                2 => {
                    procedure.tmp0 = cpu.fetch(bus);
                    None
                },
                3 => {
                    procedure.tmp1 = cpu.fetch(bus);
                    None
                },
                4 => {
                    procedure.tmp_addr = addr_concat(procedure.tmp1, procedure.tmp0);
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                    None
                },
                5 => {
                    bus.write(procedure.tmp_addr, procedure.tmp0);
                    None
                },
                6 => Some(procedure.tmp_addr),
                _ => None
            }
        },
        ZeroX => {
            match procedure.cycle {
                2 => {
                    procedure.tmp0 = cpu.fetch(bus);
                    procedure.tmp_addr = addr_concat(0x00, procedure.tmp0);
                    None
                },
                3 => {
                    bus.read(procedure.tmp_addr);
                    None
                },
                4 => {
                    procedure.tmp_addr = ((procedure.tmp0 as u16) + (cpu.x as u16)) & 0x00FF;
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                    None
                },
                5 => {
                    bus.write(procedure.tmp_addr, procedure.tmp0);
                    None
                },
                6 => Some(procedure.tmp_addr),
                _ => None
            }
        },
        AbsoluteX => {
            match procedure.cycle {
                2 => {
                    procedure.tmp0 = cpu.fetch(bus);
                    None
                },
                3 => {
                    procedure.tmp1 = cpu.fetch(bus);
                    None
                },
                4 => {
                    procedure.tmp_addr = addr_concat(procedure.tmp1, procedure.tmp0) + cpu.x as u16; //TODO: Revisit whether carry needs to be added to upper or not
                    bus.read(procedure.tmp_addr);
                    None
                },
                5 => {
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                    None
                },
                6 => {
                    bus.write(procedure.tmp_addr, procedure.tmp0);
                    None
                },
                7 => Some(procedure.tmp_addr),
                _ => None
            }
        },
        IndirectX => {
            match procedure.cycle {
                2 => {
                    procedure.tmp0 = cpu.fetch(bus);
                    None
                },
                3 => {
                    procedure.tmp_addr = addr_concat(0x00, procedure.tmp0);
                    bus.read(procedure.tmp_addr);
                    None
                },
                4 => {
                    procedure.tmp0 = bus.read((procedure.tmp_addr + cpu.x as u16) & 0x00FF);
                    None
                },
                5 => {
                    procedure.tmp1 = bus.read((procedure.tmp_addr + cpu.x as u16 + 1) & 0x00FF);
                    None
                },
                6 => {
                    procedure.tmp_addr = addr_concat(procedure.tmp1, procedure.tmp0);
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                    None
                },
                7 => {
                    bus.write(procedure.tmp_addr, procedure.tmp0);
                    None
                },
                8 => Some(procedure.tmp_addr),
                _ => None
            }
        },
        IndirectY => {
            match procedure.cycle {
                2 => {
                    procedure.tmp_addr = addr_concat(0x00, cpu.fetch(bus));
                    None
                },
                3 => {
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                    None
                },
                4 => {
                    procedure.tmp1 = bus.read((procedure.tmp_addr + 1) & 0x00FF);
                    None
                },
                5 => {
                    bus.read(addr_concat(procedure.tmp1, procedure.tmp0 + cpu.y));
                    procedure.tmp_addr = addr_concat(procedure.tmp1, procedure.tmp0) + cpu.y as u16;
                    None
                },
                6 => {
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                    None
                },
                7 => {
                    bus.write(procedure.tmp_addr, procedure.tmp0);
                    None
                },
                8 => Some(procedure.tmp_addr),
                _ => None
            }
        },
        AbsoluteY => {
            match procedure.cycle {
                2 => {
                    procedure.tmp0 = cpu.fetch(bus);
                    None
                },
                3 => {
                    procedure.tmp1 = cpu.fetch(bus);
                    None
                },
                4 => {
                    procedure.tmp_addr = addr_concat(procedure.tmp1, procedure.tmp0) + cpu.y as u16; //TODO: Revisit whether carry needs to be added to upper or not
                    bus.read(procedure.tmp_addr);
                    None
                },
                5 => {
                    procedure.tmp0 = bus.read(procedure.tmp_addr);
                    None
                },
                6 => {
                    bus.write(procedure.tmp_addr, procedure.tmp0);
                    None
                },
                7 => Some(procedure.tmp_addr),
                _ => None
            }
        },
        _ => unimplemented!("mode: {:?}", procedure.mode)
    }
}