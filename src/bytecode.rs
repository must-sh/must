use std::collections::HashMap;

use crate::common::Op;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    Push(i32),
    Binop(Op),

    Set(usize),
    Get(usize),

    Call(String, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Func {
    pub insts: Vec<Inst>,
    pub variables: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prog {
    pub funcs: HashMap<String, Func>,
}
