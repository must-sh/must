use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    Push(i32),
    Add,
    Sub,
    Mul,
    Div,

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
