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

#[derive(Debug, Clone, Copy)]
pub enum Place {
    Stack,
    Local { id: usize, offset: u32 },
    Ref { offset: u32 },
}

impl Place {
    /// Add offset to this place.
    ///
    /// Panics if place is Stack.
    pub fn add_offset(self, x: u32) -> Self {
        match self {
            Place::Local { id, offset } => Place::Local {
                id,
                offset: offset + x,
            },
            Place::Ref { offset } => Place::Ref { offset: offset + x },
            Place::Stack => panic!(),
        }
    }

    /// Emits load instructions from self to the stack.
    pub fn load(self, b: &mut Builder, layout: &bytecode::Layout) {
        match self {
            Place::Stack => (),
            Place::Local { id, offset } => match &layout.abi() {
                bytecode::Abi::Unit => todo!(),
                bytecode::Abi::Scalar(tp) => {
                    b.push_inst(Inst::Get {
                        id,
                        offset,
                        tp: *tp,
                    });
                }
                bytecode::Abi::ScalarPair(tp1, tp2) => {
                    b.push_inst(Inst::Get {
                        id,
                        offset: offset + 8,
                        tp: *tp1,
                    });
                    b.push_inst(Inst::Get {
                        id,
                        offset,
                        tp: *tp2,
                    });
                }
                bytecode::Abi::Struct => {
                    let id = b.new_tmp_var(8);
                    b.push_inst(Inst::Set { id, offset: 0 });

                    b.push_inst(Inst::LocalAddr { id, offset });
                    b.push_inst(Inst::Get {
                        id,
                        offset: 0,
                        tp: bytecode::Type::Ptr,
                    });

                    b.push_inst(Inst::MemCopy {
                        size: layout.size(),
                    });
                }
            },
            Place::Ref { offset } => match &layout.abi() {
                bytecode::Abi::Unit => todo!(),
                bytecode::Abi::Scalar(tp) => {
                    b.push_inst(Inst::Load { offset, tp: *tp });
                }
                bytecode::Abi::ScalarPair(tp1, tp2) => {
                    let id = b.new_tmp_var(8);
                    b.push_inst(Inst::Set { id, offset: 0 });

                    b.push_inst(Inst::Get {
                        id,
                        offset: 0,
                        tp: bytecode::Type::Ptr,
                    });
                    b.push_inst(Inst::Load {
                        offset: offset + 8,
                        tp: *tp1,
                    });

                    b.push_inst(Inst::Get {
                        id,
                        offset: 0,
                        tp: bytecode::Type::Ptr,
                    });
                    b.push_inst(Inst::Load { offset, tp: *tp2 });
                }
                bytecode::Abi::Struct => {
                    b.push_inst(Inst::PushInt(offset as i64));
                    b.push_inst(Inst::CapOffset);

                    let id = b.new_tmp_var(8);
                    b.push_inst(Inst::Set { id, offset: 0 });

                    b.push_inst(Inst::LocalAddr { id, offset });
                    b.push_inst(Inst::Get {
                        id,
                        offset: 0,
                        tp: bytecode::Type::Ptr,
                    });

                    b.push_inst(Inst::MemCopy {
                        size: layout.size(),
                    });
                }
            },
        }
    }

    pub fn store(&self, b: &mut Builder) {}
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

    pub fn get_tp(&self, e: ExprId<'a>) -> &Type {
        self.type_map.get(&e).unwrap()
    }

    pub fn get_layout(&self, e: ExprId<'a>) -> bytecode::Layout {
        self.get_tp(e).layout(self.type_defs)
    }

