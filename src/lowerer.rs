use std::collections::HashMap;

use salsa::Database;

use crate::{
    ast::{ExprData, ExprId, Ident, PatternData, PatternId},
    bytecode::{Block, Func, Inst, Terminator},
    common::Op,
    tp::{Type, TypeInfo},
};

pub enum Place {
    Local(usize),
    Ref { offset: usize },
}

pub struct Builder<'a> {
    blocks: Vec<Block>,
    current_block: usize,
    variable_map: HashMap<Ident<'a>, usize>,
    counter: usize,
    type_map: &'a HashMap<ExprId<'a>, Type>,
    type_defs: &'a HashMap<usize, TypeInfo<'a>>,
    db: &'a dyn Database,
}

impl<'a> Builder<'a> {
    pub fn new(
        db: &'a dyn Database,
        type_map: &'a HashMap<ExprId<'a>, Type>,
        type_defs: &'a HashMap<usize, TypeInfo<'a>>,
    ) -> Self {
        Self {
            variable_map: HashMap::new(),
            counter: 0,
            blocks: vec![Block::empty()],
            current_block: 0,
            type_map,
            type_defs,
            db,
        }
    }

    pub fn push_inst(&mut self, inst: Inst) {
        self.blocks[self.current_block].insts.push(inst);
    }

