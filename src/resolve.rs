use crate::{
    ast::{self, FnDef, Ident, Path, TypeExprId},
    diagnostic::Diagnostic,
    driver::get_child_sf,
    input::{self, Source},
    tp::{FnSig, TypeData, TypeId, TypeVar},
};
use salsa::{Accumulator, Database};
use std::collections::HashMap;

#[salsa::tracked]
pub(crate) fn parse_type_expr<'db>(
    db: &'db dyn Database,
    tp: TypeExprId<'db>,
    sf: Source,
) -> TypeId<'db> {
    let tp = match tp.data(db) {
        ast::TypeExprData::Primitive(tp) => TypeData::Primitive(tp),
        ast::TypeExprData::Fn(args, ret) => {
            let args = args
                .into_iter()
                .map(|arg| parse_type_expr(db, arg, sf))
                .collect();
            let ret = parse_type_expr(db, ret, sf);
            let fn_sig = FnSig { args, ret };

            TypeData::Fn(fn_sig)
        }
        ast::TypeExprData::Ptr(tp, is_mut) => {
            let tp = parse_type_expr(db, tp, sf);
            TypeData::Ptr(tp, is_mut)
        }
        ast::TypeExprData::Tuple(tps) => {
            let tps = tps
                .into_iter()
                .map(|tp| parse_type_expr(db, tp, sf))
                .collect();
            TypeData::Tuple(tps)
        }
        ast::TypeExprData::Var(id) => match get_item(db, sf, id) {
            Item::Type { tvar, .. } => TypeData::Var(tvar),
            _ => TypeData::Error,
        },
        ast::TypeExprData::Array(n, tp) => TypeData::Array(n, parse_type_expr(db, tp, sf)),
        ast::TypeExprData::Slice(tp, is_mut) => {
            let tp = parse_type_expr(db, tp, sf);
            TypeData::Slice(tp, is_mut)
        }
    };
    TypeId::new(db, tp)
}

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub struct ModuleDefs<'db> {
    pub items: HashMap<Ident<'db>, Item<'db>>,
}

#[salsa::tracked]
pub fn parse_fn_signature<'db>(db: &'db dyn Database, func: FnDef<'db>) -> FnSig<'db> {
    let sf = func.sf(db);
    let args = func
        .args(db)
        .into_iter()
        .map(|(_, tp)| parse_type_expr(db, tp, sf))
        .collect();
    let ret = if let Some(tp) = func.ret(db) {
        parse_type_expr(db, tp, sf)
    } else {
        TypeId::new(db, TypeData::Tuple(vec![]))
    };
    FnSig { args, ret }
}

pub fn get_defs<'db>(db: &'db dyn Database, sf: Source) -> ModuleDefs<'db> {
    let mut items = HashMap::new();

    let file = input::parse_file(db, sf);

    for def in file.defs(db) {
        match def {
            ast::Def::Fn(func) => {
                let full_name = if func.is_ext(db) {
                    func.name(db).text(db).clone()
                } else {
                    get_fn_full_name(db, sf, func)
                };
                items.insert(
                    func.name(db),
                    Item::Function {
                        def: func,
                        full_name,
                    },
                );
            }
            ast::Def::Struct(tp_def) => {
                let name = tp_def.name(db);

                let item = Item::Type {
                    tvar: TypeVar { sf, name },
                    def: tp_def,
                };
                items.insert(name, item);
            }
            ast::Def::ModuleDecl(name) => {
                items.insert(name, Item::Module);
            }
        }
    }

    let c = sf.c(db);
    let deps = c.dependencies(db);
    for (name, c) in deps {
        items.insert(Ident::new(db, name), Item::Crate(*c));
    }

    ModuleDefs { items }
}

pub fn get_fn_full_name<'db>(db: &'db dyn Database, sf: Source, func: FnDef<'db>) -> String {
    let full_name = if sf.module_path(db).is_empty() {
        func.name(db).text(db).clone()
    } else {
        format!(
            "{}::{}",
            sf.module_path(db).join("::"),
            func.name(db).text(db)
        )
    };
    full_name
}

#[derive(Clone, Debug, PartialEq, salsa::Update)]
pub enum Item<'db> {
    NotFound,
    Function {
        full_name: String,
        def: ast::FnDef<'db>,
    },
    Type {
        tvar: TypeVar<'db>,
        def: ast::StructDef<'db>,
    },
    Module,
    Crate(input::Crate),
}

#[salsa::tracked]
pub fn get_item<'db>(db: &'db dyn Database, s: Source, p: ast::Path<'db>) -> Item<'db> {
    match p.data(db)[..] {
        [] => Item::NotFound,
        [id] => {
            let def_map = get_defs(db, s);
            match def_map.items.get(&id.0) {
                Some(item) => item.clone(),
                None => {
                    Diagnostic::unbound_var(db, id.1, id.0, s).accumulate(db);
                    Item::NotFound
                }
            }
        }
        [name, ref rest @ ..] => {
            let def_map = get_defs(db, s);
            match def_map.items.get(&name.0) {
                Some(Item::Module) => {
                    let s = get_child_sf(db, s, name.0);
                    get_item(db, s, Path::new(db, rest))
                }
                Some(Item::Crate(c)) => {
                    let root = input::get_source(db, *c, vec![]).unwrap();
                    get_item(db, root, Path::new(db, rest))
                }
                _ => {
                    Diagnostic::unbound_var(db, name.1, name.0, s).accumulate(db);
                    Item::NotFound
                }
            }
        }
    }
}
