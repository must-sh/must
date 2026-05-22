use std::collections::HashMap;

use salsa::Database;

use crate::{
    ast::{ExprData, ExprId, Ident},
    bytecode::{Block, Func, Inst, Terminator},
};

pub struct Builder<'a> {
    blocks: Vec<Block>,
    current_block: usize,
    variable_map: HashMap<Ident<'a>, usize>,
    counter: usize,
    db: &'a dyn Database,
}

impl<'a> Builder<'a> {
    pub fn new(db: &'a dyn Database) -> Self {
        Self {
            variable_map: HashMap::new(),
            counter: 0,
            blocks: vec![Block::empty()],
            current_block: 0,
            db,
        }
    }

    pub fn push_inst(&mut self, inst: Inst) {
        self.blocks[self.current_block].insts.push(inst);
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
            ExprData::If(cond, th, el) => {
                let th_block = self.new_block();
                let el_block = self.new_block();
                let next_block = self.new_block();

                self.lower(cond);
                self.terminate_current_block(Terminator::Br(th_block, el_block));

                self.switch_to_block(th_block);
                self.lower(th);
                self.terminate_current_block(Terminator::Jmp(next_block));

                self.switch_to_block(el_block);
                self.lower(el);
                self.terminate_current_block(Terminator::Jmp(next_block));

                self.switch_to_block(next_block);
            }
            ExprData::While(cond, body) => {
                let cond_block = self.new_block();
                let body_block = self.new_block();
                let next_block = self.new_block();

                self.terminate_current_block(Terminator::Jmp(cond_block));

                self.switch_to_block(cond_block);
                self.lower(cond);
                self.terminate_current_block(Terminator::Br(body_block, next_block));

                self.switch_to_block(body_block);
                self.lower(body);
                self.terminate_current_block(Terminator::Jmp(cond_block));

                self.switch_to_block(next_block);
            }
        }
    }

    pub fn new_block(&mut self) -> usize {
        let id = self.blocks.len();
        self.blocks.push(Block::empty());
        id
    }

    pub fn switch_to_block(&mut self, id: usize) {
        self.current_block = id;
    }

    pub fn finish(self) -> Func {
        Func {
            blocks: self.blocks,
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

    fn terminate_current_block(&mut self, term: Terminator) {
        self.blocks[self.current_block].terminator = term;
    }
}
