use std::{collections::HashMap, marker::PhantomData};

use ena::unify::{InPlaceUnificationTable, NoError};
use salsa::{Accumulator, Database};

use crate::{
    ast::{ExprData, ExprId, Ident, PatternData, PatternId, Span},
    bytecode::{self, Type::UInt8},
    diagnostic::Diagnostic,
    input::Source,
    resolve::{self, Item, parse_fn_signature},
};

#[salsa::interned(debug)]
pub struct TypeId {
    pub data: TypeData<'db>,
}

use ena::unify::{UnifyKey, UnifyValue};

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy, salsa::Update)]
pub struct InferVar<'db>(pub u32, pub PhantomData<&'db ()>);

#[derive(Clone, Debug, PartialEq)]
pub enum InferValue<'db> {
    Unbound,
    Numeric,
    Bound(TypeId<'db>),
}

impl<'db> UnifyKey for InferVar<'db> {
    type Value = InferValue<'db>;

    fn index(&self) -> u32 {
        self.0
    }
    fn from_index(u: u32) -> Self {
        InferVar(u, PhantomData)
    }
    fn tag() -> &'static str {
        "InferVar"
    }
}

impl<'db> UnifyValue for InferValue<'db> {
    type Error = NoError;

    fn unify_values(value1: &Self, value2: &Self) -> Result<Self, Self::Error> {
        match (value1, value2) {
            // If either is bound, the result is bound.
            // (Structural checking happens in `Env::unify`, not here).
            (InferValue::Bound(t1), InferValue::Bound(t2)) => {
                assert_eq!(t1, t2);
                Ok(InferValue::Bound(*t1))
            }
            (InferValue::Bound(t), _) | (_, InferValue::Bound(t)) => Ok(InferValue::Bound(*t)),
            // If either is constrained to be numeric, propagate the constraint.
            (InferValue::Numeric, _) | (_, InferValue::Numeric) => Ok(InferValue::Numeric),
            (InferValue::Unbound, InferValue::Unbound) => Ok(InferValue::Unbound),
        }
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum TypeData<'db> {
    Error,

    Infer(InferVar<'db>),

    Primitive(bytecode::Type),
    Range,
    Fn(FnSig<'db>),
    Ptr(TypeId<'db>, bool),
    Slice(TypeId<'db>, bool),
    Tuple(Vec<TypeId<'db>>),
    Var(Source, Ident<'db>),
    Array(usize, TypeId<'db>),
}

impl<'db> TypeData<'db> {
    pub fn wrap(self, db: &'db dyn Database) -> TypeId<'db> {
        TypeId::new(db, self)
    }
}

impl<'db> TypeId<'db> {
    pub(crate) fn layout(&self, db: &dyn Database) -> bytecode::Layout {
        match self.data(db) {
            r @ (TypeData::Error | TypeData::Infer(_)) => panic!("{:?}", r),
            TypeData::Primitive(tp) => bytecode::Layout::primitive(tp),
            TypeData::Range => bytecode::Layout::strct(&[
                bytecode::Layout::primitive(bytecode::Type::UInt64),
                bytecode::Layout::primitive(bytecode::Type::UInt64),
            ]),
            TypeData::Fn(_) => bytecode::Layout::ptr(),
            TypeData::Ptr(_, _) => bytecode::Layout::ptr(),
            TypeData::Slice(_, _) => bytecode::Layout::strct(&[
                bytecode::Layout::ptr(),
                bytecode::Layout::primitive(bytecode::Type::UInt64),
            ]),
            TypeData::Tuple(items) => bytecode::Layout::strct(
                &items.iter().map(|tp| tp.layout(db)).collect::<Vec<_>>()[..],
            ),
            TypeData::Var(sf, tv) => {
                let fields = struct_fields(db, sf, tv);
                let mut fields = fields
                    .iter()
                    .map(|(_, (id, tp))| (id, tp))
                    .collect::<Vec<_>>();
                fields.sort_by_key(|(id, _)| **id);
                bytecode::Layout::strct(
                    &fields
                        .iter()
                        .map(|(_, tp)| tp.layout(db))
                        .collect::<Vec<_>>()[..],
                )
            }
            TypeData::Array(n, tp) => bytecode::Layout::array(n, tp.layout(db)),
        }
    }
}

pub fn struct_fields<'a>(
    db: &'a dyn Database,
    sf: Source,
    tv: Ident<'a>,
) -> HashMap<Ident<'a>, (usize, TypeId<'a>)> {
    match resolve::get_item(db, sf, tv) {
        Some(Item::Type { def, .. }) => def
            .fields(db)
            .into_iter()
            .enumerate()
            .map(|(id, (name, tp))| (name, (id, tp)))
            .collect(),
        _ => panic!(),
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

pub struct Env<'a> {
    scopes: Vec<HashMap<Ident<'a>, VarBinding<'a>>>,
    unif_table: InPlaceUnificationTable<InferVar<'a>>,
    source: Source,
    type_map: HashMap<ExprId<'a>, TypeId<'a>>,
    db: &'a dyn Database,
}

impl Diagnostic {
    pub fn type_mismatch(
        db: &dyn Database,
        span: Span,
        exp: TypeId,
        got: TypeId,
        source: Source,
    ) -> Diagnostic {
        Diagnostic::error(
            db,
            source,
            span,
            format!("type mismatch. expected: {:?}, got: {:?}", exp, got),
        )
    }

    pub fn missing_argument(
        db: &dyn Database,
        id: usize,
        span: Span,
        tp: TypeId,
        source: Source,
    ) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            format!("missing arg #{} of type {:?}", id, tp),
        )
    }

    pub fn unexpected_argument(db: &dyn Database, id: usize, span: Span, source: Source) -> Self {
        Diagnostic::error(db, source, span, format!("unexpected arg #{}", id))
    }

    pub fn unbound_var(db: &dyn Database, span: Span, name: Ident, source: Source) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            format!("unbound var: {:?}", name.text(db)),
        )
    }

    pub fn unknown_type(db: &dyn Database, span: Span, name: String, source: Source) -> Self {
        Diagnostic::error(db, source, span, format!("unknown type: {:?}", name))
    }

    pub fn duplicate_field(db: &dyn Database, span: Span, name: Ident, source: Source) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            format!("duplicate field: {:?}", name.text(db)),
        )
    }

    pub fn missing_field(db: &dyn Database, span: Span, name: Ident, source: Source) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            format!("missing field: {:?}", name.text(db)),
        )
    }