    pub fn lower(&mut self, e: ExprId<'a>) {
        match e.data(self.db) {
            ExprData::Number(n) => self.push_inst(Inst::PushInt(n)),
            ExprData::Binop(op, expr1, expr2) => {
                self.lower(expr1);
                self.lower(expr2);
                self.push_inst(Inst::Binop(op));
            }
            ExprData::Let(pat, e1, e2) => {
                self.lower(e1);
                let tp = self.type_map.get(&e1).unwrap();
                self.lower_pat(pat, tp);
                self.lower(e2);
            }
            ExprData::Var(x) => {
                let tp = self.type_map.get(&e).unwrap();
                let size = tp.get_size(self.type_defs);
                let id = self.get_var(x);
                self.push_inst(Inst::Get { id, size });
            }
            ExprData::FnCall(name, args) => {
                for arg in args.into_iter().rev() {
                    self.lower(arg);
                }
                self.push_inst(Inst::Call(name.text(self.db).clone()));
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
                if let Some(el) = el {
                    self.lower(el);
                }
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
            ExprData::Assign(e1, e2) => {
                let tp = self.type_map.get(&e1).unwrap();
                let size = tp.get_size(self.type_defs);
                self.lower(e2);
                let id = self.lower_place(e1);
                match id {
                    Place::Local(id) => self.push_inst(Inst::Set { id, size }),
                    Place::Ref { offset } => self.push_inst(Inst::Store { offset, size }),
                }
            }
            ExprData::Deref(expr) => {
                let tp = self.type_map.get(&e).unwrap();
                let size = tp.get_size(self.type_defs);
                self.lower(expr);
                self.push_inst(Inst::Load { offset: 0, size });
            }
            ExprData::AddressOf(e) => match self.lower_place(e) {
                Place::Local(id) => self.push_inst(Inst::LocalAddr(id)),
                Place::Ref { offset } => {
                    self.push_inst(Inst::PushInt(offset as i32));
                    self.push_inst(Inst::CapOffset)
                }
            },
            ExprData::Tuple(exprs) => {
                for e in exprs {
                    self.lower(e);
                }
            }
            ExprData::Bool(b) => self.push_inst(Inst::PushBool(b)),
            ExprData::Seq(e1, e2) => {
                self.lower(e1);
                let tp = self.type_map.get(&e1).unwrap();
                let size = tp.get_size(self.type_defs);
                for _ in 0..size {
                    self.push_inst(Inst::Drop);
                }
                self.lower(e2);
            }
            ExprData::Struct(ident, exprs) => {
                let info = self.type_defs.get(&ident.get_id()).unwrap();
                let mut fields = info
                    .fields
                    .iter()
                    .map(|(name, (id, _))| (id, name))
                    .collect::<Vec<_>>();
                fields.sort_by_key(|(id, _)| **id);
                let mut exprs_map: HashMap<_, _> = exprs.into_iter().collect();
                for (_, name) in fields {
                    self.lower(exprs_map.remove(&name).unwrap())
                }
            }
            ExprData::Field(expr, ident) => {
                let tp_struct = self.type_map.get(&expr).unwrap();
                let this_offset = self.get_offset(tp_struct, ident);
                let tp_field = self.type_map.get(&e).unwrap();
                let size = tp_field.get_size(self.type_defs);
                match self.lower_place(expr) {
                    Place::Local(place) => self.push_inst(Inst::Get {
                        id: place + this_offset,
                        size,
                    }),
                    Place::Ref { offset } => self.push_inst(Inst::Load {
                        offset: offset + this_offset,
                        size,
                    }),
                }
            }
            ExprData::Array(exprs) => {
                for e in exprs {
                    self.lower(e);
                }
            }
            ExprData::Index(_, e2) => {
                let tp = self.type_map.get(&e).unwrap();
                let size = tp.get_size(self.type_defs);
                match self.type_map.get(&e2).unwrap() {
                    Type::Error => todo!(),
                    Type::Int => {
                        if let Place::Ref { offset } = self.lower_place(e) {
                            self.push_inst(Inst::Load { offset, size });
                        } else {
                            panic!()
                        }
                    }
                    Type::Bool => todo!(),
                    Type::Range => {
                        self.lower_place(e);
                        self.lower(e2);
                        self.push_inst(Inst::Binop(Op::Sub));
                        self.push_inst(Inst::PushInt(-1));
                        self.push_inst(Inst::Binop(Op::Mul));
                    }
                    Type::Fn(fn_sig) => todo!(),
                    Type::Ptr(_, _) => todo!(),
                    Type::Slice(_, _) => (),
                    Type::Tuple(items) => todo!(),
                    Type::Var(_) => todo!(),
                    Type::Array(_, _) => todo!(),
                }
            }
            ExprData::Range(e1, e2) => {
                self.lower(e1);
                self.lower(e2);
            }
        }
    }

    pub fn get_offset(&mut self, tp: &Type, field_name: Ident<'_>) -> usize {
        match tp {
            Type::Error
            | Type::Int
            | Type::Bool
            | Type::Range
            | Type::Fn(_)
            | Type::Ptr(_, _)
            | Type::Tuple(_)
            | Type::Array(_, _) => panic!(),
            Type::Slice(_, _) => {
                assert_eq!(field_name.text(self.db), "len");
                1
            }
            Type::Var(id) => {
                let fields = &self.type_defs.get(&id).unwrap().fields;
                let field_id = fields.get(&field_name).unwrap().0;
                fields
                    .iter()
                    .filter(|(_, (id, _))| *id < field_id)
                    .map(|(_, (_, tp))| tp.get_size(self.type_defs))
                    .sum()
            }
        }
    }

    pub fn lower_place(&mut self, e: ExprId<'a>) -> Place {
        match e.data(self.db) {
            ExprData::Binop(_, _, _)
            | ExprData::Error
            | ExprData::FnCall(_, _)
            | ExprData::While(_, _)
            | ExprData::Assign(_, _)
            | ExprData::AddressOf(_)
            | ExprData::Bool(_)
            | ExprData::Tuple(_)
            | ExprData::If(_, _, _)
            | ExprData::Number(_)
            | ExprData::Array(_)
            | ExprData::Range(_, _)
            | ExprData::Struct(_, _) => todo!(),
            ExprData::Let(pat, e1, e2) => {
                self.lower(e1);
                let tp = self.type_map.get(&e1).unwrap();
                self.lower_pat(pat, tp);
                self.lower_place(e2)
            }
            ExprData::Var(x) => Place::Local(self.get_var(x)),
            ExprData::Deref(e) => {
                self.lower(e);
                Place::Ref { offset: 0 }
            }
            ExprData::Seq(e1, e2) => {
                self.lower(e1);
                let tp = self.type_map.get(&e1).unwrap();
                let size = tp.get_size(self.type_defs);
                for _ in 0..size {
                    self.push_inst(Inst::Drop);
                }
                self.lower_place(e2)
            }

            ExprData::Field(expr, ident) => {
                let tp = self.type_map.get(&expr).unwrap();
                let this_offset = self.get_offset(tp, ident);
                match self.lower_place(expr) {
                    Place::Local(id) => Place::Local(id + this_offset),
                    Place::Ref { offset } => Place::Ref {
                        offset: offset + this_offset,
                    },
                }
            }
            ExprData::Index(e1, e2) => {
                let tp = self.type_map.get(&e2).unwrap();
                let e1_tp = self.type_map.get(&e1).unwrap();
                let elem_size = {
                    match e1_tp {
                        Type::Error
                        | Type::Int
                        | Type::Bool
                        | Type::Range
                        | Type::Fn(_)
                        | Type::Ptr(_, _)
                        | Type::Tuple(_)
                        | Type::Var(_) => panic!(),
                        Type::Slice(tp, _) | Type::Array(_, tp) => tp.get_size(self.type_defs),
                    }
                };
                let offset = if matches!(e1_tp, Type::Slice(_, _)) {
                    self.lower(e1);
                    self.push_inst(Inst::Drop);
                    0
                } else {
                    match self.lower_place(e1) {
                        Place::Local(id) => {
                            self.push_inst(Inst::LocalAddr(id));
                            0
                        }
                        Place::Ref { offset } => offset,
                    }
                };
                self.lower(e2);
                if matches!(tp, Type::Range) {
                    self.push_inst(Inst::Drop);
                }
                self.push_inst(Inst::PushInt(elem_size as i32));
                self.push_inst(Inst::Binop(Op::Mul));
                self.push_inst(Inst::CapOffset);
                Place::Ref { offset }
            }
        }
    }

    pub fn lower_pat(&mut self, pat: PatternId<'a>, tp: &Type) {
        match pat.data(self.db) {
            PatternData::Wildcard => {
                let size = tp.get_size(self.type_defs);
                for _ in 0..size {
                    self.push_inst(Inst::Drop);
                }
            }
            PatternData::Var(name, _) => {
                let size = tp.get_size(self.type_defs);
                let id = self.new_var(name, size);
                self.push_inst(Inst::Set { id, size });
            }
            PatternData::Tuple(pats) => {
                if let Type::Tuple(tps) = tp {
                    for (pat, tp) in pats.into_iter().zip(tps) {
                        self.lower_pat(pat, tp);
                    }
                } else {
                    panic!()
                }
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

    pub fn new_var(&mut self, x: Ident<'a>, size: usize) -> usize {
        let id = self.counter;
        self.variable_map.insert(x, id);
        self.counter += size;
        id
    }

    pub fn get_var(&self, x: Ident<'a>) -> usize {
        *self.variable_map.get(&x).unwrap()
    }

    fn terminate_current_block(&mut self, term: Terminator) {
        self.blocks[self.current_block].terminator = term;
    }
}
