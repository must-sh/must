use std::collections::HashMap;

use salsa::Database;

use crate::{
    ast::{ExprData, ExprId, Ident},
    bytecode::{Func, Inst},
};

pub struct Builder<'a> {
    insts: Vec<Inst>,
    variable_map: HashMap<Ident<'a>, usize>,
    counter: usize,
    db: &'a dyn Database,
}

impl<'a> Builder<'a> {
    pub fn new(db: &'a dyn Database) -> Self {
        Self {
            insts: vec![],
            variable_map: HashMap::new(),
            counter: 0,
            db,
        }
    }

    pub fn push_inst(&mut self, inst: Inst) {
        self.insts.push(inst);
    }

    pub fn lower(&mut self, e: ExprId<'a>) {
        match e.data(self.db) {
            ExprData::Number(n) => self.push_inst(Inst::Push(n)),
            ExprData::Binop(op, expr1, expr2) => {
                self.lower(expr1);
                self.lower(expr2);
                self.push_inst(Inst::Binop(op));
            }
            ExprData::Let(x, e1, e2) => {
                let id = self.new_var(x);
                self.lower(e1);
                self.push_inst(Inst::Set(id));
                self.lower(e2);
            }
            ExprData::Var(x) => {
                let id = self.get_var(x);
                self.push_inst(Inst::Get(id));
            }
            ExprData::FnCall(name, args) => {
                let n = args.len();
                for arg in args {
                    self.lower(arg);
                }
                self.push_inst(Inst::Call(name.text(self.db).clone(), n));
            }
            ExprData::Error => panic!("no errors allowed here"),
        }
    }

    pub fn finish(self) -> Func {
        Func {
            insts: self.insts,
            variables: self.counter,
        }
    }

    pub fn new_var(&mut self, x: Ident<'a>) -> usize {
        let id = self.counter;
        self.variable_map.insert(x, id);
        self.counter += 1;
        id
    }

    pub fn get_var(&self, x: Ident<'a>) -> usize {
        *self.variable_map.get(&x).unwrap()
    }
}
