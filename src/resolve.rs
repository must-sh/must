use crate::{
    ast::{self, FnDef, Ident, TypeExprId},
    input::{self, Source},
    tp::{FnSig, TypeData, TypeId, TypeInfo},
};
use salsa::Database;
use std::collections::HashMap;

#[salsa::tracked]
pub(crate) fn parse_type_expr<'db>(db: &'db dyn Database, tp: TypeExprId<'db>) -> TypeId<'db> {
    let tp = match tp.data(db) {
        ast::TypeExprData::Int => TypeData::Int,
        ast::TypeExprData::Bool => TypeData::Bool,
        ast::TypeExprData::Fn(args, ret) => {
            let args = args
                .into_iter()
                .map(|arg| parse_type_expr(db, arg))
                .collect();
            let ret = parse_type_expr(db, ret);
            let fn_sig = FnSig { args, ret };

            TypeData::Fn(fn_sig)
        }
        ast::TypeExprData::Ptr(tp, is_mut) => {
            let tp = parse_type_expr(db, tp);
            TypeData::Ptr(tp, is_mut)
        }
        ast::TypeExprData::Tuple(tps) => {
            let tps = tps.into_iter().map(|tp| parse_type_expr(db, tp)).collect();
            TypeData::Tuple(tps)
        }
        ast::TypeExprData::Var(id) => TypeData::Var(id.get_id()),
        ast::TypeExprData::Array(n, tp) => TypeData::Array(n, parse_type_expr(db, tp)),
        ast::TypeExprData::Slice(tp, is_mut) => {
            let tp = parse_type_expr(db, tp);
            TypeData::Slice(tp, is_mut)
        }
    };
    TypeId::new(db, tp)
}

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub struct ModuleDefs<'db> {
    pub function_map: HashMap<Ident<'db>, FnSig<'db>>,
    pub type_map: HashMap<usize, TypeInfo<'db>>,
}

#[salsa::tracked]
pub fn parse_fn_signature<'db>(db: &'db dyn Database, func: FnDef<'db>) -> FnSig<'db> {
    let args = func
        .args(db)
        .into_iter()
        .map(|(_, tp)| parse_type_expr(db, tp))
        .collect();
    let ret = if let Some(tp) = func.ret(db) {
        parse_type_expr(db, tp)
    } else {
        TypeId::new(db, TypeData::Tuple(vec![]))
    };
    FnSig { args, ret }
}

pub fn get_defs<'db>(db: &'db dyn Database, sf: Source) -> ModuleDefs<'db> {
    let mut function_map = HashMap::new();
    let mut type_map = HashMap::new();

    let file = input::parse_file(db, sf);

    for def in file.defs(db) {
        match def {
            ast::Def::Fn(func) => {
                let sig = parse_fn_signature(db, func);
                function_map.insert(func.name(db), sig);
            }
            ast::Def::Struct(tp_def) => {
                let name = tp_def.name(db);
                let info = TypeInfo {
                    name,
                    fields: tp_def
                        .fields(db)
                        .into_iter()
                        .enumerate()
                        .map(|(id, (name, tp))| (name, (id, parse_type_expr(db, tp))))
                        .collect(),
                };
                type_map.insert(name.get_id(), info);
            }
        }
    }

    ModuleDefs {
        function_map,
        type_map,
    }
}
