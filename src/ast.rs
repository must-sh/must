#[derive(Debug)]
pub struct File {
    pub expr: Expr,
}

impl File {
    pub fn new(expr: Expr) -> Self {
        Self { expr }
    }
}

#[derive(Debug)]
pub enum Expr {
    Number(i32),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}
