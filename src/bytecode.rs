use std::{collections::HashMap, fmt::Display};

use crate::{
    common::{Binop, Unop},
    tp::FnSig,
};

#[derive(Debug, Clone)]
pub enum Inst {
    PushInt(i64),
    PushBool(bool),
    Binop(Binop),
    Unop(Unop),

    Set { id: usize, offset: i32, tp: Type },
    Get { id: usize, offset: i32, tp: Type },
    LocalAddr { id: usize, offset: i32 },

    // PTR ON TOP, VALUE SECOND
    Load { offset: i32, tp: Type },
    Store { offset: i32, tp: Type },

    // SRC ON TOP, DST SECOND
    MemCopy { size: u64 },

    // REF, INT -> REF + INT
    CapOffset,

    Drop(Type),
    Dup,

    Call(String),
}

impl Display for Inst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Inst::PushInt(n) => write!(f, "push {n}"),
            Inst::PushBool(b) => write!(f, "push {b}"),
            Inst::Binop(op) => write!(f, "{op}"),
            Inst::Unop(op) => write!(f, "{op}"),
            Inst::Set { id, offset, tp } => write!(f, "set ${id} +{offset} {:?}", tp),
            Inst::Get { id, offset, tp } => write!(f, "get ${id} +{offset} {:?}", tp),
            Inst::LocalAddr { id, offset } => write!(f, "addr ${id} +{offset}"),
            Inst::Load { offset, tp } => write!(f, "load +{offset}"),
            Inst::Store { offset, tp } => write!(f, "store +{offset}"),
            Inst::CapOffset => write!(f, "capoffset"),
            Inst::Drop(tp) => write!(f, "drop"),
            Inst::Dup => write!(f, "dup"),
            Inst::Call(name) => write!(f, "call {name:?}"),
            Inst::MemCopy { size } => write!(f, "memcpy {size}"),
        }
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct Func {
    pub blocks: Vec<Block>,
    pub variables: Vec<u32>,
    pub sig: FuncSig,
}

#[derive(Debug, Clone, Copy)]
pub enum Type {
    Int,
    Bool,
    Ref,
    SRet,
}

impl Type {
    pub fn get_size(&self) -> i32 {
        match self {
            Type::Int => 8,
            Type::Bool => 1,
            Type::Ref => 8,
            Type::SRet => 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FuncSig {
    pub args: Vec<Type>,
    pub rets: Vec<Type>,
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

#[derive(Debug, Clone)]
pub struct Prog {
    pub funcs: HashMap<String, Func>,
    pub externs: HashMap<String, FuncSig>,
}

impl Display for Prog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (name, func) in &self.funcs {
            writeln!(f, "{name:?} ({}):", func.variables.len())?;
            writeln!(f, "{}", func)?;
        }
        Ok(())
    }
}