    pub fn no_field_on_type(
        db: &dyn Database,
        span: Span,
        name: Ident,
        tp: TypeId,
        source: Source,
    ) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            format!("no field named {:?} on type {:?}", name.text(db), tp),
        )
    }

    pub fn not_a_function(db: &dyn Database, span: Span, source: Source) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            "this expression is not a function and cannot be called".to_string(),
        )
    }

    pub fn cannot_assign(db: &dyn Database, span: Span, source: Source) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            "this expression cannot be mutated".to_string(),
        )
    }

    pub fn cannot_index(db: &dyn Database, span: Span, source: Source) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            "this expression cannot be indexed".to_string(),
        )
    }

    pub fn cannot_index_with(db: &dyn Database, span: Span, source: Source) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            "this expression cannot be use as an index".to_string(),
        )
    }

    pub fn cannot_dereference(db: &dyn Database, span: Span, source: Source) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            "this expression cannot be dereferenced".to_string(),
        )
    }

    pub fn unexpected_tuple(
        db: &dyn Database,
        span: Span,
        n: usize,
        tp: TypeId,
        source: Source,
    ) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            format!("expected {:?}, but this matches {}-element tuple", tp, n),
        )
    }

    pub fn missing_else_branch(db: &dyn Database, span: Span, tp: TypeId, source: Source) -> Self {
        Diagnostic::error(
            db,
            source,
            span,
            format!("missing else branch of type {:?}", tp),
        )
    }
}

impl<'a> Env<'a> {
    pub fn new(db: &'a dyn Database, source: Source) -> Self {
        let scopes = vec![HashMap::new()];
        Self {
            scopes,
            source,
            unif_table: InPlaceUnificationTable::new(),
            type_map: HashMap::new(),
            db,
        }
    }

