use std::collections::HashMap;

use salsa::Database;

use crate::{
    ast::{self, ExprData, ExprId, Ident, PatternData, PatternId},
    bytecode::{self, Block, Func, FuncSig, Inst, Layout, Terminator},
    common::{Binop, Unop},
    driver::type_check_func,
    resolve::{self, parse_type_expr},
    tp::{Type, TypeInfo},
};

#[derive(Debug, Clone)]
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
        self.store_into(b, Place::Stack, layout);
    }

    pub fn get_addr(self, b: &mut Builder, layout: &bytecode::Layout) {
        match self {
            Place::Local { id, offset } => b.push_inst(Inst::LocalAddr { id, offset }),
            Place::Ref { offset } => {
                b.push_inst(Inst::PushInt(offset as i64));
                b.push_inst(Inst::CapOffset)
            }
            Place::Stack => {
                let size = layout.size();
                let id = b.new_tmp_var(size as u32);
                let place = Place::Local { id, offset: 0 };
                match &layout.fields {
                    bytecode::Fields::Primitive(_) => Place::Stack.store_into(b, place, layout),
                    bytecode::Fields::Array { stride, count } => {
                        let size = stride.size();
                        for i in (0..*count).rev() {
                            Place::Stack.store_into(b, place.add_offset((size * i) as u32), stride);
                        }
                    }
                    bytecode::Fields::Struct { fields } => {
                        for (offset, layout) in fields.into_iter().rev() {
                            Place::Stack.store_into(b, place.add_offset(*offset), layout);
                        }
                    }
                }
                b.push_inst(Inst::LocalAddr { id, offset: 0 })
            }
        };
    }

    /// Emits instructions to store data from self into place.
    pub fn store_into(self, b: &mut Builder, place: Self, layout: &bytecode::Layout) {
        match (self, place) {
            (Place::Stack, Place::Stack) => (),
            (Place::Local { id, offset }, Place::Stack) => match &layout.abi() {
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
                bytecode::Abi::Struct => match &layout.fields {
                    bytecode::Fields::Primitive(_) => self.store_into(b, Place::Stack, layout),
                    bytecode::Fields::Array { stride, count } => {
                        let size = stride.size();
                        for i in 0..*count {
                            self.add_offset((size * i) as u32)
                                .store_into(b, Place::Stack, stride);
                        }
                    }
                    bytecode::Fields::Struct { fields } => {
                        for (offset, layout) in fields {
                            self.add_offset(*offset).store_into(b, Place::Stack, layout);
                        }
                    }
                },
            },
            (Place::Ref { offset }, Place::Stack) => match &layout.abi() {
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
            (Place::Stack, Place::Ref { offset }) => match &layout.abi() {
                bytecode::Abi::Unit => todo!(),
                bytecode::Abi::Scalar(_) => {
                    b.push_inst(Inst::Store { offset });
                }
                bytecode::Abi::ScalarPair(_, _) => {
                    let id = b.new_tmp_var(8);
                    b.push_inst(Inst::Set { id, offset: 0 });

                    b.push_inst(Inst::Get {
                        id,
                        offset: 0,
                        tp: bytecode::Type::Ptr,
                    });
                    b.push_inst(Inst::Store { offset: offset + 8 });

                    b.push_inst(Inst::Get {
                        id,
                        offset: 0,
                        tp: bytecode::Type::Ptr,
                    });
                    b.push_inst(Inst::Store { offset });
                }
                bytecode::Abi::Struct => {
                    self.get_addr(b, layout);
                    let src_ptr = b.new_tmp_var(8);
                    b.push_inst(Inst::Set {
                        id: src_ptr,
                        offset: 0,
                    });
                    place.get_addr(b, layout);
                    b.push_inst(Inst::Get {
                        id: src_ptr,
                        offset: 0,
                        tp: bytecode::Type::Ptr,
                    });

                    b.push_inst(Inst::MemCopy {
                        size: layout.size(),
                    });
                }
            },
            (Place::Stack, Place::Local { id, offset }) => {
                match &layout.fields {
                    bytecode::Fields::Primitive(_) => {
                        // Manually inline the primitive storage to break the recursion
                        match place {
                            Place::Local { id, offset } => b.push_inst(Inst::Set { id, offset }),
                            Place::Ref { offset } => b.push_inst(Inst::Store { offset }),
                            Place::Stack => (), // Already handled
                        }
                    }
                    bytecode::Fields::Array { stride, count } => {
                        let size = stride.size();
                        for i in (0..*count).rev() {
                            Place::Stack.store_into(b, place.add_offset((size * i) as u32), stride);
                        }
                    }
                    bytecode::Fields::Struct { fields } => {
                        for (offset, field_layout) in fields.iter().rev() {
                            Place::Stack.store_into(b, place.add_offset(*offset), field_layout);
                        }
                    }
                }
            }
            // (Place::Stack, Place::Local { id, offset }) => match &layout.abi() {
            //     bytecode::Abi::Unit => todo!(),
            //     bytecode::Abi::Scalar(_) => {
            //         b.push_inst(Inst::Set { id, offset });
            //     }
            //     bytecode::Abi::ScalarPair(_, _) => {
            //         b.push_inst(Inst::Set {
            //             id,
            //             offset: offset + 8,
            //         });
            //         b.push_inst(Inst::Set { id, offset });
            //     }
            //     bytecode::Abi::Struct => {
            //         self.get_addr(b, layout);
            //         let src_ptr = b.new_tmp_var(8);
            //         b.push_inst(Inst::Set {
            //             id: src_ptr,
            //             offset: 0,
            //         });
            //         place.get_addr(b, layout);
            //         b.push_inst(Inst::Get {
            //             id: src_ptr,
            //             offset: 0,
            //             tp: bytecode::Type::Ptr,
            //         });

            //         b.push_inst(Inst::MemCopy {
            //             size: layout.size(),
            //         });
            //     }
            // },
            (Place::Local { .. }, Place::Ref { .. })
            | (Place::Local { .. }, Place::Local { .. }) => {
                place.get_addr(b, layout);
                self.get_addr(b, layout);
                let size = layout.size();
                b.push_inst(Inst::MemCopy { size });
            }
            (Place::Ref { .. }, Place::Local { .. }) | (Place::Ref { .. }, Place::Ref { .. }) => {
                self.get_addr(b, layout);
                let src_ptr = b.new_tmp_var(8);
                b.push_inst(Inst::Set {
                    id: src_ptr,
                    offset: 0,
                });

                place.get_addr(b, layout);

                b.push_inst(Inst::Get {
                    id: src_ptr,
                    offset: 0,
                    tp: bytecode::Type::Ptr,
                });
                let size = layout.size();
                b.push_inst(Inst::MemCopy { size });
            }
        }
    }
}

