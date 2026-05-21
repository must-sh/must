use std::collections::HashMap;

use salsa::Database;

use crate::ast::{ExprData, ExprId, Ident};

#[derive(Debug, PartialEq, Eq, Clone, salsa::Update)]
pub enum Type {
    Int,
    Error,
    Fn(FnSig),
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

impl<'a> Env<'a> {
    pub fn new(db: &'a dyn Database, function_defs: &'a HashMap<Ident<'a>, FnSig>) -> Self {
        let type_map = HashMap::new();
        Self {
            type_map,
            function_defs,
            db,
        }
    }

    pub fn infer_expr(&mut self, e: ExprId<'a>) -> Option<Type> {
        match e.data(self.db) {
            ExprData::Number(_) => Some(Type::Int),
            ExprData::Add(expr, expr1)
            | ExprData::Sub(expr, expr1)
            | ExprData::Mul(expr, expr1)
            | ExprData::Div(expr, expr1) => {
                self.check_expr(expr, &Type::Int)?;
                self.check_expr(expr1, &Type::Int)?;
                Some(Type::Int)
            }
            ExprData::Let(x, e1, e2) => {
                let tp1 = self.infer_expr(e1)?;
                self.type_map.insert(x, tp1);
                self.infer_expr(e2)
            }
            ExprData::Var(x) => self.type_map.get(&x).cloned(),
            ExprData::FnCall(fn_name, exprs) => {
                let sig = self.function_defs.get(&fn_name)?;
                let mut tp_args = sig.args.iter();
                for e in exprs {
                    self.check_expr(e, tp_args.next()?)?;
                }
                if let Some(_) = tp_args.next() {
                    return None;
                }
                Some(*sig.ret.clone())
            }
            ExprData::Error => Some(Type::Error),
        }
    }

    pub fn check_expr(&mut self, e: ExprId<'a>, tp: &Type) -> Option<()> {
        let tp_inferred = self.infer_expr(e)?;
        if tp_inferred == *tp { Some(()) } else { None }
    }

    pub(crate) fn add_var(&mut self, arg: Ident<'a>, tp: Type) {
        self.type_map.insert(arg, tp);
    }
}
