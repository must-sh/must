use std::collections::HashMap;

use salsa::Database;

use crate::{
    ast::{self, ExprData, ExprId, Ident, PatternData, PatternId},
    bytecode::{self, Block, Func, FuncSig, Inst, Terminator},
    common::Binop,
    driver::type_check_func,
    resolve::{self, parse_type_expr},
    tp::{TypeData, TypeId, TypeInfo},
};

#[derive(Debug, Clone, Copy)]
pub enum Place {
    Local {
        id: usize,
        offset: u32,
    },
    /// `id` is a local variable containing the base pointer.
    Ref {
        local_id: usize,
        offset: u32,
    },
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
            Place::Ref { local_id, offset } => Place::Ref {
                local_id,
                offset: offset + x,
            },
        }
    }

    /// Emits load instructions from self to the stack.
    pub fn load(self, b: &mut Builder, tp: bytecode::Type) {
        match self {
            Place::Local { id, offset } => {
                b.push_inst(Inst::Get { id, offset, tp });
            }
            Place::Ref { local_id, offset } => {
                b.push_inst(Inst::Get {
                    id: local_id,
                    offset: 0,
                    tp: bytecode::Type::Ptr,
                });
                b.push_inst(Inst::Load { offset, tp });
            }
        }
    }

    /// Emits store instructions from stack to self.
    pub fn store(self, b: &mut Builder) {
        match self {
            Place::Local { id, offset } => {
                b.push_inst(Inst::Set { id, offset });
            }
            Place::Ref { local_id, offset } => {
                b.push_inst(Inst::Get {
                    id: local_id,
                    offset: 0,
                    tp: bytecode::Type::Ptr,
                });
                b.push_inst(Inst::Store { offset });
            }
        }
    }

    pub fn leave_addr(self, b: &mut Builder) {
        match self {
            Place::Local { id, offset } => b.push_inst(Inst::LocalAddr { id, offset }),
            Place::Ref { local_id, offset } => {
                b.push_inst(Inst::Get {
                    id: local_id,
                    offset: 0,
                    tp: bytecode::Type::Ptr,
                });
                b.push_inst(Inst::PushInt(offset as i64));
                b.push_inst(Inst::CapOffset)
            }
        };
    }

    /// Emits instructions to store data from self into place.
    pub fn copy_to(self, b: &mut Builder, dest: Self, layout: &bytecode::Layout) {
        dest.leave_addr(b);
        self.leave_addr(b);

        let size = layout.size();
        let align = layout.align();
        b.push_inst(Inst::MemCopy { size, align });
    }

    fn as_local_id(self) -> Option<usize> {
        match self {
            Place::Local { id, .. } => Some(id),
            Place::Ref { .. } => None,
        }
    }
}

pub struct Builder<'a> {
    blocks: Vec<Block>,
    current_block: usize,
    variable_map: HashMap<Ident<'a>, Place>,
    variables: Vec<bytecode::Layout>,
    db: &'a dyn Database,
    func: ast::FnDef<'a>,
}

impl<'a> Builder<'a> {
    pub fn new(db: &'a dyn Database, func: ast::FnDef<'a>) -> Self {
        Self {
            variable_map: HashMap::new(),
            variables: vec![],
            blocks: vec![Block::empty()],
            current_block: 0,
            db,
            func,
        }
    }

    pub fn push_inst(&mut self, inst: Inst) {
        self.blocks[self.current_block].insts.push(inst);
    }

    pub fn get_tp(&self, e: ExprId<'a>) -> TypeId<'a> {
        let tp_map = type_check_func(self.db, self.func).type_map;
        *tp_map.get(&e).unwrap()
    }

    pub fn get_type_info(&self, id: usize) -> TypeInfo<'a> {
        let tp_map = resolve::get_defs(self.db, self.func.sf(self.db)).type_map;
        tp_map.get(&id).unwrap().clone()
    }

    pub fn get_layout_of_expr(&self, e: ExprId<'a>) -> bytecode::Layout {
        let tp_map = resolve::get_defs(self.db, self.func.sf(self.db)).type_map;
        self.get_tp(e).layout(&tp_map, self.db)
    }

    pub fn get_layout_of_type(&self, tp: TypeId) -> bytecode::Layout {
        let tp_map = resolve::get_defs(self.db, self.func.sf(self.db)).type_map;
        tp.layout(&tp_map, self.db)
    }