    pub fn lower(&mut self, e: ExprId<'a>) -> Place {
        match e.data(self.db) {
            ExprData::Number(n) => {
                self.push_inst(Inst::PushInt(n));
                Place::Stack
            }
            ExprData::Binop(op, e1, e2) => {
                let layout = &self.get_layout(e1);
                self.lower(e1).load(self, layout);
                self.lower(e2).load(self, layout);
                self.push_inst(Inst::Binop(op));
                Place::Stack
            }
            ExprData::Unop(op, e1) => {
                let layout = &self.get_layout(e1);
                self.lower(e1).load(self, layout);
                self.push_inst(Inst::Unop(op));
                Place::Stack
            }
            ExprData::Let(pat, e1, e2) => {
                let place = self.lower(e1);
                let tp = self.type_map.get(&e1).unwrap();
                self.lower_pat(pat, tp, &place);
                self.lower(e2)
            }
            ExprData::Var(x) => Place::Local {
                id: self.get_var(x),
                offset: 0,
            },
            ExprData::FnCall(name, args) => {
                for arg in args.into_iter().rev() {
                    let layout = &self.get_layout(arg);
                    self.lower(arg).load(self, layout);
                }
                self.push_inst(Inst::Call(name.text(self.db).clone()));
                Place::Stack
            }
            ExprData::Error => panic!("no errors allowed here"),
            ExprData::If(cond, th, el) => {
                let layout = &self.get_layout(e);

                let th_block = self.new_block();
                let el_block = self.new_block();
                let next_block = self.new_block();

                self.lower(cond).load(self, &bytecode::Layout::bool());
                self.terminate_current_block(Terminator::Br(th_block, el_block));

                self.switch_to_block(th_block);
                self.lower(th).load(self, layout);
                self.terminate_current_block(Terminator::Jmp(next_block));

                self.switch_to_block(el_block);
                if let Some(el) = el {
                    self.lower(el).load(self, layout);
                }
                self.terminate_current_block(Terminator::Jmp(next_block));

                self.switch_to_block(next_block);
                Place::Stack
            }
            ExprData::While(cond, body) => {
                let layout = &self.get_layout(e);

                let cond_block = self.new_block();
                let body_block = self.new_block();
                let next_block = self.new_block();

                self.terminate_current_block(Terminator::Jmp(cond_block));

                self.switch_to_block(cond_block);
                self.lower(cond).load(self, &bytecode::Layout::bool());
                self.terminate_current_block(Terminator::Br(body_block, next_block));

                self.switch_to_block(body_block);
                self.lower(body).load(self, layout);
                self.terminate_current_block(Terminator::Jmp(cond_block));

                self.switch_to_block(next_block);
                Place::Stack
            }
            ExprData::Assign(e1, e2) => {
                let tp = self.type_map.get(&e1).unwrap();
                let layout = tp.layout(&self.type_defs);
                self.lower(e2).load(self, &layout);
                let place = self.lower(e1);
                self.store_to_place(place, &layout);
                Place::Stack
            }
            ExprData::Deref(expr) => {
                self.lower(expr).load(self, &bytecode::Layout::ptr());
                Place::Ref { offset: 0 }
            }
            ExprData::AddressOf(e) => {
                match self.lower(e) {
                    Place::Local { id, offset } => self.push_inst(Inst::LocalAddr { id, offset }),
                    Place::Ref { offset } => {
                        self.push_inst(Inst::PushInt(offset as i64));
                        self.push_inst(Inst::CapOffset)
                    }
                    Place::Stack => panic!(),
                };
                Place::Stack
            }
            ExprData::Tuple(exprs) => {
                for e in exprs {
                    self.lower(e);
                }
                Place::Stack
            }
            ExprData::Bool(b) => {
                self.push_inst(Inst::PushBool(b));
                Place::Stack
            }
            ExprData::Seq(e1, e2) => {
                self.lower(e1);
                let tp = self.type_map.get(&e1).unwrap();
                let layout = tp.layout(self.type_defs);
                for _ in 0..layout.primitives().len() {
                    self.push_inst(Inst::Drop);
                }
                self.lower(e2)
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
                    self.lower(exprs_map.remove(&name).unwrap());
                }
                Place::Stack
            }
            ExprData::Field(expr, ident) => {
                let tp_struct = self.type_map.get(&expr).unwrap();
                let offset = self.get_offset(tp_struct, ident);
                let place = self.lower(expr);
                place.add_offset(offset)
            }
            ExprData::Array(exprs) => {
                for e in exprs {
                    self.lower(e);
                }
                Place::Stack
            }
            ExprData::Index(_, e2) => {
                let tp = self.type_map.get(&e).unwrap();
                let layout = tp.layout(&self.type_defs);
                match self.type_map.get(&e2).unwrap() {
                    Type::Int => self.lower(e),
                    Type::Range => {
                        self.lower(e);
                        self.lower(e2);
                        self.push_inst(Inst::Binop(Binop::Sub));
                        self.push_inst(Inst::PushInt(-1));
                        self.push_inst(Inst::Binop(Binop::Mul));
                        Place::Stack
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
                Place::Stack
            }
        }
    }

    pub fn store_to_place(&mut self, place: Place, layout: &bytecode::Layout) {
        match place {
            Place::Local { id, offset } => match &layout.abi() {
                bytecode::Abi::Unit => todo!(),
                bytecode::Abi::Scalar(_) => {
                    self.push_inst(Inst::Set { id, offset });
                }
                bytecode::Abi::ScalarPair(_, _) => {
                    self.push_inst(Inst::Set {
                        id,
                        offset: offset + 8,
                    });
                    self.push_inst(Inst::Set { id, offset });
                }
                bytecode::Abi::Struct => {
                    self.push_inst(Inst::LocalAddr { id, offset });
                    self.push_inst(Inst::MemCopy {
                        size: layout.size(),
                    });
                }
            },
            Place::Ref { offset } => match &layout.abi() {
                bytecode::Abi::Unit => todo!(),
                bytecode::Abi::Scalar(_) => {
                    self.push_inst(Inst::Store { offset });
                }
                bytecode::Abi::ScalarPair(_, _) => {
                    let id = self.new_tmp_var(8);
                    self.push_inst(Inst::Set { id, offset: 0 });

                    self.push_inst(Inst::Get {
                        id,
                        offset: 0,
                        tp: bytecode::Type::Ptr,
                    });
                    self.push_inst(Inst::Store { offset: offset + 8 });

                    self.push_inst(Inst::Get {
                        id,
                        offset: 0,
                        tp: bytecode::Type::Ptr,
                    });
                    self.push_inst(Inst::Store { offset });
                }
                bytecode::Abi::Struct => {
                    self.push_inst(Inst::PushInt(offset as i64));
                    self.push_inst(Inst::CapOffset);
                    self.push_inst(Inst::MemCopy {
                        size: layout.size(),
                    });
                }
            },
            Place::Stack => todo!(),
        }
    }

    pub fn get_offset(&mut self, tp: &Type, field_name: Ident<'_>) -> u32 {
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

    // pub fn lower_place(&mut self, e: ExprId<'a>) -> Place {

    //         ExprData::Field(expr, ident) => {
    //             let tp = self.type_map.get(&expr).unwrap();
    //             let this_offset = self.get_offset(tp, ident);
    //             match self.lower_place(expr) {
    //                 Place::Local { id, offset } => Place::Local {
    //                     id,
    //                     offset: offset + this_offset,
    //                 },
    //                 Place::Ref { offset } => Place::Ref {
    //                     offset: offset + this_offset,
    //                 },
    //             }
    //         }
    //         ExprData::Index(e1, e2) => {
    //             let tp = self.type_map.get(&e2).unwrap();
    //             let e1_tp = self.type_map.get(&e1).unwrap();
    //             let elem_size = {
    //                 match e1_tp {
    //                     Type::Error
    //                     | Type::Int
    //                     | Type::Bool
    //                     | Type::Range
    //                     | Type::Fn(_)
    //                     | Type::Ptr(_, _)
    //                     | Type::Tuple(_)
    //                     | Type::Var(_) => panic!(),
    //                     Type::Slice(tp, _) | Type::Array(_, tp) => tp.get_size(self.type_defs),
    //                 }
    //             };
    //             let offset = if matches!(e1_tp, Type::Slice(_, _)) {
    //                 self.lower(e1);
    //                 self.push_inst(Inst::Drop);
    //                 0
    //             } else {
    //                 match self.lower_place(e1) {
    //                     Place::Local { id, offset } => {
    //                         self.push_inst(Inst::LocalAddr { id, offset });
    //                         0
    //                     }
    //                     Place::Ref { offset } => offset,
    //                 }
    //             };
    //             self.lower(e2);
    //             if matches!(tp, Type::Range) {
    //                 self.push_inst(Inst::Drop);
    //             }
    //             self.push_inst(Inst::PushInt(elem_size as i64));
    //             self.push_inst(Inst::Binop(Binop::Mul));
    //             self.push_inst(Inst::CapOffset);
    //             Place::Ref { offset }
    //         }
    //     }
    // }

    pub fn lower_pat(&mut self, pat: PatternId<'a>, tp: &Type, place: &Place) {
        match pat.data(self.db) {
            PatternData::Wildcard => {
                let layout = tp.layout(self.type_defs);
                for _ in 0..layout.primitives().len() {
                    self.push_inst(Inst::Drop);
                }
            }
            PatternData::Var(name, _) => {
                let size = tp.get_size(self.type_defs);
                let layout = tp.layout(self.type_defs);
                let id = self.new_var(name, size as u32);
                place.load(self, &layout);
                self.store_to_place(Place::Local { id, offset: 0 }, &layout);
            }
            PatternData::Tuple(pats) => {
                if let Type::Tuple(tps) = tp {
                    for (pat, tp) in pats.into_iter().zip(tps).rev() {
                        self.lower_pat(pat, tp, place);
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
            args.push(layout);
            self.lower_pat(arg, &tp, &Place::Stack); // if its extern, we can lower but whatever, they will be freed anyways
        }
        let layout = if let Some(tp) = self.func.ret(self.db) {
            let tp = parse_type_expr(self.db, tp);
            tp.layout(self.type_defs)
        } else {
            bytecode::Layout::unit()
        };
        rets.push(layout.clone());
        let sig = FuncSig { args, rets };
        let res = if let Some(body) = self.func.body(self.db) {
            self.lower(body).load(&mut self, &layout);
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
