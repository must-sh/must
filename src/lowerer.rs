use crate::{ast::Expr, bytecode::Inst};

pub struct Builder {
    insts: Vec<Inst>,
}

impl Builder {
    pub fn new() -> Self {
        Self { insts: vec![] }
    }

    pub fn push_inst(&mut self, inst: Inst) {
        self.insts.push(inst);
    }

    pub fn lower(&mut self, e: Expr) {
        match e {
            Expr::Number(n) => self.push_inst(Inst::Push(n)),
            Expr::Add(expr1, expr2) => {
                self.lower(*expr1);
                self.lower(*expr2);
                self.push_inst(Inst::Add);
            }
            Expr::Sub(expr1, expr2) => {
                self.lower(*expr1);
                self.lower(*expr2);
                self.push_inst(Inst::Sub);
            }
            Expr::Mul(expr1, expr2) => {
                self.lower(*expr1);
                self.lower(*expr2);
                self.push_inst(Inst::Mul);
            }
            Expr::Div(expr1, expr2) => {
                self.lower(*expr1);
                self.lower(*expr2);
                self.push_inst(Inst::Div);
            }
        }
    }

    pub fn finish(self) -> Vec<Inst> {
        self.insts
    }
}