    pub fn lower_into_tmp(&mut self, e: ExprId<'a>) -> Place {
        let tp = self.get_tp(e);
        let x = self.new_tmp_var(tp);
        self.lower(e, x);
        x
    }

    pub fn lower_place(&mut self, e: ExprId<'a>) -> Option<Place> {
        match e.data(self.db) {
            ExprData::Error => panic!(),
            ExprData::Var(x) => Some(self.get_var(x)),
            ExprData::Field(expr, name) => {
                let tp_struct = self.get_tp(expr);
                let offset = self.get_offset(tp_struct, name);
                let place = self.lower_place(expr).unwrap();
                let src = place.add_offset(offset);
                Some(src)
            }
            ExprData::Let(pat, e1, e2) => {
                let place = self.lower_into_tmp(e1);
                let tp = self.get_tp(e1);
                self.lower_pat(pat, tp, place);
                self.lower_place(e2)
            }
            ExprData::Deref(expr) => {
                let id = self.lower_into_tmp(expr).as_local_id().unwrap();
                Some(Place::Ref {
                    local_id: id,
                    offset: 0,
                })
            }
            ExprData::Seq(e1, e2) => {
                self.lower_into_tmp(e1);
                self.lower_place(e2)
            }
            ExprData::Index(e1, e2) => match self.get_tp(e2).data(self.db) {
                TypeData::Int => {
                    let elem_layout = match self.get_tp(e1).data(self.db) {
                        TypeData::Slice(tp, _) => {
                            self.lower_into_tmp(e1).load(self, bytecode::Type::Ptr);
                            self.get_layout_of_type(tp)
                        }
                        TypeData::Array(_, tp) => {
                            self.lower_place(e1).unwrap().leave_addr(self);
                            self.get_layout_of_type(tp)
                        }
                        _ => panic!(),
                    };

                    let idx_place = self.lower_into_tmp(e2);

                    idx_place.load(self, bytecode::Type::Int64); // start
                    self.push_inst(Inst::PushInt(elem_layout.size() as i64));
                    self.push_inst(Inst::Binop(Binop::Mul));

                    self.push_inst(Inst::CapOffset);
                    let place = self.store_ptr();
                    Some(place)
                }
                TypeData::Range => None,
                _ => panic!(),
            },
            _ => None,
        }
    }

