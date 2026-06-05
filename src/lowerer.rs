use std::collections::HashMap;

use salsa::Database;

use crate::{
    ast::{self, ExprData, ExprId, Ident, PatternData, PatternId},
    bytecode::{self, Block, Func, FuncSig, Inst, Terminator},
    common::Binop,
    driver::type_check_func,
    resolve::{self, parse_type_expr},
    tp::{Type, TypeInfo},
};

pub enum Place {
    Local { id: usize, offset: i32 },
    Ref { offset: i32 },
}

impl Place {
    pub fn add_offset(self, x: i32) -> Self {
        match self {
            Place::Local { id, offset } => Place::Local {
                id,
                offset: offset + x,
            },
            Place::Ref { offset } => Place::Ref { offset: offset + x },
        }
    }
}

pub struct Builder<'a> {
    blocks: Vec<Block>,
    current_block: usize,
    variable_map: HashMap<Ident<'a>, usize>,
    variables: Vec<u32>,
    type_map: &'a HashMap<ExprId<'a>, Type>,
    type_defs: &'a HashMap<usize, TypeInfo<'a>>,
    db: &'a dyn Database,
    func: ast::FnDef<'a>,
}

impl<'a> Builder<'a> {
    pub fn new(db: &'a dyn Database, func: ast::FnDef<'a>) -> Self {
        let type_map = Box::new(type_check_func(db, func).type_map);
        let type_defs = Box::new(resolve::get_defs(db, func.sf(db)).type_map);

        Self {
            variable_map: HashMap::new(),
            variables: vec![],
            blocks: vec![Block::empty()],
            current_block: 0,
            type_map: Box::leak(type_map),
            type_defs: Box::leak(type_defs),
            db,
            func,
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
            ExprData::Unop(op, expr1) => {
                self.lower(expr1);
                self.push_inst(Inst::Unop(op));
            }
            ExprData::Let(pat, e1, e2) => {
                self.lower(e1);
                let tp = self.type_map.get(&e1).unwrap();
                self.lower_pat(pat, tp);
                self.lower(e2);
            }
            ExprData::Var(x) => {
                let tp = self.type_map.get(&e).unwrap();
                println!("{}, {:?}", x.text(self.db), tp);
                let layout = tp.layout(&self.type_defs);
                let id = self.get_var(x);
                self.load_from_place(Place::Local { id, offset: 0 }, layout);
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
                let layout = tp.layout(&self.type_defs);
                self.lower(e2);
                let place = self.lower_place(e1);
                self.store_to_place(place, layout);
            }
            ExprData::Deref(expr) => {
                let tp = self.type_map.get(&e).unwrap();
                let layout = tp.layout(self.type_defs);
                self.lower(expr);
                self.load_from_place(Place::Ref { offset: 0 }, layout);
            }
            ExprData::AddressOf(e) => match self.lower_place(e) {
                Place::Local { id, offset } => self.push_inst(Inst::LocalAddr { id, offset }),
                Place::Ref { offset } => {
                    self.push_inst(Inst::PushInt(offset as i64));
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
                let layout = tp.layout(self.type_defs);
                for tp in layout {
                    self.push_inst(Inst::Drop(tp));
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
                let offset = self.get_offset(tp_struct, ident);
                let tp_field = self.type_map.get(&e).unwrap();
                let layout = tp_field.layout(&self.type_defs);
                let place = self.lower_place(expr);
                self.load_from_place(place.add_offset(offset), layout);
            }
            ExprData::Array(exprs) => {
                for e in exprs {
                    self.lower(e);
                }
            }
            ExprData::Index(_, e2) => {
                let tp = self.type_map.get(&e).unwrap();
                let layout = tp.layout(&self.type_defs);
                match self.type_map.get(&e2).unwrap() {
                    Type::Int => {
                        let place = self.lower_place(e);
                        self.load_from_place(place, layout);
                    }
                    Type::Range => {
                        self.lower_place(e);
                        self.lower(e2);
                        self.push_inst(Inst::Binop(Binop::Sub));
                        self.push_inst(Inst::PushInt(-1));
                        self.push_inst(Inst::Binop(Binop::Mul));
                    }
                    Type::Bool
                    | Type::Fn(_)
                    | Type::Ptr(_, _)
                    | Type::Slice(_, _)
                    | Type::Tuple(_)
                    | Type::Var(_)
                    | Type::Array(_, _)
                    | Type::Error => panic!(),
                }
            }
            ExprData::Range(e1, e2) => {
                self.lower(e1);
                self.lower(e2);
            }
        }
    }

    pub fn load_from_place(&mut self, place: Place, layout: Vec<bytecode::Type>) {
        match place {
            Place::Local { id, offset } => {
                let mut i = 0;
                for tp in layout {
                    self.push_inst(Inst::Get {
                        id,
                        offset: offset + i,
                        tp,
                    });
                    i += tp.get_size();
                }
            }
            Place::Ref { offset } => {
                let id = self.new_tmp_var(8);
                self.push_inst(Inst::Set {
                    id,
                    offset: 0,
                    tp: bytecode::Type::Ref,
                });
                let mut i = 0;
                for tp in layout {
                    self.push_inst(Inst::Get {
                        id,
                        offset: 0,
                        tp: bytecode::Type::Ref,
                    });
                    self.push_inst(Inst::Load {
                        offset: offset + i,
                        tp,
                    });
                    i += tp.get_size();
                }
            }
        }
    }

    pub fn store_to_place(&mut self, place: Place, layout: Vec<bytecode::Type>) {
        let size: i32 = layout.iter().map(|tp| tp.get_size()).sum();
        let mut i = size;
        match place {
            Place::Local { id, offset } => {
                for tp in layout.into_iter().rev() {
                    i -= tp.get_size();
                    self.push_inst(Inst::Set {
                        id,
                        offset: offset + i,
                        tp,
                    });
                }
            }
            Place::Ref { offset } => {
                let id = self.new_tmp_var(8);
                self.push_inst(Inst::Set {
                    id,
                    offset: 0,
                    tp: bytecode::Type::Ref,
                });
                for tp in layout.into_iter().rev() {
                    i -= tp.get_size();
                    self.push_inst(Inst::Get {
                        id,
                        offset: 0,
                        tp: bytecode::Type::Ref,
                    });
                    self.push_inst(Inst::Store {
                        offset: offset + i,
                        tp,
                    });
                }
                // self.push_inst(Inst::Drop(bytecode::Type::Ref));
            }
        }
    }

    pub fn get_offset(&mut self, tp: &Type, field_name: Ident<'_>) -> i32 {
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
                8
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
            | ExprData::Unop(_, _)
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
            ExprData::Var(x) => Place::Local {
                id: self.get_var(x),
                offset: 0,
            },
            ExprData::Deref(e) => {
                self.lower(e);
                Place::Ref { offset: 0 }
            }
            ExprData::Seq(e1, e2) => {
                self.lower(e1);
                let tp = self.type_map.get(&e1).unwrap();
                let layout = tp.layout(self.type_defs);
                for tp in layout {
                    self.push_inst(Inst::Drop(tp));
                }
                self.lower_place(e2)
            }

            ExprData::Field(expr, ident) => {
                let tp = self.type_map.get(&expr).unwrap();
                let this_offset = self.get_offset(tp, ident);
                match self.lower_place(expr) {
                    Place::Local { id, offset } => Place::Local {
                        id,
                        offset: offset + this_offset,
                    },
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
                    self.push_inst(Inst::Drop(bytecode::Type::Int));
                    0
                } else {
                    match self.lower_place(e1) {
                        Place::Local { id, offset } => {
                            self.push_inst(Inst::LocalAddr { id, offset });
                            0
                        }
                        Place::Ref { offset } => offset,
                    }
                };
                self.lower(e2);
                if matches!(tp, Type::Range) {
                    self.push_inst(Inst::Drop(bytecode::Type::Int));
                }
                self.push_inst(Inst::PushInt(elem_size as i64));
                self.push_inst(Inst::Binop(Binop::Mul));
                self.push_inst(Inst::CapOffset);
                Place::Ref { offset }
            }
        }
    }

    pub fn lower_pat(&mut self, pat: PatternId<'a>, tp: &Type) {
        match pat.data(self.db) {
            PatternData::Wildcard => {
                let layout = tp.layout(self.type_defs);
                for tp in layout {
                    self.push_inst(Inst::Drop(tp));
                }
            }
            PatternData::Var(name, _) => {
                let size = tp.get_size(self.type_defs);
                let layout = tp.layout(self.type_defs);
                let id = self.new_var(name, size as u32);
                let mut offset = size;
                for tp in layout.into_iter().rev() {
                    offset -= tp.get_size();
                    self.push_inst(Inst::Set { id, offset, tp });
                }
            }
            PatternData::Tuple(pats) => {
                if let Type::Tuple(tps) = tp {
                    for (pat, tp) in pats.into_iter().zip(tps).rev() {
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

    pub fn compile(mut self) -> LoweringResult {
        let mut args = vec![];
        let mut rets = vec![];
        for (arg, tp) in self.func.args(self.db) {
            let tp = parse_type_expr(self.db, tp);
            let layout = tp.layout(self.type_defs);
            args.extend(layout);
            self.lower_pat(arg, &tp); // if its extern, we can lower but whatever, they will be freed anyways
        }
        if let Some(tp) = self.func.ret(self.db) {
            let tp = parse_type_expr(self.db, tp);
            let layout = tp.layout(self.type_defs);
            rets.extend(layout);
        }
        let sig = FuncSig { args, rets };
        let res = if let Some(body) = self.func.body(self.db) {
            self.lower(body);
            LoweringResult::Function(Func {
                blocks: self.blocks,
                variables: self.variables,
                sig,
            })
        } else {
            LoweringResult::Extern(sig)
        };

        unsafe {
            drop(Box::from_raw(
                self.type_defs as *const HashMap<usize, TypeInfo<'a>>
                    as *mut HashMap<usize, TypeInfo<'a>>,
            ));
            drop(Box::from_raw(
                self.type_map as *const HashMap<ExprId<'a>, Type> as *mut HashMap<ExprId<'a>, Type>,
            ));
        }
        res
    }

    pub fn new_var(&mut self, x: Ident<'a>, size: u32) -> usize {
        let id = self.variables.len();
        self.variable_map.insert(x, id);
        self.variables.push(size);
        id
    }

    pub fn new_tmp_var(&mut self, size: u32) -> usize {
        let id = self.variables.len();
        self.variables.push(size);
        id
    }

    pub fn get_var(&self, x: Ident<'a>) -> usize {
        *self.variable_map.get(&x).unwrap()
    }

    fn terminate_current_block(&mut self, term: Terminator) {
        self.blocks[self.current_block].terminator = term;
    }
}

#[derive(Debug, Clone)]
pub enum LoweringResult {
    Function(Func),
    Extern(FuncSig),
}
