use salsa::Database;

use crate::ast::{ExprData, ExprId};

#[derive(Debug, Clone)]
pub enum Value {
    UInt(u64),
}
impl Value {
    pub(crate) fn as_bytes(&self) -> Box<[u8]> {
        match self {
            Value::UInt(n) => Box::new(n.to_le_bytes()),
        }
    }
}

pub fn eval_expr(db: &dyn Database, e: ExprId) -> Value {
    match e.data(db) {
        ExprData::Int(n) => Value::UInt(n),
        ExprData::Float(_str_lit) => todo!(),
        ExprData::Bool(_) => todo!(),
        ExprData::Char(_) => todo!(),
        ExprData::Str(_str_lit) => todo!(),
        ExprData::Binop(_binop, _expr_id, _expr_id1) => todo!(),
        ExprData::Unop(_unop, _expr_id) => todo!(),
        ExprData::Let(_pattern_id, _expr_id, _expr_id1) => todo!(),
        ExprData::Var(_ident) => todo!(),
        ExprData::FnCall(_expr_id, _expr_ids) => todo!(),
        ExprData::If(_expr_id, _expr_id1, _expr_id2) => todo!(),
        ExprData::While(_expr_id, _expr_id1) => todo!(),
        ExprData::Assign(_expr_id, _expr_id1) => todo!(),
        ExprData::Deref(_expr_id) => todo!(),
        ExprData::AddressOf(_expr_id) => todo!(),
        ExprData::Tuple(_expr_ids) => todo!(),
        ExprData::Seq(_expr_id, _expr_id1) => todo!(),
        ExprData::Struct(_ident, _items) => todo!(),
        ExprData::Field(_expr_id, _ident) => todo!(),
        ExprData::Array(_expr_ids) => todo!(),
        ExprData::Index(_expr_id, _expr_id1) => todo!(),
        ExprData::Range(_expr_id, _expr_id1) => todo!(),
        ExprData::Cast(e, _) => eval_expr(db, e),
        ExprData::Error => todo!(),
    }
}
