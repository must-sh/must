use std::collections::HashMap;

use crate::common::Op;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    Push(i32),
    Binop(Op),

    Set(usize),
    Get(usize),

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

// Block0:
//  push 42
//  push 13
//  add
//  jmp Block1
// Block1:
//  ...
//  eq
//  br Block2 Block3
