use std::collections::HashMap;

use salsa::{Accumulator, Database};

use crate::{
    ast::{ExprData, ExprId, Ident, Span},
    diagnostic::Diagnostic,
};

#[derive(Debug, Eq, Clone, salsa::Update)]
pub enum Type {
    Error,

    Int,
    Bool,
    Fn(FnSig),
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (_, Type::Error) | (Type::Error, _) => true,
            (Type::Int, Type::Int) => true,
            (Type::Bool, Type::Bool) => true,
            (Self::Fn(l0), Self::Fn(r0)) => l0 == r0,
            _ => false,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, salsa::Update)]
pub struct FnSig {
    pub args: Vec<Type>,
    pub ret: Box<Type>,
}

pub struct Env<'a> {
    type_map: HashMap<Ident<'a>, Type>,
    function_defs: &'a HashMap<Ident<'a>, FnSig>,
    db: &'a dyn Database,
}

impl Diagnostic {
    pub fn type_mismatch(db: &dyn Database, span: Span, exp: Type, got: Type) -> Diagnostic {
        Diagnostic::error(
            db,
            span,
            format!("type mismatch. expected: {:?}, got: {:?}", exp, got),
        )
    }

    pub fn missing_argument(db: &dyn Database, id: usize, span: Span, tp: Type) -> Self {
        Diagnostic::error(db, span, format!("missing arg #{} of type {:?}", id, tp))
    }

    pub fn unexpected_argument(db: &dyn Database, id: usize, span: Span) -> Self {
        Diagnostic::error(db, span, format!("unexpected arg #{}", id))
    }

    pub fn unbound_var(db: &dyn Database, span: Span, name: &str) -> Self {
        Diagnostic::error(db, span, format!("unbound var: {:?}", name))
    }
}

impl<'a> Env<'a> {
    pub fn new(db: &'a dyn Database, function_defs: &'a HashMap<Ident<'a>, FnSig>) -> Self {
        let type_map = HashMap::new();
        Self {
            type_map,
            function_defs,
            db,
        }
    }

    pub fn infer_expr(&mut self, e: ExprId<'a>) -> Type {
        match e.data(self.db) {
            ExprData::Number(_) => Type::Int,
            ExprData::Binop(op, expr, expr1) => {
                use crate::common::Op::*;
                match op {
                    Add | Sub | Mul | Div => {
                        self.check_expr(expr, &Type::Int);
                        self.check_expr(expr1, &Type::Int);
                        Type::Int
                    }
                    Eq => {
                        self.check_expr(expr, &Type::Int);
                        self.check_expr(expr1, &Type::Int);
                        Type::Bool
                    }
                }
            }
            ExprData::Let(x, e1, e2) => {
                let tp1 = self.infer_expr(e1);
                self.type_map.insert(x, tp1);
                self.infer_expr(e2)
            }
            ExprData::Var(x) => match self.get_var(x) {
                Some(tp) => tp.clone(),
                None => {
                    Diagnostic::unbound_var(self.db, e.span(self.db), x.text(self.db))
                        .accumulate(self.db);
                    Type::Error
                }
            },
            ExprData::FnCall(fn_name, exprs) => {
                let sig = match self.function_defs.get(&fn_name) {
                    Some(sig) => sig,
                    None => return Type::Error,
                };
                let mut tp_args = sig.args.iter();
                let mut id = 0;
                for e in exprs {
                    id += 1;
                    let exp_tp = match tp_args.next() {
                        Some(tp) => tp,
                        None => {
                            Diagnostic::unexpected_argument(self.db, id, e.span(self.db))
                                .accumulate(self.db);
                            continue;
                        }
                    };
                    self.check_expr(e, exp_tp);
                }
                if let Some(tp) = tp_args.next() {
                    Diagnostic::missing_argument(self.db, id, e.span(self.db), tp.clone())
                        .accumulate(self.db);
                }
                *sig.ret.clone()
            }
            ExprData::Error => Type::Error,
            ExprData::If(cond, th, el) => {
                self.check_expr(cond, &Type::Bool);
                let tp = self.infer_expr(th);
                self.check_expr(el, &tp);
                tp
            }
            ExprData::While(cond, body) => {
                self.check_expr(cond, &Type::Bool);
                self.infer_expr(body);
                Type::Int
            }
        }
    }

    pub fn check_expr(&mut self, e: ExprId<'a>, tp: &Type) {
        let tp_inferred = self.infer_expr(e);
        if !(tp_inferred == *tp) {
            Diagnostic::type_mismatch(self.db, e.span(self.db), tp.clone(), tp_inferred)
                .accumulate(self.db);
        }
    }

    pub(crate) fn add_var(&mut self, arg: Ident<'a>, tp: Type) {
        self.type_map.insert(arg, tp);
    }

    pub fn get_var(&self, x: Ident<'a>) -> Option<Type> {
        self.type_map
            .get(&x)
            .cloned()
            .or_else(|| self.function_defs.get(&x).cloned().map(Type::Fn))
    }
}
