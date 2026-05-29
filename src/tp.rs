use std::collections::HashMap;

use salsa::{Accumulator, Database};

use crate::{
    ast::{ExprData, ExprId, Ident, PatternData, PatternId, Span},
    diagnostic::Diagnostic,
    resolve::ModuleDefs,
};

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub enum Type {
    Error,

    Int,
    Bool,
    Fn(FnSig),
    Ptr(Box<Type>, bool),
    Tuple(Vec<Type>),
    Var(usize),
}

impl Type {
    pub fn coerce_into(&self, into: &Type) -> bool {
        match (self, into) {
            (_, Type::Error) | (Type::Error, _) => true,
            (Type::Int, Type::Int) => true,
            (Type::Bool, Type::Bool) => true,
            (Type::Var(id1), Type::Var(id2)) => id1 == id2,
            (Type::Tuple(tps1), Type::Tuple(tps2)) => {
                tps1.len() == tps2.len()
                    && tps1
                        .iter()
                        .zip(tps2.iter())
                        .all(|(tp1, tp2)| tp1.coerce_into(tp2))
            }
            (Type::Ptr(tp1, is_mut1), Type::Ptr(tp2, is_mut2)) => {
                (!is_mut2 || *is_mut1) && tp1.coerce_into(tp2) && (!is_mut2 || tp2.coerce_into(tp1))
            }
            (
                Self::Fn(FnSig {
                    args: args1,
                    ret: ret1,
                }),
                Self::Fn(FnSig {
                    args: args2,
                    ret: ret2,
                }),
            ) => {
                args1.len() == args2.len()
                    && args1
                        .iter()
                        .zip(args2)
                        .all(|(arg1, arg2)| arg2.coerce_into(arg1))
                    && ret1.coerce_into(ret2)
            }
            _ => false,
        }
    }

    pub fn get_size<'db>(&self, type_map: &HashMap<usize, TypeInfo<'db>>) -> usize {
        match self {
            Type::Error => panic!(),
            Type::Int | Type::Bool | Type::Fn(_) | Type::Ptr(_, _) => 1,
            Type::Tuple(tps) => tps.iter().map(|tp| tp.get_size(type_map)).sum(),
            Type::Var(id) => {
                let info = type_map.get(id).unwrap();
                info.fields
                    .iter()
                    .map(|(_, tp)| tp.1.get_size(type_map))
                    .sum()
            }
        }
    }

    pub fn as_var_id(&self) -> usize {
        match self {
            Type::Error
            | Type::Int
            | Type::Bool
            | Type::Fn(_)
            | Type::Ptr(_, _)
            | Type::Tuple(_) => panic!(),
            Type::Var(id) => *id,
        }
    }
}

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub struct FnSig {
    pub args: Vec<Type>,
    pub ret: Box<Type>,
}

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub struct InferenceResult<'db> {
    pub type_map: HashMap<ExprId<'db>, Type>,
}

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub struct TypeInfo<'db> {
    pub name: Ident<'db>,
    pub fields: HashMap<Ident<'db>, (usize, Type)>,
}

pub struct Env<'a> {
    scopes: Vec<HashMap<Ident<'a>, VarBinding>>,
    mod_defs: ModuleDefs<'a>,
    type_map: HashMap<ExprId<'a>, Type>,
    db: &'a dyn Database,
}

impl Diagnostic {
    pub fn type_mismatch(db: &dyn Database, span: Span, exp: &Type, got: &Type) -> Diagnostic {
        Diagnostic::error(
            db,
            span,
            format!("type mismatch. expected: {:?}, got: {:?}", exp, got),
        )
    }