    pub fn new_numeric_var(&mut self) -> TypeId<'a> {
        let var = self.unif_table.new_key(InferValue::Numeric);
        TypeData::Infer(var).wrap(self.db)
    }

    pub fn new_unbound_var(&mut self) -> TypeId<'a> {
        let var = self.unif_table.new_key(InferValue::Unbound);
        TypeData::Infer(var).wrap(self.db)
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

    fn bind_var(&mut self, var: InferVar<'a>, ty: TypeId<'a>) -> bool {
        if let InferValue::Numeric = self.unif_table.probe_value(var) {
            match ty.data(self.db) {
                TypeData::Primitive(_) => {} // Assuming `is_numeric` exists
                TypeData::Infer(_) => {}     // Binding to another infer var is fine
                _ => return false,           // Failed constraint
            }
        }

        // TODO: occurs check

        self.unif_table
            .unify_var_value(var, InferValue::Bound(ty))
            .is_ok()
    }

    pub fn coerce_into(&mut self, from: TypeId<'a>, into: TypeId<'a>) -> bool {
        let db = self.db;
        match (
            self.weak_resolve(from).data(db),
            self.weak_resolve(into).data(db),
        ) {
            (_, TypeData::Error) | (TypeData::Error, _) => true,
            (TypeData::Primitive(tp1), TypeData::Primitive(tp2)) => tp1 == tp2,
            (TypeData::Range, TypeData::Range) => true,
            (TypeData::Infer(v1), TypeData::Infer(v2)) => {
                self.unif_table.unify_var_var(v1, v2).is_ok()
            }

            (TypeData::Infer(var), _) => self.bind_var(var, into),
            (_, TypeData::Infer(var)) => self.bind_var(var, from),
            (TypeData::Var(sf1, id1), TypeData::Var(sf2, id2)) => sf1 == sf2 && id1 == id2,
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

    pub fn weak_resolve(&mut self, mut tp: TypeId<'a>) -> TypeId<'a> {
        while let TypeData::Infer(var) = tp.data(self.db) {
            match self.unif_table.probe_value(var) {
                InferValue::Bound(bound_tp) => {
                    tp = bound_tp;
                }
                _ => break,
            }
        }
        tp
    }

    pub fn infer_expr(&mut self, e: ExprId<'a>) -> (TypeId<'a>, bool) {
        let db = self.db;
        let (tp, is_mut) = match e.data(db) {
            ExprData::Number(_) => (self.new_numeric_var(), false),
            ExprData::Char(_) => (TypeData::Primitive(UInt8).wrap(db), false),
            ExprData::Str(s) => {
                let n = s.text(db).len();
                (
                    TypeData::Array(n + 1, TypeData::Primitive(UInt8).wrap(db)).wrap(db),
                    false,
                )
            }
            ExprData::Binop(op, expr, expr1) => {
                use crate::common::Binop::*;
                let tp = match op {
                    Add | Sub | Mul | Div | Mod => {
                        let tp = self.new_numeric_var();
                        self.check_expr(expr, tp, false);
                        self.check_expr(expr1, tp, false);
                        tp
                    }
                    Eq | Lt | NEq | Gt | Le | Ge => {
                        let tp = self.new_numeric_var();
                        self.check_expr(expr, tp, false);
                        self.check_expr(expr1, tp, false);
                        TypeData::Primitive(bytecode::Type::Bool).wrap(db)
                    }
                    And | Or => {
                        self.check_expr(
                            expr,
                            TypeData::Primitive(bytecode::Type::Bool).wrap(db),
                            false,
                        );
                        self.check_expr(
                            expr1,
                            TypeData::Primitive(bytecode::Type::Bool).wrap(db),
                            false,
                        );
                        TypeData::Primitive(bytecode::Type::Bool).wrap(db)
                    }
                };
                (tp, false)
            }
            ExprData::Unop(op, expr) => {
                use crate::common::Unop::*;
                let tp = match op {
                    Neg => {
                        let tp = self.new_numeric_var();
                        self.check_expr(expr, tp, false);
                        tp
                    }
                    Not => {
                        self.check_expr(
                            expr,
                            TypeData::Primitive(bytecode::Type::Bool).wrap(db),
                            false,
                        );
                        TypeData::Primitive(bytecode::Type::Bool).wrap(db)
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
                    Diagnostic::unbound_var(db, e.span(db), x, self.source);
                    (TypeData::Error.wrap(db), true)
                }
            },
            ExprData::FnCall(fn_expr, exprs) => {
                let (tp, _) = self.infer_expr(fn_expr);
                let sig = match tp.data(db) {
                    TypeData::Fn(sig) => sig,
                    _ => {
                        Diagnostic::not_a_function(db, fn_expr.span(db), self.source)
                            .accumulate(db);
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
                            Diagnostic::unexpected_argument(db, id, e.span(db), self.source)
                                .accumulate(db);
                            continue;
                        }
                    };
                    self.check_expr(e, exp_tp, false);
                }
                if let Some(tp) = tp_args.next() {
                    Diagnostic::missing_argument(db, id, e.span(db), tp, self.source)
                        .accumulate(db);
                }
                (sig.ret, false)
            }
            ExprData::Error => (TypeData::Error.wrap(db), true),
            ExprData::If(cond, th, el) => {
                self.check_expr(
                    cond,
                    TypeData::Primitive(bytecode::Type::Bool).wrap(db),
                    false,
                );
                let (tp, _) = self.infer_expr(th);
                if let Some(el) = el {
                    self.check_expr(el, tp, false);
                } else {
                    if !self.coerce_into(tp, TypeData::Tuple(vec![]).wrap(db)) {
                        Diagnostic::missing_else_branch(db, e.span(db), tp, self.source)
                            .accumulate(db)
                    }
                }
                (tp, false)
            }
            ExprData::While(cond, body) => {
                self.check_expr(
                    cond,
                    TypeData::Primitive(bytecode::Type::Bool).wrap(db),
                    false,
                );
                self.infer_expr(body);
                (TypeData::Tuple(vec![]).wrap(db), false)
            }
            ExprData::Assign(e1, e2) => {
                let (tp, is_mut) = self.infer_expr(e1);
                if !is_mut {
                    Diagnostic::cannot_assign(db, e1.span(db), self.source).accumulate(db);
                }
                self.check_expr(e2, tp, false);
                (TypeData::Tuple(vec![]).wrap(db), false)
            }
            ExprData::Deref(e) => match self.infer_expr(e).0.data(db) {
                TypeData::Ptr(tp, is_mut) => (tp, is_mut),
                _ => {
                    Diagnostic::cannot_dereference(db, e.span(db), self.source).accumulate(db);
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
            ExprData::Bool(_) => (TypeData::Primitive(bytecode::Type::Bool).wrap(db), false),
            ExprData::Seq(e1, e2) => {
                self.infer_expr(e1);
                self.infer_expr(e2)
            }
            ExprData::Struct(name, mut items) => {
                let tp = {
                    let fields = struct_fields(db, self.source, name);
                    for (field, tp) in fields {
                        let mut iter = items.extract_if(.., |(name, _)| *name == field);
                        match (iter.next(), iter.next()) {
                            (Some((_, expr)), None) => {
                                self.check_expr(expr, tp.1, false);
                            }
                            (Some(_), Some(_)) => {
                                Diagnostic::duplicate_field(db, e.span(db), field, self.source)
                                    .accumulate(db);
                            }
                            (None, _) => {
                                Diagnostic::missing_field(db, e.span(db), field, self.source)
                                    .accumulate(db);
                            }
                        }
                    }
                    TypeData::Var(self.source, name).wrap(db)
                };
                (tp, false)
            }
            ExprData::Field(expr, ident) => {
                let (tp, is_mut) = self.infer_expr(expr);
                match self.weak_resolve(tp).data(db) {
                    TypeData::Error => (TypeData::Error.wrap(db), true),
                    TypeData::Var(sf, tv) => {
                        if let Some(tp) = struct_fields(db, sf, tv).get(&ident) {
                            (tp.1, is_mut)
                        } else {
                            Diagnostic::no_field_on_type(db, e.span(db), ident, tp, self.source)
                                .accumulate(db);
                            (TypeData::Error.wrap(db), true)
                        }
                    }
                    TypeData::Slice(_, _) => {
                        if ident.text(db) == "len" {
                            (TypeData::Primitive(bytecode::Type::UInt64).wrap(db), false)
                        } else {
                            Diagnostic::no_field_on_type(db, e.span(db), ident, tp, self.source)
                                .accumulate(db);
                            (TypeData::Error.wrap(db), true)
                        }
                    }
                    _ => {
                        Diagnostic::no_field_on_type(db, e.span(db), ident, tp, self.source)
                            .accumulate(db);
                        (TypeData::Error.wrap(db), true)
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
                let (tp, is_mut) = match self.weak_resolve(tp).data(db) {
                    TypeData::Error => (TypeData::Error.wrap(db), true),
                    TypeData::Slice(tp, is_mut) => (tp, is_mut),
                    TypeData::Array(_, tp) => (tp, is_mut),
                    _ => {
                        Diagnostic::cannot_index(db, e1.span(db), self.source).accumulate(db);
                        (TypeData::Error.wrap(db), true)
                    }
                };
                let idx_tp = self.infer_expr(e2).0;
                match self.weak_resolve(idx_tp).data(db) {
                    TypeData::Error => (TypeData::Error.wrap(db), true),
                    TypeData::Range => (TypeData::Slice(tp, is_mut).wrap(db), false),
                    TypeData::Primitive(bytecode::Type::UInt64) => (tp, is_mut),
                    TypeData::Infer(var) => {
                        self.bind_var(var, TypeData::Primitive(bytecode::Type::UInt64).wrap(db));
                        (tp, is_mut)
                    }
                    _ => {
                        Diagnostic::cannot_index_with(db, e2.span(db), self.source).accumulate(db);
                        (TypeData::Error.wrap(db), true)
                    }
                }
            }
            ExprData::Range(e1, e2) => {
                self.check_expr(
                    e1,
                    TypeData::Primitive(bytecode::Type::UInt64).wrap(db),
                    false,
                );
                self.check_expr(
                    e2,
                    TypeData::Primitive(bytecode::Type::UInt64).wrap(db),
                    false,
                );
                (TypeData::Range.wrap(db), false)
            }
            ExprData::Cast(expr, tp) => {
                self.infer_expr(expr);
                (tp, false)
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
            Diagnostic::type_mismatch(self.db, e.span(self.db), tp, tp_inferred, self.source)
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
            .or_else(|| match resolve::get_item(self.db, self.source, x) {
                Some(Item::Function { def, .. }) => {
                    let sig = parse_fn_signature(self.db, def);
                    Some(VarBinding {
                        tp: TypeData::Fn(sig).wrap(self.db),
                        is_mut: false,
                    })
                }
                _ => None,
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
                vec![(name, VarBinding { tp, is_mut })]
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
                    Diagnostic::unexpected_tuple(
                        self.db,
                        pat.span(self.db),
                        pats.len(),
                        tp,
                        self.source,
                    )
                    .accumulate(self.db);
                    vec![]
                }
            }
        }
    }

    pub fn finish(mut self) -> InferenceResult<'a> {
        let mut type_map = self.type_map.clone();

        for tp in type_map.values_mut() {
            *tp = self.zonk(*tp);
        }
        InferenceResult { type_map }
    }

    pub fn zonk(&mut self, tp: TypeId<'a>) -> TypeId<'a> {
        let db = self.db;
        let tp = self.weak_resolve(tp);
        match tp.data(db) {
            TypeData::Infer(id) => match self.unif_table.probe_value(id) {
                InferValue::Numeric => TypeData::Primitive(bytecode::Type::Int64).wrap(db),
                InferValue::Bound(tp) => self.zonk(tp),
                InferValue::Unbound => TypeData::Error.wrap(db),
            },
            TypeData::Var(_, _) | TypeData::Error | TypeData::Primitive(_) | TypeData::Range => tp,
            TypeData::Fn(fn_sig) => {
                let args = fn_sig.args.into_iter().map(|tp| self.zonk(tp)).collect();
                let ret = self.zonk(fn_sig.ret);
                let sig = FnSig { args, ret };
                TypeData::Fn(sig).wrap(db)
            }
            TypeData::Ptr(ptr_tp, is_mut) => TypeData::Ptr(self.zonk(ptr_tp), is_mut).wrap(db),
            TypeData::Slice(elem_tp, is_mut) => {
                TypeData::Slice(self.zonk(elem_tp), is_mut).wrap(db)
            }
            TypeData::Tuple(tps) => {
                TypeData::Tuple(tps.into_iter().map(|tp| self.zonk(tp)).collect()).wrap(db)
            }
            TypeData::Array(n, elem_tp) => TypeData::Array(n, self.zonk(elem_tp)).wrap(db),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VarBinding<'a> {
    tp: TypeId<'a>,
    is_mut: bool,
}
