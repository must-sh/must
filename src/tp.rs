use std::collections::HashMap;

use salsa::{Accumulator, Database};

use crate::{
    ast::{ExprData, ExprId, Ident, PatternData, PatternId, Span},
    bytecode,
    diagnostic::Diagnostic,
    resolve::ModuleDefs,
};

#[salsa::interned(debug)]
pub struct TypeId {
    pub data: TypeData<'db>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum TypeData<'db> {
    Error,

    Int,
    Bool,
    Range,
    Fn(FnSig<'db>),
    Ptr(TypeId<'db>, bool),
    Slice(TypeId<'db>, bool),
    Tuple(Vec<TypeId<'db>>),
    Var(usize),
    Array(usize, TypeId<'db>),
}

impl<'db> TypeData<'db> {
    pub fn wrap(self, db: &'db dyn Database) -> TypeId<'db> {
        TypeId::new(db, self)
    }
}

impl<'db> TypeId<'db> {
    pub(crate) fn layout(
        &self,
        type_map: &HashMap<usize, TypeInfo>,
        db: &dyn Database,
    ) -> bytecode::Layout {
        match self.data(db) {
            TypeData::Error => panic!(),
            TypeData::Int => bytecode::Layout::int64(),
            TypeData::Bool => bytecode::Layout::bool(),
            TypeData::Range => {
                bytecode::Layout::strct(&[bytecode::Layout::int64(), bytecode::Layout::int64()])
            }
            TypeData::Fn(_) => bytecode::Layout::ptr(),
            TypeData::Ptr(_, _) => bytecode::Layout::ptr(),
            TypeData::Slice(_, _) => {
                bytecode::Layout::strct(&[bytecode::Layout::ptr(), bytecode::Layout::int64()])
            }
            TypeData::Tuple(items) => bytecode::Layout::strct(
                &items
                    .iter()
                    .map(|tp| tp.layout(type_map, db))
                    .collect::<Vec<_>>()[..],
            ),
            TypeData::Var(id) => {
                let info = type_map.get(&id).unwrap();
                let mut fields = info
                    .fields
                    .iter()
                    .map(|(_, (id, tp))| (id, tp))
                    .collect::<Vec<_>>();
                fields.sort_by_key(|(id, _)| **id);
                bytecode::Layout::strct(
                    &fields
                        .iter()
                        .map(|(_, tp)| tp.layout(type_map, db))
                        .collect::<Vec<_>>()[..],
                )
            }
            TypeData::Array(n, tp) => bytecode::Layout::array(n, tp.layout(type_map, db)),
        }
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub struct FnSig<'db> {
    pub args: Vec<TypeId<'db>>,
    pub ret: TypeId<'db>,
}

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub struct InferenceResult<'db> {
    pub type_map: HashMap<ExprId<'db>, TypeId<'db>>,
}

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub struct TypeInfo<'db> {
    pub name: Ident<'db>,
    pub fields: HashMap<Ident<'db>, (usize, TypeId<'db>)>,
}

pub struct Env<'a> {
    scopes: Vec<HashMap<Ident<'a>, VarBinding<'a>>>,
    mod_defs: ModuleDefs<'a>,
    type_map: HashMap<ExprId<'a>, TypeId<'a>>,
    db: &'a dyn Database,
}

impl Diagnostic {
    pub fn type_mismatch(db: &dyn Database, span: Span, exp: TypeId, got: TypeId) -> Diagnostic {
        Diagnostic::error(
            db,
            span,
            format!("type mismatch. expected: {:?}, got: {:?}", exp, got),
        )
    }

    pub fn missing_argument(db: &dyn Database, id: usize, span: Span, tp: TypeId) -> Self {
        Diagnostic::error(db, span, format!("missing arg #{} of type {:?}", id, tp))
    }

    pub fn unexpected_argument(db: &dyn Database, id: usize, span: Span) -> Self {
        Diagnostic::error(db, span, format!("unexpected arg #{}", id))
    }

    pub fn unbound_var(db: &dyn Database, span: Span, name: Ident) -> Self {
        Diagnostic::error(db, span, format!("unbound var: {:?}", name.text(db)))
    }

    pub fn unknown_type(db: &dyn Database, span: Span, name: Ident) -> Self {
        Diagnostic::error(db, span, format!("unknown type: {:?}", name.text(db)))
    }

    pub fn duplicate_field(db: &dyn Database, span: Span, name: Ident) -> Self {
        Diagnostic::error(db, span, format!("duplicate field: {:?}", name.text(db)))
    }

    pub fn missing_field(db: &dyn Database, span: Span, name: Ident) -> Self {
        Diagnostic::error(db, span, format!("missing field: {:?}", name.text(db)))
    }

    pub fn no_field_on_type(db: &dyn Database, span: Span, name: Ident, tp: TypeId) -> Self {
        Diagnostic::error(
            db,
            span,
            format!("no field named {:?} on type {:?}", name.text(db), tp),
        )
    }

    pub fn cannot_assign(db: &dyn Database, span: Span) -> Self {
        Diagnostic::error(db, span, "this expression cannot be mutated".to_string())
    }

    pub fn cannot_index(db: &dyn Database, span: Span) -> Self {
        Diagnostic::error(db, span, "this expression cannot be indexed".to_string())
    }

    pub fn cannot_index_with(db: &dyn Database, span: Span) -> Self {
        Diagnostic::error(
            db,
            span,
            "this expression cannot be use as an index".to_string(),
        )
    }

    pub fn cannot_dereference(db: &dyn Database, span: Span) -> Self {
        Diagnostic::error(
            db,
            span,
            "this expression cannot be dereferenced".to_string(),
        )
    }

    pub fn unexpected_tuple(db: &dyn Database, span: Span, n: usize, tp: TypeId) -> Self {
        Diagnostic::error(
            db,
            span,
            format!("expected {:?}, but this matches {}-element tuple", tp, n),
        )
    }

    pub fn missing_else_branch(db: &dyn Database, span: Span, tp: TypeId) -> Self {
        Diagnostic::error(db, span, format!("missing else branch of type {:?}", tp))
    }
}

impl<'a> Env<'a> {
    pub fn new(db: &'a dyn Database, mod_defs: ModuleDefs<'a>) -> Self {
        let scopes = vec![HashMap::new()];
        Self {
            scopes,
            mod_defs,
            type_map: HashMap::new(),
            db,
        }
    }

    pub fn with_scope<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.scopes.push(HashMap::new());
        let r = f(self);
        self.scopes.pop();
        r
    }

    pub fn coerce_into(&self, from: TypeId<'a>, into: TypeId<'a>) -> bool {
        let db = self.db;
        match (from.data(db), into.data(db)) {
            (_, TypeData::Error) | (TypeData::Error, _) => true,
            (TypeData::Int, TypeData::Int) => true,
            (TypeData::Bool, TypeData::Bool) => true,
            (TypeData::Range, TypeData::Range) => true,
            (TypeData::Var(id1), TypeData::Var(id2)) => id1 == id2,
            (TypeData::Tuple(tps1), TypeData::Tuple(tps2)) => {
                tps1.len() == tps2.len()
                    && tps1
                        .into_iter()
                        .zip(tps2)
                        .all(|(tp1, tp2)| self.coerce_into(tp1, tp2))
            }
            (TypeData::Ptr(tp1, is_mut1), TypeData::Ptr(tp2, is_mut2)) => {
                (!is_mut2 || is_mut1)
                    && self.coerce_into(tp1, tp2)
                    && (!is_mut2 || self.coerce_into(tp2, tp1))
            }
            (TypeData::Slice(tp1, is_mut1), TypeData::Slice(tp2, is_mut2)) => {
                (!is_mut2 || is_mut1)
                    && self.coerce_into(tp1, tp2)
                    && (!is_mut2 || self.coerce_into(tp2, tp1))
            }
            (TypeData::Array(n1, tp1), TypeData::Array(n2, tp2)) => {
                n1 == n2 && self.coerce_into(tp1, tp2) && self.coerce_into(tp2, tp1)
            }
            (
                TypeData::Fn(FnSig {
                    args: args1,
                    ret: ret1,
                }),
                TypeData::Fn(FnSig {
                    args: args2,
                    ret: ret2,
                }),
            ) => {
                args1.len() == args2.len()
                    && args1
                        .into_iter()
                        .zip(args2)
                        .all(|(arg1, arg2)| self.coerce_into(arg2, arg1))
                    && self.coerce_into(ret1, ret2)
            }
            _ => false,
        }
    }

    pub fn infer_expr(&mut self, e: ExprId<'a>) -> (TypeId<'a>, bool) {
        let db = self.db;
        let (tp, is_mut) = match e.data(db) {
            ExprData::Number(_) => (TypeData::Int.wrap(db), false),
            ExprData::Binop(op, expr, expr1) => {
                use crate::common::Binop::*;
                let tp = match op {
                    Add | Sub | Mul | Div | Mod => {
                        self.check_expr(expr, TypeData::Int.wrap(db), false);
                        self.check_expr(expr1, TypeData::Int.wrap(db), false);
                        TypeData::Int.wrap(db)
                    }
                    Eq | Lt | NEq | Gt | Le | Ge => {
                        self.check_expr(expr, TypeData::Int.wrap(db), false);
                        self.check_expr(expr1, TypeData::Int.wrap(db), false);
                        TypeData::Bool.wrap(db)
                    }
                    And | Or => {
                        self.check_expr(expr, TypeData::Bool.wrap(db), false);
                        self.check_expr(expr1, TypeData::Bool.wrap(db), false);
                        TypeData::Bool.wrap(db)
                    }
                };
                (tp, false)
            }
            ExprData::Unop(op, expr) => {
                use crate::common::Unop::*;
                let tp = match op {
                    Neg => {
                        self.check_expr(expr, TypeData::Int.wrap(db), false);
                        TypeData::Int.wrap(db)
                    }
                    Not => {
                        self.check_expr(expr, TypeData::Bool.wrap(db), false);
                        TypeData::Bool.wrap(db)
                    }
                };
                (tp, false)
            }
            ExprData::Let(pat, e1, e2) => {
                let (tp1, _) = self.infer_expr(e1);
                self.with_scope(|env| {
                    let bindings = env.check_pat(pat, tp1);
                    env.extend(bindings);
                    env.infer_expr(e2)
                })
            }
            ExprData::Var(x) => match self.get_var(x) {
                Some(VarBinding { tp, is_mut }) => (tp, is_mut),
                None => {
                    Diagnostic::unbound_var(db, e.span(db), x).accumulate(db);
                    (TypeData::Error.wrap(db), true)
                }
            },
            ExprData::FnCall(fn_name, exprs) => {
                let sig = match self.mod_defs.function_map.get(&fn_name) {
                    Some(sig) => sig.clone(),
                    None => {
                        Diagnostic::unbound_var(db, e.span(db), fn_name).accumulate(db);
                        return (TypeData::Error.wrap(db), true);
                    }
                };
                let mut tp_args = sig.args.into_iter();
                let mut id = 0;
                for e in exprs {
                    id += 1;
                    let exp_tp = match tp_args.next() {
                        Some(tp) => tp,
                        None => {
                            Diagnostic::unexpected_argument(db, id, e.span(db)).accumulate(db);
                            continue;
                        }
                    };
                    self.check_expr(e, exp_tp, false);
                }
                if let Some(tp) = tp_args.next() {
                    Diagnostic::missing_argument(db, id, e.span(db), tp).accumulate(db);
                }
                (sig.ret, false)
            }
            ExprData::Error => (TypeData::Error.wrap(db), true),
            ExprData::If(cond, th, el) => {
                self.check_expr(cond, TypeData::Bool.wrap(db), false);
                let (tp, _) = self.infer_expr(th);
                if let Some(el) = el {
                    self.check_expr(el, tp, false);
                } else {
                    if !self.coerce_into(tp, TypeData::Tuple(vec![]).wrap(db)) {
                        Diagnostic::missing_else_branch(db, e.span(db), tp).accumulate(db)
                    }
                }
                (tp, false)
            }
            ExprData::While(cond, body) => {
                self.check_expr(cond, TypeData::Bool.wrap(db), false);
                self.infer_expr(body);
                (TypeData::Tuple(vec![]).wrap(db), false)
            }
            ExprData::Assign(e1, e2) => {
                let (tp, is_mut) = self.infer_expr(e1);
                if !is_mut {
                    Diagnostic::cannot_assign(db, e1.span(db)).accumulate(db);
                }
                self.check_expr(e2, tp, false);
                (TypeData::Tuple(vec![]).wrap(db), false)
            }
            ExprData::Deref(e) => match self.infer_expr(e).0.data(db) {
                TypeData::Ptr(tp, is_mut) => (tp, is_mut),
                _ => {
                    Diagnostic::cannot_dereference(db, e.span(db)).accumulate(db);
                    (TypeData::Error.wrap(db), true)
                }
            },
            ExprData::AddressOf(e) => {
                let (tp, is_mut) = self.infer_expr(e);
                (TypeData::Ptr(tp, is_mut).wrap(db), false)
            }
            ExprData::Tuple(exprs) => {
                let tps = exprs.into_iter().map(|e| self.infer_expr(e).0).collect();
                (TypeData::Tuple(tps).wrap(db), false)
            }
            ExprData::Bool(_) => (TypeData::Bool.wrap(db), false),
            ExprData::Seq(e1, e2) => {
                self.infer_expr(e1);
                self.infer_expr(e2)
            }
            ExprData::Struct(ident, mut items) => {
                let tp = if let Some(tp_info) = self.get_type_info(ident.get_id()) {
                    for (field, tp) in tp_info.fields {
                        let mut iter = items.extract_if(.., |(name, _)| *name == field);
                        match (iter.next(), iter.next()) {
                            (Some((_, expr)), None) => {
                                self.check_expr(expr, tp.1, false);
                            }
                            (Some(_), Some(_)) => {
                                Diagnostic::duplicate_field(db, e.span(db), field).accumulate(db);
                            }
                            (None, _) => {
                                Diagnostic::missing_field(db, e.span(db), field).accumulate(db);
                            }
                        }
                    }
                    TypeData::Var(tp_info.name.get_id()).wrap(db)
                } else {
                    Diagnostic::unknown_type(db, e.span(db), ident).accumulate(db);
                    TypeData::Error.wrap(db)
                };
                (tp, false)
            }
            ExprData::Field(expr, ident) => {
                let (tp, is_mut) = self.infer_expr(expr);
                match tp.data(db) {
                    TypeData::Error => (TypeData::Error.wrap(db), true),
                    TypeData::Ptr(_, _)
                    | TypeData::Int
                    | TypeData::Bool
                    | TypeData::Range
                    | TypeData::Fn(_)
                    | TypeData::Array(_, _)
                    | TypeData::Tuple(_) => {
                        Diagnostic::no_field_on_type(db, e.span(db), ident, tp).accumulate(db);
                        (TypeData::Error.wrap(db), true)
                    }
                    TypeData::Var(id) => {
                        if let Some(tp_info) = self.get_type_info(id) {
                            if let Some(tp) = tp_info.fields.get(&ident) {
                                (tp.1, is_mut)
                            } else {
                                Diagnostic::no_field_on_type(db, e.span(db), ident, tp)
                                    .accumulate(db);
                                (TypeData::Error.wrap(db), true)
                            }
                        } else {
                            panic!()
                        }
                    }
                    TypeData::Slice(_, _) => {
                        if ident.text(db) == "len" {
                            (TypeData::Int.wrap(db), false)
                        } else {
                            Diagnostic::no_field_on_type(db, e.span(db), ident, tp).accumulate(db);
                            (TypeData::Error.wrap(db), true)
                        }
                    }
                }
            }
            ExprData::Array(mut exprs) => {
                if exprs.is_empty() {
                    (TypeData::Tuple(vec![]).wrap(db), false)
                } else {
                    let n = exprs.len();
                    let first_expr = exprs.swap_remove(0);
                    let (tp, _) = self.infer_expr(first_expr);
                    for e in exprs {
                        self.check_expr(e, tp, false);
                    }
                    (TypeData::Array(n, tp).wrap(db), false)
                }
            }
            ExprData::Index(e1, e2) => {
                let (tp, is_mut) = self.infer_expr(e1);
                let (tp, is_mut) = match tp.data(db) {
                    TypeData::Error => (TypeData::Error.wrap(db), true),
                    TypeData::Int
                    | TypeData::Bool
                    | TypeData::Range
                    | TypeData::Fn(_)
                    | TypeData::Ptr(_, _)
                    | TypeData::Tuple(_)
                    | TypeData::Var(_) => {
                        Diagnostic::cannot_index(db, e1.span(db)).accumulate(db);
                        (TypeData::Error.wrap(db), true)
                    }
                    TypeData::Slice(tp, is_mut) => (tp, is_mut),
                    TypeData::Array(_, tp) => (tp, is_mut),
                };
                match self.infer_expr(e2).0.data(db) {
                    TypeData::Error => (TypeData::Error.wrap(db), true),
                    TypeData::Range => (TypeData::Slice(tp, is_mut).wrap(db), false),
                    TypeData::Int => (tp, is_mut),
                    TypeData::Bool
                    | TypeData::Fn(_)
                    | TypeData::Ptr(_, _)
                    | TypeData::Slice(_, _)
                    | TypeData::Tuple(_)
                    | TypeData::Var(_)
                    | TypeData::Array(_, _) => {
                        Diagnostic::cannot_index_with(db, e1.span(db)).accumulate(db);
                        (TypeData::Error.wrap(db), true)
                    }
                }
            }
            ExprData::Range(e1, e2) => {
                self.check_expr(e1, TypeData::Int.wrap(db), false);
                self.check_expr(e2, TypeData::Int.wrap(db), false);
                (TypeData::Range.wrap(db), false)
            }
        };
        self.type_map.insert(e, tp);
        (tp, is_mut)
    }

    pub fn extend(&mut self, bindings: Vec<(Ident<'a>, VarBinding<'a>)>) {
        for (name, binding) in bindings {
            self.add_var(name, binding);
        }
    }

    pub fn check_expr(&mut self, e: ExprId<'a>, tp: TypeId<'a>, exp_mut: bool) {
        let (tp_inferred, mut_inferred) = self.infer_expr(e);
        if !(self.coerce_into(tp_inferred, tp) && (!exp_mut || mut_inferred)) {
            Diagnostic::type_mismatch(self.db, e.span(self.db), tp, tp_inferred)
                .accumulate(self.db);
        }
    }

    pub(crate) fn add_var(&mut self, arg: Ident<'a>, binding: VarBinding<'a>) {
        self.scopes.last_mut().unwrap().insert(arg, binding);
    }

    pub fn get_var(&self, x: Ident<'a>) -> Option<VarBinding<'a>> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&x))
            .cloned()
            .or_else(|| {
                self.mod_defs
                    .function_map
                    .get(&x)
                    .cloned()
                    .map(TypeData::Fn)
                    .map(|tp| VarBinding {
                        tp: tp.wrap(self.db),
                        is_mut: false,
                    })
            })
    }

    pub fn check_pat(
        &self,
        pat: PatternId<'a>,
        tp: TypeId<'a>,
    ) -> Vec<(Ident<'a>, VarBinding<'a>)> {
        match pat.data(self.db) {
            PatternData::Wildcard => vec![],
            PatternData::Var(name, is_mut) => {
                vec![(
                    name,
                    VarBinding {
                        tp,
                        is_mut,
                    },
                )]
            }
            PatternData::Tuple(pats) => {
                if let TypeData::Tuple(tps) = tp.data(self.db)
                    && tps.len() == pats.len()
                {
                    pats.into_iter()
                        .zip(tps)
                        .flat_map(|(pat, tp)| self.check_pat(pat, tp))
                        .collect()
                } else {
                    Diagnostic::unexpected_tuple(self.db, pat.span(self.db), pats.len(), tp)
                        .accumulate(self.db);
                    vec![]
                }
            }
        }
    }

    pub fn finish(self) -> InferenceResult<'a> {
        InferenceResult {
            type_map: self.type_map,
        }
    }

    fn get_type_info(&self, id: usize) -> Option<TypeInfo<'a>> {
        self.mod_defs.type_map.get(&id).cloned()
    }
}

#[derive(Debug, Clone)]
pub struct VarBinding<'a> {
    tp: TypeId<'a>,
    is_mut: bool,
}