pub struct Builder<'a> {
    blocks: Vec<Block>,
    current_block: usize,
    variable_map: HashMap<Ident<'a>, usize>,
    variables: Vec<u32>,
    expr_type_map: &'a HashMap<ExprId<'a>, Type>,
    type_map: &'a HashMap<usize, TypeInfo<'a>>,
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
            expr_type_map: Box::leak(type_map),
            type_map: Box::leak(type_defs),
            db,
            func,
        }
    }

    pub fn push_inst(&mut self, inst: Inst) {
        self.blocks[self.current_block].insts.push(inst);
    }

    pub fn get_tp(&self, e: ExprId<'a>) -> &Type {
        self.expr_type_map.get(&e).unwrap()
    }

    pub fn get_layout(&self, e: ExprId<'a>) -> bytecode::Layout {
        self.get_tp(e).layout(self.type_map)
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
                let tp = self.expr_type_map.get(&e1).unwrap();
                self.lower_pat(pat, tp, &place);
                self.lower(e2)
            }
            ExprData::Var(x) => Place::Local {
                id: self.get_var(x),
                offset: 0,
            },
            ExprData::FnCall(name, args) => {
                let place = match self.get_layout(e).abi() {
                    bytecode::Abi::Unit
                    | bytecode::Abi::Scalar(_)
                    | bytecode::Abi::ScalarPair(_, _) => Place::Stack,
                    bytecode::Abi::Struct => {
                        let id = self.new_tmp_var(self.get_layout(e).size() as u32);
                        let place = Place::Local { id, offset: 0 };
                        place.get_addr(self, &bytecode::Layout::ptr());
                        place
                    }
                };
                for arg in args.into_iter() {
                    let layout = &self.get_layout(arg);
                    println!("{}, {:#?}", name.text(self.db), layout);
                    match layout.abi() {
                        bytecode::Abi::Unit
                        | bytecode::Abi::Scalar(_)
                        | bytecode::Abi::ScalarPair(_, _) => {
                            self.lower(arg).load(self, layout);
                        }
                        bytecode::Abi::Struct => {
                            let id = self.new_tmp_var(layout.size() as u32);
                            let place = Place::Local { id, offset: 0 };
                            self.lower(arg).store_into(self, place, layout);
                            place.get_addr(self, layout);
                        }
                    }
                }

                self.push_inst(Inst::Call(name.text(self.db).clone()));
                place
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
                let layout = self.get_layout(e1);
                let dest = self.lower(e1);
                self.lower(e2).store_into(self, dest, &layout);
                Place::Stack
            }
            ExprData::Deref(expr) => {
                self.lower(expr).load(self, &bytecode::Layout::ptr());
                Place::Ref { offset: 0 }
            }
            ExprData::AddressOf(e) => {
                let layout = self.get_layout(e);
                self.lower(e).get_addr(self, &layout);
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
                let tp = self.expr_type_map.get(&e1).unwrap();
                let layout = tp.layout(self.type_map);
                for _ in 0..layout.primitives().len() {
                    self.push_inst(Inst::Drop);
                }
                self.lower(e2)
            }
            ExprData::Struct(ident, exprs) => {
                let info = self.type_map.get(&ident.get_id()).unwrap();
                let mut fields = info
                    .fields
                    .iter()
                    .map(|(name, (id, _))| (id, name))
                    .collect::<Vec<_>>();
                fields.sort_by_key(|(id, _)| **id);
                let mut exprs_map: HashMap<_, _> = exprs.into_iter().collect();
                for (_, name) in fields.into_iter() {
                    let e = exprs_map.remove(&name).unwrap();
                    let layout = self.get_layout(e);
                    self.lower(e).load(self, &layout);
                }
                Place::Stack
            }
            ExprData::Field(expr, ident) => {
                let tp_struct = self.expr_type_map.get(&expr).unwrap();
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
            ExprData::Index(e1, e2) => {
                let elem_layout = {
                    match self.get_tp(e1) {
                        Type::Slice(tp, _) | Type::Array(_, tp) => tp.layout(self.type_map),
                        _ => panic!(),
                    }
                };

                let offset = match self.get_tp(e1) {
                    Type::Slice(_, _) => {
                        self.lower(e1).load(self, &bytecode::Layout::ptr());
                        0
                    }
                    Type::Array(_, _) => match self.lower(e1) {
                        Place::Local { id, offset } => {
                            self.push_inst(Inst::LocalAddr { id, offset });
                            0
                        }
                        Place::Ref { offset } => offset,
                        Place::Stack => todo!(),
                    },
                    _ => panic!(),
                };

                self.lower(e2).load(self, &bytecode::Layout::int64());

                self.push_inst(Inst::PushInt(elem_layout.size() as i64));
                self.push_inst(Inst::Binop(Binop::Mul));
                self.push_inst(Inst::CapOffset);

                match self.get_tp(e2) {
                    Type::Int => Place::Ref { offset },
                    Type::Range => {
                        self.push_inst(Inst::PushInt(offset as i64));
                        self.push_inst(Inst::CapOffset);
                        self.lower(e2).load(
                            self,
                            &bytecode::Layout::strct(&[
                                bytecode::Layout::int64(),
                                bytecode::Layout::int64(),
                            ]),
                        );
                        self.push_inst(Inst::Binop(Binop::Sub));
                        self.push_inst(Inst::Unop(Unop::Neg));
                        Place::Stack
                    }
                    _ => panic!(),
                }
            }

            ExprData::Range(e1, e2) => {
                self.lower(e1).load(self, &bytecode::Layout::int64());
                self.lower(e2).load(self, &bytecode::Layout::int64());
                Place::Stack
            }
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
                let fields = &self.type_map.get(&id).unwrap().fields;
                let field_id = fields.get(&field_name).unwrap().0;
                let layout = tp.layout(self.type_map);
                match layout.fields {
                    bytecode::Fields::Primitive(_) => todo!(),
                    bytecode::Fields::Array { .. } => todo!(),
                    bytecode::Fields::Struct { fields } => fields[field_id].0,
                }
            }
        }
    }

    pub fn lower_pat(&mut self, pat: PatternId<'a>, tp: &Type, place: &Place) {
        match pat.data(self.db) {
            PatternData::Wildcard => {
                let layout = tp.layout(self.type_map);
                for _ in 0..layout.primitives().len() {
                    self.push_inst(Inst::Drop);
                }
            }
            PatternData::Var(name, _) => {
                let layout = tp.layout(self.type_map);
                let id = self.new_var(name, layout.size as u32);
                place.store_into(self, Place::Local { id, offset: 0 }, &layout);
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

        for (arg, tp) in self.func.args(self.db).into_iter().rev() {
            let tp = parse_type_expr(self.db, tp);
            let layout = tp.layout(self.type_map);
            // if its extern, we can lower but whatever, they will be freed anyways
            match layout.abi() {
                bytecode::Abi::Unit
                | bytecode::Abi::Scalar(_)
                | bytecode::Abi::ScalarPair(_, _) => self.lower_pat(arg, &tp, &Place::Stack),
                bytecode::Abi::Struct => self.lower_pat(arg, &tp, &Place::Ref { offset: 0 }),
            }
            args.push(layout);
        }

        let layout = if let Some(tp) = self.func.ret(self.db) {
            let tp = parse_type_expr(self.db, tp);
            tp.layout(self.type_map)
        } else {
            bytecode::Layout::unit()
        };

        rets.push(layout.clone());
        let sig = FuncSig { args, rets };
        let res = if let Some(body) = self.func.body(self.db) {
            let res = self.lower(body);
            let res_place = match layout.abi() {
                bytecode::Abi::Unit
                | bytecode::Abi::Scalar(_)
                | bytecode::Abi::ScalarPair(_, _) => Place::Stack,
                bytecode::Abi::Struct => Place::Ref { offset: 0 },
            };
            res.store_into(&mut self, res_place, &layout);
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
                self.type_map as *const HashMap<usize, TypeInfo<'a>>
                    as *mut HashMap<usize, TypeInfo<'a>>,
            ));
            drop(Box::from_raw(
                self.expr_type_map as *const HashMap<ExprId<'a>, Type>
                    as *mut HashMap<ExprId<'a>, Type>,
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
