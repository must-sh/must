use std::collections::HashMap;

use crate::{
    ast::Expr,
    bytecode::{Inst, Prog},
};

pub struct Builder {
    insts: Vec<Inst>,
    variable_map: HashMap<String, usize>,
    counter: usize,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            insts: vec![],
            variable_map: HashMap::new(),
            counter: 0,
        }
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
            Expr::Let(x, e1, e2) => {
                let id = self.new_var(x);
                self.lower(*e1);
                self.push_inst(Inst::Set(id));
                self.lower(*e2);
            }
            Expr::Var(x) => {
                let id = self.get_var(&x);
                self.push_inst(Inst::Get(id));
            }
        }
    }

    pub fn finish(self) -> Prog {
        Prog {
            insts: self.insts,
            variables: self.counter,
        }
    }

    fn new_var(&mut self, x: String) -> usize {
        let id = self.counter;
        self.variable_map.insert(x, id);
        self.counter += 1;
        id
    }

    fn get_var(&self, x: &str) -> usize {
        *self.variable_map.get(x).unwrap()
    }
}
