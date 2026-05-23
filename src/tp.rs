use std::collections::HashMap;

use salsa::{Accumulator, Database};

use crate::{
    ast::{ExprData, ExprId, Ident, PatternData, PatternId, Span},
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
    var_map: HashMap<Ident<'a>, VarBinding>,
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

    pub fn cannot_assign(db: &dyn Database, span: Span) -> Self {
        Diagnostic::error(db, span, format!("this expression cannot be mutated",))
    }
}

impl<'a> Env<'a> {
    pub fn new(db: &'a dyn Database, function_defs: &'a HashMap<Ident<'a>, FnSig>) -> Self {
        let var_map = HashMap::new();
        Self {
            var_map,
            function_defs,
            db,
        }
    }

    pub fn infer_expr(&mut self, e: ExprId<'a>) -> (Type, bool) {
        match e.data(self.db) {
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
                let bindings = self.check_pat(pat, tp1);
                self.extend(bindings);
                self.infer_expr(e2)
            }
            ExprData::Var(x) => match self.get_var(x) {
                Some(VarBinding { tp, is_mut }) => (tp.clone(), is_mut),
                None => {
                    Diagnostic::unbound_var(self.db, e.span(self.db), x.text(self.db))
                        .accumulate(self.db);
                    (Type::Error, true)
                }
            },
            ExprData::FnCall(fn_name, exprs) => {
                let sig = match self.function_defs.get(&fn_name) {
                    Some(sig) => sig,
                    None => return (Type::Error, true),
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
                    self.check_expr(e, exp_tp, false);
                }
                if let Some(tp) = tp_args.next() {
                    Diagnostic::missing_argument(self.db, id, e.span(self.db), tp.clone())
                        .accumulate(self.db);
                }
                (*sig.ret.clone(), false)
            }
            ExprData::Error => (Type::Error, true),
            ExprData::If(cond, th, el) => {
                self.check_expr(cond, &Type::Bool, false);
                let (tp, is_mut) = self.infer_expr(th);
                self.check_expr(el, &tp, is_mut);
                (tp, is_mut)
            }
            ExprData::While(cond, body) => {
                self.check_expr(cond, &Type::Bool, false);
                self.infer_expr(body);
                (Type::Int, false)
            }
            ExprData::Assign(e1, e2) => {
                let (tp, is_mut) = self.infer_expr(e1);
                if !is_mut {
                    Diagnostic::cannot_assign(self.db, e1.span(self.db)).accumulate(self.db);
                }
                self.check_expr(e2, &tp, false);
                (Type::Bool, false)
            }
        }
    }

    pub fn extend(&mut self, bindings: Vec<(Ident<'a>, VarBinding)>) {
        for (name, binding) in bindings {
            self.add_var(name, binding);
        }
    }

    // p -> q
    // ~p ∨ q
    pub fn check_expr(&mut self, e: ExprId<'a>, tp: &Type, exp_mut: bool) {
        let (tp_inferred, mut_inferred) = self.infer_expr(e);
        if !(tp_inferred == *tp && (!exp_mut || mut_inferred)) {
            Diagnostic::type_mismatch(self.db, e.span(self.db), tp.clone(), tp_inferred)
                .accumulate(self.db);
        }
    }

    pub(crate) fn add_var(&mut self, arg: Ident<'a>, binding: VarBinding) {
        self.var_map.insert(arg, binding);
    }

    pub fn get_var(&self, x: Ident<'a>) -> Option<VarBinding> {
        self.var_map.get(&x).cloned().or_else(|| {
            self.function_defs
                .get(&x)
                .cloned()
                .map(Type::Fn)
                .map(|tp| VarBinding { tp, is_mut: false })
        })
    }

    // pub fn extend(&mut self, v: Vec<(I))

    pub fn check_pat(&self, pat: PatternId<'a>, tp: Type) -> Vec<(Ident<'a>, VarBinding)> {
        match pat.data(self.db) {
            PatternData::Wildcard => vec![],
            PatternData::Var(name, is_mut) => {
                vec![(name, VarBinding { tp, is_mut })]
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct VarBinding {
    tp: Type,
    is_mut: bool,
}