    pub fn missing_argument(db: &dyn Database, id: usize, span: Span, tp: &Type) -> Self {
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

    pub fn no_field_on_type(db: &dyn Database, span: Span, name: Ident, tp: &Type) -> Self {
        Diagnostic::error(
            db,
            span,
            format!("no field named {:?} on type {:?}", name.text(db), tp),
        )
    }

    pub fn cannot_assign(db: &dyn Database, span: Span) -> Self {
        Diagnostic::error(db, span, "this expression cannot be mutated".to_string())
    }

    pub fn cannot_dereference(db: &dyn Database, span: Span) -> Self {
        Diagnostic::error(
            db,
            span,
            "this expression cannot be dereferenced".to_string(),
        )
    }

    pub fn unexpected_tuple(db: &dyn Database, span: Span, n: usize, tp: &Type) -> Self {
        Diagnostic::error(
            db,
            span,
            format!("expected {:?}, but this matches {}-element tuple", tp, n),
        )
    }

    pub fn missing_else_branch(db: &dyn Database, span: Span, tp: &Type) -> Self {
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

    pub fn infer_expr(&mut self, e: ExprId<'a>) -> (Type, bool) {
        let db = self.db;
        let (tp, is_mut) = match e.data(db) {
            ExprData::Number(_) => (Type::Int, false),
            ExprData::Binop(op, expr, expr1) => {
                use crate::common::Op::*;
                let tp = match op {
                    Add | Sub | Mul | Div => {
                        self.check_expr(expr, &Type::Int, false);
                        self.check_expr(expr1, &Type::Int, false);
                        Type::Int
                    }
                    Eq | Lt => {
                        self.check_expr(expr, &Type::Int, false);
                        self.check_expr(expr1, &Type::Int, false);
                        Type::Bool
                    }
                };
                (tp, false)
            }
            ExprData::Let(pat, e1, e2) => {
                let (tp1, _) = self.infer_expr(e1);
                self.with_scope(|env| {
                    let bindings = env.check_pat(pat, &tp1);
                    env.extend(bindings);
                    env.infer_expr(e2)
                })
            }
            ExprData::Var(x) => match self.get_var(x) {
                Some(VarBinding { tp, is_mut }) => (tp.clone(), is_mut),
                None => {
                    Diagnostic::unbound_var(db, e.span(db), x).accumulate(db);
                    (Type::Error, true)
                }
            },
            ExprData::FnCall(fn_name, exprs) => {
                let sig = match self.mod_defs.function_map.get(&fn_name) {
                    Some(sig) => sig.clone(),
                    None => {
                        Diagnostic::unbound_var(db, e.span(db), fn_name).accumulate(db);
                        return (Type::Error, true);
                    }
                };
                let mut tp_args = sig.args.iter();
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
                (*sig.ret.clone(), false)
            }
            ExprData::Error => (Type::Error, true),
            ExprData::If(cond, th, el) => {
                self.check_expr(cond, &Type::Bool, false);
                let (tp, _) = self.infer_expr(th);
                if let Some(el) = el {
                    self.check_expr(el, &tp, false);
                } else {
                    if !tp.coerce_into(&Type::Tuple(vec![])) {
                        Diagnostic::missing_else_branch(db, e.span(db), &tp).accumulate(db)
                    }
                }
                (tp, false)
            }
            ExprData::While(cond, body) => {
                self.check_expr(cond, &Type::Bool, false);
                self.infer_expr(body);
                (Type::Tuple(vec![]), false)
            }
            ExprData::Assign(e1, e2) => {
                let (tp, is_mut) = self.infer_expr(e1);
                if !is_mut {
                    Diagnostic::cannot_assign(db, e1.span(db)).accumulate(db);
                }
                self.check_expr(e2, &tp, false);
                (Type::Tuple(vec![]), false)
            }
            ExprData::Deref(e) => match self.infer_expr(e).0 {
                Type::Ptr(tp, is_mut) => (*tp, is_mut),
                _ => {
                    Diagnostic::cannot_dereference(db, e.span(db)).accumulate(db);
                    (Type::Error, true)
                }
            },
            ExprData::AddressOf(e) => {
                let (tp, is_mut) = self.infer_expr(e);
                (Type::Ptr(Box::new(tp), is_mut), false)
            }
            ExprData::Tuple(exprs) => {
                let tps = exprs.into_iter().map(|e| self.infer_expr(e).0).collect();
                (Type::Tuple(tps), false)
            }
            ExprData::Bool(_) => (Type::Bool, false),
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
                                self.check_expr(expr, &tp.1, false);
                            }
                            (Some(_), Some(_)) => {
                                Diagnostic::duplicate_field(db, e.span(db), field).accumulate(db);
                            }
                            (None, _) => {
                                Diagnostic::missing_field(db, e.span(db), field).accumulate(db);
                            }
                        }
                    }
                    Type::Var(tp_info.name.get_id())
                } else {
                    Diagnostic::unknown_type(db, e.span(db), ident).accumulate(db);
                    Type::Error
                };
                (tp, false)
            }
            ExprData::Field(expr, ident) => {
                let (tp, is_mut) = self.infer_expr(expr);
                match tp {
                    Type::Ptr(_, _)
                    | Type::Error
                    | Type::Int
                    | Type::Bool
                    | Type::Fn(_)
                    | Type::Tuple(_) => {
                        Diagnostic::no_field_on_type(db, e.span(db), ident, &tp).accumulate(db);
                        (Type::Error, true)
                    }
                    Type::Var(id) => {
                        if let Some(tp_info) = self.get_type_info(id) {
                            if let Some(tp) = tp_info.fields.get(&ident) {
                                (tp.1.clone(), is_mut)
                            } else {
                                Diagnostic::no_field_on_type(db, e.span(db), ident, &tp)
                                    .accumulate(db);
                                (Type::Error, true)
                            }
                        } else {
                            panic!()
                        }
                    }
                }
            }
        };
        self.type_map.insert(e, tp.clone());
        (tp, is_mut)
    }

    pub fn extend(&mut self, bindings: Vec<(Ident<'a>, VarBinding)>) {
        for (name, binding) in bindings {
            self.add_var(name, binding);
        }
    }

    pub fn check_expr(&mut self, e: ExprId<'a>, tp: &Type, exp_mut: bool) {
        let (tp_inferred, mut_inferred) = self.infer_expr(e);
        if !(tp_inferred.coerce_into(tp) && (!exp_mut || mut_inferred)) {
            Diagnostic::type_mismatch(self.db, e.span(self.db), tp, &tp_inferred)
                .accumulate(self.db);
        }
    }

    pub(crate) fn add_var(&mut self, arg: Ident<'a>, binding: VarBinding) {
        self.scopes.last_mut().unwrap().insert(arg, binding);
    }

    pub fn get_var(&self, x: Ident<'a>) -> Option<VarBinding> {
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
                    .map(Type::Fn)
                    .map(|tp| VarBinding { tp, is_mut: false })
            })
    }

    pub fn check_pat(&self, pat: PatternId<'a>, tp: &Type) -> Vec<(Ident<'a>, VarBinding)> {
        match pat.data(self.db) {
            PatternData::Wildcard => vec![],
            PatternData::Var(name, is_mut) => {
                vec![(
                    name,
                    VarBinding {
                        tp: tp.clone(),
                        is_mut,
                    },
                )]
            }
            PatternData::Tuple(pats) => {
                if let Type::Tuple(tps) = tp
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
pub struct VarBinding {
    tp: Type,
    is_mut: bool,
}
