use salsa::Database;

use crate::ast::{ExprData, ExprId};

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    UInt(u64),
}
impl Value {
    pub(crate) fn as_bytes(&self) -> Box<[u8]> {
        match self {
            Value::Int(n) => Box::new(n.to_le_bytes()),
            Value::UInt(n) => Box::new(n.to_le_bytes()),
        }
    }
}

pub fn eval_expr(db: &dyn Database, e: ExprId) -> Value {
    match e.data(db) {
        ExprData::Int(n) => Value::UInt(n),
        ExprData::Float(str_lit) => todo!(),
        ExprData::Bool(_) => todo!(),
        ExprData::Char(_) => todo!(),
        ExprData::Str(str_lit) => todo!(),
        ExprData::Binop(binop, expr_id, expr_id1) => todo!(),
        ExprData::Unop(unop, expr_id) => todo!(),
        ExprData::Let(pattern_id, expr_id, expr_id1) => todo!(),
        ExprData::Var(ident) => todo!(),
        ExprData::FnCall(expr_id, expr_ids) => todo!(),
        ExprData::If(expr_id, expr_id1, expr_id2) => todo!(),
        ExprData::While(expr_id, expr_id1) => todo!(),
        ExprData::Assign(expr_id, expr_id1) => todo!(),
        ExprData::Deref(expr_id) => todo!(),
        ExprData::AddressOf(expr_id) => todo!(),
        ExprData::Tuple(expr_ids) => todo!(),
        ExprData::Seq(expr_id, expr_id1) => todo!(),
        ExprData::Struct(ident, items) => todo!(),
        ExprData::Field(expr_id, ident) => todo!(),
        ExprData::Array(expr_ids) => todo!(),
        ExprData::Index(expr_id, expr_id1) => todo!(),
        ExprData::Range(expr_id, expr_id1) => todo!(),
        ExprData::Cast(e, _) => eval_expr(db, e),
        ExprData::Error => todo!(),
    }
}
