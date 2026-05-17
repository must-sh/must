#[derive(Debug)]
pub enum Inst {
    Push(i32),
    Add,
    Sub,
    Mul,
    Div,
}
