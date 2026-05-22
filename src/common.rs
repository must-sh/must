#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,

    Eq,
}
