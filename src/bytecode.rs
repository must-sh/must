use std::collections::HashMap;

use crate::common::Op;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    PushInt(i32),
    PushBool(bool),
    Binop(Op),

    Set(usize),
    Get(usize),
    LocalAddr(usize),

    Load(usize),
    Store(usize),

    Drop,

    Call(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub insts: Vec<Inst>,
    pub terminator: Terminator,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Func {
    pub blocks: Vec<Block>,
    pub variables: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prog {
    pub funcs: HashMap<String, Func>,
}
