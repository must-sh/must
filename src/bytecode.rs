use std::collections::HashMap;

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Func {
    pub insts: Vec<Inst>,
    pub variables: usize,
}

#[derive(Debug)]
pub struct Prog {
    pub funcs: HashMap<String, Func>,
}
