use std::collections::HashMap;

use crate::ast::Expr;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Type {
    Int,
    Fn(FnSig),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FnSig {
    pub args: Vec<Type>,
    pub ret: Box<Type>,
}

pub struct Env<'a> {
    type_map: HashMap<String, Type>,
    function_defs: &'a HashMap<String, FnSig>,
}

impl<'a> Env<'a> {
    pub fn new(function_defs: &'a HashMap<String, FnSig>) -> Self {
        let type_map = HashMap::new();
        Self {
            type_map,
            function_defs,
        }
    }

    pub fn infer_expr(&mut self, e: &Expr) -> Option<Type> {
        match e {
            Expr::Number(_) => Some(Type::Int),
            Expr::Add(expr, expr1)
            | Expr::Sub(expr, expr1)
            | Expr::Mul(expr, expr1)
            | Expr::Div(expr, expr1) => {
                self.check_expr(expr, &Type::Int)?;
                self.check_expr(expr1, &Type::Int)?;
                Some(Type::Int)
            }
            Expr::Let(x, e1, e2) => {
                let tp1 = self.infer_expr(e1)?;
                self.type_map.insert(x.clone(), tp1);
                self.infer_expr(e2)
            }
            Expr::Var(x) => self.type_map.get(x).cloned(),
            Expr::FnCall(fn_name, exprs) => {
                let sig = self.function_defs.get(fn_name)?;
                let mut tp_args = sig.args.iter();
                for e in exprs {
                    self.check_expr(e, tp_args.next()?)?;
                }
                if let Some(_) = tp_args.next() {
                    return None;
                }
                Some(*sig.ret.clone())
            }
        }
    }

    pub fn check_expr(&mut self, e: &Expr, tp: &Type) -> Option<()> {
        let tp_inferred = self.infer_expr(e)?;
        if tp_inferred == *tp { Some(()) } else { None }
    }

    pub(crate) fn add_var(&mut self, arg: String, tp: Type) {
        self.type_map.insert(arg, tp);
    }
}
