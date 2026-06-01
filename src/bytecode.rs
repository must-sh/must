use std::{collections::HashMap, fmt::Display};

use crate::common::{Binop, Unop};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    PushInt(i32),
    PushBool(bool),
    Binop(Binop),
    Unop(Unop),

    Set { id: usize, size: usize },
    Get { id: usize, size: usize },
    LocalAddr(usize),

    Load { offset: usize, size: usize },
    Store { offset: usize, size: usize },

    // REF, INT -> REF + INT
    CapOffset,

    Drop,

    Call(String),
}

impl Display for Inst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Inst::PushInt(n) => write!(f, "push {n}"),
            Inst::PushBool(b) => write!(f, "push {b}"),
            Inst::Binop(op) => write!(f, "{op}"),
            Inst::Unop(op) => write!(f, "{op}"),
            Inst::Set { id, size } => write!(f, "set {id} size: {size}"),
            Inst::Get { id, size } => write!(f, "get {id} size: {size}"),
            Inst::LocalAddr(n) => write!(f, "addr {n}"),
            Inst::Load { offset, size } => write!(f, "load offset: {offset} size: {size}"),
            Inst::Store { offset, size } => write!(f, "store offset: {offset} size: {size}"),
            Inst::CapOffset => write!(f, "capoffset"),
            Inst::Drop => write!(f, "drop"),
            Inst::Call(name) => write!(f, "call {name:?}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub insts: Vec<Inst>,
    pub terminator: Terminator,
}

impl Display for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for inst in &self.insts {
            writeln!(f, "\t{inst}")?
        }
        writeln!(f, "\t{}", self.terminator)
    }
}

impl Block {
    pub fn empty() -> Self {
        Self {
            insts: vec![],
            terminator: Terminator::Ret,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    Jmp(usize),
    Br(usize, usize),
    Ret,
}

impl Display for Terminator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Terminator::Jmp(n) => write!(f, "jmp blk{n}"),
            Terminator::Br(th, el) => write!(f, "br blk{th} blk{el}"),
            Terminator::Ret => write!(f, "ret"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Func {
    pub blocks: Vec<Block>,
    pub variables: usize,
}

impl Display for Func {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (id, blk) in self.blocks.iter().enumerate() {
            writeln!(f, "block {id}:")?;
            write!(f, "{blk}")?
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prog {
    pub funcs: HashMap<String, Func>,
}

impl Display for Prog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (name, func) in &self.funcs {
            writeln!(f, "{name:?} ({}):", func.variables)?;
            writeln!(f, "{}", func)?;
        }
        Ok(())
    }
}
