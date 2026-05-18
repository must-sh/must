#[derive(Debug)]
pub struct File {
    pub defs: Vec<FnDef>,
}

impl File {
    pub fn new(defs: Vec<FnDef>) -> Self {
        Self { defs }
    }
}

#[derive(Debug)]
pub struct FnDef {
    pub name: String,
    pub args: Vec<String>,
    pub body: Expr,
}

#[derive(Debug)]
pub enum Expr {
    Number(i32),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Let(String, Box<Expr>, Box<Expr>),
    Var(String),
    FnCall(String, Vec<Expr>),
}
