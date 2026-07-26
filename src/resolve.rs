use crate::{
    ast::{self, FnDef, Ident},
    driver::get_child_sf,
    input::{self, Source},
    tp::{FnSig, TypeData, TypeId},
};
use salsa::{Accumulator, Database};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub struct ModuleDefs<'db> {
    pub items: HashMap<Ident<'db>, Item<'db>>,
}

#[salsa::tracked]
pub fn parse_fn_signature<'db>(db: &'db dyn Database, func: FnDef<'db>) -> FnSig<'db> {
    let args = func.args(db).into_iter().map(|(_, tp)| tp).collect();
    let ret = if let Some(tp) = func.ret(db) {
        tp
    } else {
        TypeId::new(db, TypeData::Tuple(vec![]))
    };
    FnSig { args, ret }
}

#[salsa::tracked]
fn get_defs<'db>(db: &'db dyn Database, sf: Source) -> ModuleDefs<'db> {
    let mut items = HashMap::new();

    let file = input::parse_file(db, sf);

    for def in file.defs(db) {
        match def {
            ast::Def::Fn(func) => {
                let full_name = func.name(db).text(db).clone();
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

                let item = Item::Type { def: tp_def };
                items.insert(name, item);
            }
            ast::Def::Import(name) => {
                // 1. resolve mod filename
                // 2. create source file
                // 3. get defs for this source
                // 4. append to items
                let sf = get_child_sf(db, sf, name);
                let defs = get_defs(db, sf);
                items.extend(defs.items);
            }
        }
    }

    ModuleDefs { items }
}

#[derive(Clone, Debug, PartialEq, salsa::Update)]
pub enum Item<'db> {
    Function {
        full_name: String,
        def: ast::FnDef<'db>,
    },
    Type {
        def: ast::StructDef<'db>,
    },
}

#[salsa::tracked]
pub(crate) fn get_item<'db>(
    db: &'db dyn Database,
    sf: Source,
    name: Ident<'db>,
) -> Option<Item<'db>> {
    get_defs(db, sf).items.get(&name).cloned()
}