    /// Lowers expression into dest, returning Place, if the expression had any
    /// place before copying it.
    pub fn lower(&mut self, e: ExprId<'a>, dest: Place) {
        match e.data(self.db) {
            ExprData::Number(n) => {
                self.push_inst(Inst::PushInt(n));
                dest.store(self);
            }
            ExprData::Binop(op, e1, e2) => {
                let x = self.lower_into_tmp(e1);
                let y = self.lower_into_tmp(e2);

                let tp_x = self.get_layout_of_expr(e1).as_primitive().unwrap();
                let tp_y = self.get_layout_of_expr(e2).as_primitive().unwrap();

                x.load(self, tp_x);
                y.load(self, tp_y);
                self.push_inst(Inst::Binop(op));
                dest.store(self);
            }
            ExprData::Unop(op, e1) => {
                let x = self.lower_into_tmp(e1);

                let tp_x = self.get_layout_of_expr(e1).as_primitive().unwrap();

                x.load(self, tp_x);
                self.push_inst(Inst::Unop(op));
                dest.store(self);
            }
            ExprData::Let(pat, e1, e2) => {
                let place = self.lower_into_tmp(e1);
                let tp = self.get_tp(e1);
                self.lower_pat(pat, tp, place);
                self.lower(e2, dest)
            }
            ExprData::Var(x) => {
                let layout = self.get_layout_of_expr(e);
                let src = self.get_var(x);
                src.copy_to(self, dest, &layout);
            }
            ExprData::FnCall(name, args) => {
                match self.get_layout_of_expr(e).abi() {
                    bytecode::Abi::Unit | bytecode::Abi::Scalar(_) => (),
                    bytecode::Abi::Struct => {
                        dest.leave_addr(self);
                    }
                };

                for arg in args.into_iter() {
                    let layout = &self.get_layout_of_expr(arg);
                    let place = self.lower_into_tmp(arg);
                    match layout.abi() {
                        bytecode::Abi::Unit => (),
                        bytecode::Abi::Scalar(tp) => {
                            place.load(self, tp);
                        }
                        bytecode::Abi::Struct => {
                            place.leave_addr(self);
                        }
                    }
                }

                self.push_inst(Inst::Call(name.text(self.db).clone()));
                match self.get_layout_of_expr(e).abi() {
                    bytecode::Abi::Unit => (),
                    bytecode::Abi::Scalar(_) => dest.store(self),
                    bytecode::Abi::Struct => (),
                };
            }
            ExprData::Error => panic!("no errors allowed here"),
            ExprData::If(cond, th, el) => {
                let th_block = self.new_block();
                let el_block = self.new_block();
                let next_block = self.new_block();

                self.lower_into_tmp(cond).load(self, bytecode::Type::Bool);
                self.terminate_current_block(Terminator::Br(th_block, el_block));

                self.switch_to_block(th_block);
                self.lower(th, dest);
                self.terminate_current_block(Terminator::Jmp(next_block));

                self.switch_to_block(el_block);
                if let Some(el) = el {
                    self.lower(el, dest);
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
                self.lower_into_tmp(cond).load(self, bytecode::Type::Bool);
                self.terminate_current_block(Terminator::Br(body_block, next_block));

                self.switch_to_block(body_block);

                // both body and whole loop expression evaluate to unit,
                // and the load will be skipped anyways
                self.lower(body, dest);

                self.terminate_current_block(Terminator::Jmp(cond_block));

                self.switch_to_block(next_block);
            }
            ExprData::Assign(e1, e2) => {
                let dest = self.lower_place(e1).unwrap();
                self.lower(e2, dest);
            }
            ExprData::Deref(expr) => {
                let layout = self.get_layout_of_expr(e);
                self.lower_place(expr).unwrap().copy_to(self, dest, &layout);
            }
            ExprData::AddressOf(e) => {
                self.lower_place(e)
                    .unwrap_or_else(|| self.lower_into_tmp(e))
                    .leave_addr(self);
                dest.store(self);
            }
            ExprData::Tuple(exprs) => {
                let fields = match self.get_layout_of_expr(e).fields {
                    bytecode::Fields::Struct { fields } => fields,
                    _ => panic!(),
                };
                for (i, e) in exprs.into_iter().enumerate() {
                    self.lower(e, dest.add_offset(fields[i].0));
                }
            }
            ExprData::Bool(b) => {
                self.push_inst(Inst::PushBool(b));
                dest.store(self);
            }
            ExprData::Seq(e1, e2) => {
                self.lower_into_tmp(e1);
                self.lower(e2, dest)
            }
            ExprData::Struct(name, exprs) => {
                let info = self.get_type_info(name.get_id());
                let mut fields = info
                    .fields
                    .iter()
                    .map(|(name, (id, _))| (id, name))
                    .collect::<Vec<_>>();
                fields.sort_by_key(|(id, _)| **id);
                let mut exprs_map: HashMap<_, _> = exprs.into_iter().collect();
                let offsets = match self.get_layout_of_expr(e).fields {
                    bytecode::Fields::Struct { fields } => fields,
                    _ => panic!(),
                };
                for (i, name) in fields.into_iter() {
                    let e = exprs_map.remove(name).unwrap();
                    self.lower(e, dest.add_offset(offsets[*i].0));
                }
            }
            ExprData::Field(_, _) => {
                let layout = &self.get_layout_of_expr(e);
                let src = self.lower_place(e).unwrap();
                src.copy_to(self, dest, layout);
            }
            ExprData::Array(exprs) => {
                let mut x = 0;
                let elem_size = match self.get_layout_of_expr(e).fields {
                    bytecode::Fields::Array { stride, .. } => stride.size(),
                    _ => panic!(),
                };
                for e in exprs {
                    self.lower(e, dest.add_offset(x));
                    x += elem_size as u32;
                }
            }
            ExprData::Index(e1, e2) => {
                let elem_layout = match self.get_tp(e1).data(self.db) {
                    TypeData::Slice(tp, _) => {
                        self.lower_into_tmp(e1).load(self, bytecode::Type::Ptr);
                        self.get_layout_of_type(tp)
                    }
                    TypeData::Array(_, tp) => {
                        self.lower_place(e1).unwrap().leave_addr(self);
                        self.get_layout_of_type(tp)
                    }
                    _ => panic!(),
                };

                let idx_place = self.lower_into_tmp(e2);

                idx_place.load(self, bytecode::Type::Int64); // start
                self.push_inst(Inst::PushInt(elem_layout.size() as i64));
                self.push_inst(Inst::Binop(Binop::Mul));

                self.push_inst(Inst::CapOffset);

                match self.get_tp(e2).data(self.db) {
                    TypeData::Int => {
                        let place = self.store_ptr();
                        place.copy_to(self, dest, &elem_layout)
                    }
                    TypeData::Range => {
                        dest.store(self);
                        idx_place.add_offset(8).load(self, bytecode::Type::Int64); // end
                        idx_place.load(self, bytecode::Type::Int64); // start
                        self.push_inst(Inst::Binop(Binop::Sub));
                        dest.add_offset(8).store(self);
                    }
                    _ => panic!(),
                };
            }
            ExprData::Range(e1, e2) => {
                self.lower(e1, dest);
                self.lower(e2, dest.add_offset(8));
            }
        }
    }

    pub fn get_offset(&mut self, tp: TypeId<'a>, field_name: Ident<'a>) -> u32 {
        match tp.data(self.db) {
            TypeData::Slice(_, _) => {
                assert_eq!(field_name.text(self.db), "len");
                8
            }
            TypeData::Var(id) => {
                let field_id = self.get_type_info(id).fields.get(&field_name).unwrap().0;
                let layout = self.get_layout_of_type(tp);
                match layout.fields {
                    bytecode::Fields::Struct { fields } => fields[field_id].0,
                    _ => panic!(),
                }
            }
            _ => panic!(),
        }
    }

    pub fn lower_pat(&mut self, pat: PatternId<'a>, tp: TypeId<'a>, place: Place) {
        match pat.data(self.db) {
            PatternData::Wildcard => (),
            PatternData::Var(name, _) => {
                let layout = self.get_layout_of_type(tp);
                let id = self.new_var(name, tp);
                place.copy_to(self, id, &layout);
            }
            PatternData::Tuple(pats) => {
                if let TypeData::Tuple(tps) = tp.data(self.db) {
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
            let layout = self.get_layout_of_type(tp);
            // if its extern, we can lower but whatever, they will be freed anyways
            match layout.abi() {
                bytecode::Abi::Unit => (),
                bytecode::Abi::Scalar(_) => {
                    let tmp = self.new_tmp_var(tp);
                    tmp.store(&mut self);
                    self.lower_pat(arg, tp, tmp)
                }
                bytecode::Abi::Struct => {
                    let place = self.store_ptr();
                    self.lower_pat(arg, tp, place)
                }
            }
            args.push(layout);
        }

        let layout = if let Some(tp) = self.func.ret(self.db) {
            let tp = parse_type_expr(self.db, tp);
            self.get_layout_of_type(tp)
        } else {
            bytecode::Layout::unit()
        };

        rets.push(layout.clone());
        args.reverse();
        let sig = FuncSig { args, rets };

        if let Some(body) = self.func.body(self.db) {
            match layout.abi() {
                bytecode::Abi::Unit => {
                    self.lower_into_tmp(body);
                }
                bytecode::Abi::Scalar(tp) => {
                    self.lower_into_tmp(body).load(&mut self, tp);
                }
                bytecode::Abi::Struct => {
                    let place = self.store_ptr();
                    self.lower(body, place);
                }
            };

            LoweringResult::Function(Func {
                blocks: self.blocks,
                variables: self.variables,
                sig,
            })
        } else {
            LoweringResult::Extern(sig)
        }
    }

    fn store_ptr(&mut self) -> Place {
        let tmp = self.new_tmp_var_layout(bytecode::Layout::ptr());
        tmp.store(self);
        
        Place::Ref {
            local_id: tmp.as_local_id().unwrap(),
            offset: 0,
        }
    }

    pub fn new_var(&mut self, x: Ident<'a>, tp: TypeId<'a>) -> Place {
        let id = self.new_tmp_var(tp);
        self.variable_map.insert(x, id);
        id
    }

    pub fn new_tmp_var(&mut self, tp: TypeId<'a>) -> Place {
        let id = self.variables.len();
        let layout = self.get_layout_of_type(tp);
        self.variables.push(layout);
        Place::Local { id, offset: 0 }
    }

    pub fn new_tmp_var_layout(&mut self, layout: bytecode::Layout) -> Place {
        let id = self.variables.len();
        self.variables.push(layout);
        Place::Local { id, offset: 0 }
    }

    pub fn get_var(&self, x: Ident<'a>) -> Place {
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
