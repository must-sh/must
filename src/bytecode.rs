#[derive(Debug)]
pub enum Inst {
    Push(i32),
    Add,
    Sub,
    Mul,
    Div,

    Set(usize),
    Get(usize),
}

#[derive(Debug)]
pub struct Prog {
    pub insts: Vec<Inst>,
    pub variables: usize,
}
