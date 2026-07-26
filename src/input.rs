use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use salsa::{Accumulator, Database};

use crate::{ast::File, diagnostic::Diagnostic, parser};

#[salsa::input(debug)]
pub struct Source {
    #[returns(ref)]
    pub text: String,
    #[returns(ref)]
    pub file_name: PathBuf,
}

#[salsa::tracked]
pub fn parse_file<'db>(db: &'db dyn Database, source: Source) -> File<'db> {
    let parser = parser::FileParser::new();
    match parser.parse(db, source, source.text(db)) {
        Ok(file) => file,
        Err(e) => {
            Diagnostic::parser_error(e, source).accumulate(db);
            File::new(db, vec![])
        }
    }
}

pub fn resolve_import<'db>(db: &'db dyn Database, sf: Source, path: String) -> Option<Source> {
    // let root = c.root_dir(db);
    // let physical_path = resolve_module_path(root, &path)?;

    // let text = std::fs::read_to_string(&physical_path).ok()?;

    // Some(Source::new(db, c, text, path, physical_path))
    todo!()
}

/// Resolves a logical module path (e.g., ["ast", "parser"]) into a physical file path.
pub fn resolve_module_path(root: &Path, module_path: &[String]) -> Option<PathBuf> {
    let root = root.join("src/");
    if module_path.is_empty() {
        let default_root = root.join("mod.must");
        return if default_root.exists() {
            Some(default_root)
        } else {
            None
        };
    }

    // Build the base directory (everything BEFORE the actual module name)
    // E.g., if path is ["frontend", "math"], base_dir becomes "root/frontend"
    let mut base_dir = root.to_path_buf();
    for component in &module_path[..module_path.len() - 1] {
        base_dir.push(component);
    }

    let mod_name = module_path.last().unwrap();

    // Option A: root/frontend/math.must
    let file_path = base_dir.join(format!("{}.must", mod_name));

    // Option B: root/frontend/math/mod.must
    let dir_path = base_dir.join(mod_name).join("mod.must");

    if file_path.exists() {
        Some(file_path)
    } else if dir_path.exists() {
        Some(dir_path)
    } else {
        None
    }
}

/// TODO: enable reporting string errors through the parser.
/// Now it will panic if this function doesn't succeed
pub fn unescape_json_string(s: &str) -> Result<String, String> {
    // Strip surrounding quotes
    let raw = &s[1..s.len() - 1];

    let mut result = String::new();
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('/') => result.push('/'),
                Some('b') => result.push('\u{0008}'),
                Some('f') => result.push('\u{000C}'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('u') => {
                    // Expect 4 hex digits
                    let code: String = chars.by_ref().take(4).collect();
                    if code.len() == 4 {
                        if let Ok(num) = u16::from_str_radix(&code, 16) {
                            if let Some(ch) = char::from_u32(num as u32) {
                                result.push(ch);
                            } else {
                                return Err(format!("Invalid unicode escape: {}", code));
                            }
                        } else {
                            return Err(format!("Bad hex in unicode escape: {}", code));
                        }
                    } else {
                        return Err("Incomplete unicode escape".into());
                    }
                }
                Some(other) => return Err(format!("Invalid escape: \\{}", other)),
                None => return Err("Incomplete escape".into()),
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}
