use std::{collections::HashMap, fmt::Display};

use crate::common::{Binop, Unop};

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Inst {
    PushInt(i64),
    PushBool(bool),
    Binop(Binop),
    Unop(Unop),

    Set { id: usize, offset: u32 },
    Get { id: usize, offset: u32, tp: Type },
    LocalAddr { id: usize, offset: u32 },

    // PTR ON TOP, VALUE SECOND
    Load { offset: u32, tp: Type },
    Store { offset: u32 },

    // SRC ON TOP, DST SECOND
    MemCopy { size: usize },

    // REF, INT -> REF + INT
    CapOffset,

    Drop,
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
            Inst::Set { id, offset } => write!(f, "set ${id} +{offset}"),
            Inst::Get { id, offset, tp } => write!(f, "get ${id} +{offset} {:?}", tp),
            Inst::LocalAddr { id, offset } => write!(f, "addr ${id} +{offset}"),
            Inst::Load { offset, tp } => write!(f, "load +{offset} {:?}", tp),
            Inst::Store { offset } => write!(f, "store +{offset}"),
            Inst::CapOffset => write!(f, "capoffset"),
            Inst::Drop => write!(f, "drop"),
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

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum Type {
    Int64,
    Bool,
    Ptr,
}

impl Type {
    pub fn as_layout(self) -> Layout {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub size: usize,
    pub align: u32,
    pub fields: Fields,
}

#[derive(Debug, Clone)]
pub enum Abi {
    Unit,
    Scalar(Type),
    ScalarPair(Type, Type),
    Struct,
}

#[derive(Debug, Clone)]
pub enum Fields {
    Primitive(Type),
    Array { stride: Box<Layout>, count: usize },
    Struct { fields: Vec<(u32, Layout)> },
}

impl Layout {
    pub fn size(&self) -> usize {
        self.size
    }

    pub fn align(&self) -> u32 {
        self.align
    }

    pub fn int64() -> Self {
        Self {
            size: 8,
            align: 8,
            fields: Fields::Primitive(Type::Int64),
        }
    }

    pub fn bool() -> Self {
        Self {
            size: 1,
            align: 1,
            fields: Fields::Primitive(Type::Bool),
        }
    }

    pub fn ptr() -> Self {
        Self {
            size: 8,
            align: 8,
            fields: Fields::Primitive(Type::Ptr),
        }
    }

    pub fn strct(tps: &[Layout]) -> Self {
        let fields = tps
            .iter()
            .scan(0, |off, tp| {
                let res = Some((*off, tp.clone()));
                *off += tp.size() as u32;
                res
            })
            .collect();
        Self {
            size: tps.iter().map(|lt| lt.size()).sum(),
            align: tps.iter().map(|lt| lt.align()).max().unwrap_or(0),
            fields: Fields::Struct { fields },
        }
    }

    pub fn array(size: usize, lt: Layout) -> Self {
        Self {
            size: lt.size * size,
            align: lt.align,
            fields: Fields::Array {
                stride: Box::new(lt),
                count: size,
            },
        }
    }

    pub fn abi(&self) -> Abi {
        match &self.fields {
            Fields::Primitive(tp) => Abi::Scalar(*tp),
            Fields::Array { stride, count } => Abi::Struct,
            Fields::Struct { fields } => match self.primitives()[..] {
                [] => Abi::Unit,
                [tp] => Abi::Scalar(tp),
                [tp1, tp2] => Abi::ScalarPair(tp1, tp2),
                _ => Abi::Struct,
            },
        }
    }

    pub fn primitives(&self) -> Vec<Type> {
        match &self.fields {
            Fields::Primitive(tp) => vec![*tp],
            Fields::Array { stride, count } => stride.primitives().repeat(*count),
            Fields::Struct { fields } => {
                fields.iter().flat_map(|(_, lt)| lt.primitives()).collect()
            }
        }
    }

    pub(crate) fn unit() -> Layout {
        Self {
            size: 0,
            align: 0,
            fields: Fields::Struct { fields: vec![] },
        }
    }
}

impl Type {
    pub fn size(&self) -> u32 {
        match self {
            Type::Int64 => 8,
            Type::Bool => 1,
            Type::Ptr => 8,
        }
    }

    pub fn align(&self) -> u32 {
        match self {
            Type::Int64 => 8,
            Type::Bool => 1,
            Type::Ptr => 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FuncSig {
    pub args: Vec<Layout>,
    pub rets: Vec<Layout>,
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
