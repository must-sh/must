use std::fmt::Display;

use crate::bytecode;

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum Binop {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,

    // Comparision
    Eq,
    Lt,
    Gt,
    Le,
    Ge,
    NEq,

    // Boolean
    And,
    Or,
}
impl Binop {
    pub(crate) fn ret_tp(&self) -> bytecode::Type {
        todo!()
    }
}

impl Display for Binop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Binop::Add => "add",
            Binop::Sub => "sub",
            Binop::Mul => "mul",
            Binop::Div => "div",
            Binop::Mod => "mod",
            Binop::Eq => "eq",
            Binop::Lt => "lt",
            Binop::Gt => "gt",
            Binop::Le => "le",
            Binop::Ge => "ge",
            Binop::NEq => "neq",
            Binop::And => "and",
            Binop::Or => "or",
        };
        write!(f, "{}", str)
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum Unop {
    Not,
    Neg,
}
impl Unop {
    pub(crate) fn ret_tp(&self) -> bytecode::Type {
        todo!()
    }
}

impl Display for Unop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Unop::Not => "not",
            Unop::Neg => "neg",
        };
        write!(f, "{}", str)
    }
}
