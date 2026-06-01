use crate::{
    common::{Binop, Unop},
    input::Source,
};

#[salsa::tracked(debug)]
pub struct Span<'db> {
    #[tracked]
    pub start_byte: usize,
    #[tracked]
    pub end_byte: usize,
}

#[salsa::tracked(debug)]
pub struct File<'db> {
    pub defs: Vec<Def<'db>>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum Def<'db> {
    Fn(FnDef<'db>),
    Struct(StructDef<'db>),
}

#[salsa::tracked(debug)]
pub struct StructDef<'db> {
    pub name: Ident<'db>,
    pub span: Span<'db>,
    pub fields: Vec<(Ident<'db>, TypeExprId<'db>)>,
    pub sf: Source,
}

#[salsa::tracked(debug)]
pub struct FnDef<'db> {
    pub is_ext: bool,
    pub name: Ident<'db>,
    pub span: Span<'db>,
    pub args: Vec<(PatternId<'db>, TypeExprId<'db>)>,
    pub ret: Option<TypeExprId<'db>>,
    pub body: Option<ExprId<'db>>,
    pub sf: Source,
}

#[salsa::interned(debug)]
pub struct ExprId {
    pub data: ExprData<'db>,
    pub span: Span<'db>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum ExprData<'db> {
    Number(i32),
    Bool(bool),
    Binop(Binop, ExprId<'db>, ExprId<'db>),
    Unop(Unop, ExprId<'db>),
    Let(PatternId<'db>, ExprId<'db>, ExprId<'db>),
    Var(Ident<'db>),
    FnCall(Ident<'db>, Vec<ExprId<'db>>),
    If(ExprId<'db>, ExprId<'db>, Option<ExprId<'db>>),
    While(ExprId<'db>, ExprId<'db>),
    Assign(ExprId<'db>, ExprId<'db>),

    Deref(ExprId<'db>),
    AddressOf(ExprId<'db>),

    Tuple(Vec<ExprId<'db>>),
    Seq(ExprId<'db>, ExprId<'db>),

    Struct(Ident<'db>, Vec<(Ident<'db>, ExprId<'db>)>),
    Field(ExprId<'db>, Ident<'db>),

    Array(Vec<ExprId<'db>>),
    Index(ExprId<'db>, ExprId<'db>),

    Range(ExprId<'db>, ExprId<'db>),

    Error,
}

#[salsa::interned(debug)]
pub struct PatternId {
    pub data: PatternData<'db>,
    pub span: Span<'db>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum PatternData<'db> {
    Wildcard,
    Var(Ident<'db>, bool),
    Tuple(Vec<PatternId<'db>>),
}

#[salsa::interned(debug)]
pub struct TypeExprId {
    pub data: TypeExprData<'db>,
    pub span: Span<'db>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum TypeExprData<'db> {
    Int,
    Bool,
    Ptr(TypeExprId<'db>, bool),
    Fn(Vec<TypeExprId<'db>>, TypeExprId<'db>),
    Tuple(Vec<TypeExprId<'db>>),
    Var(Ident<'db>),
    Array(usize, TypeExprId<'db>),
    Slice(TypeExprId<'db>, bool),
}

#[salsa::interned(debug)]
pub struct Ident {
    #[returns(ref)]
    pub text: String,
}

impl<'db> Ident<'db> {
    pub fn get_id(&self) -> usize {
        self.0.as_bits() as usize
    }
}
