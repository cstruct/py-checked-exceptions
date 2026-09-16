use ruff_db::{files::File, source::source_text};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;
use ty_project::Db;

#[derive(Debug, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct Exception {
    pub name: String,
    pub bases: Vec<Exception>,
}

impl Exception {
    pub fn new(name: String, bases: Vec<Exception>) -> Self {
        Self { name, bases }
    }

    pub fn is_subclass_of(&self, other: &Exception) -> bool {
        if self == other {
            return true;
        }

        for base in &self.bases {
            if base.is_subclass_of(other) {
                return true;
            }
        }

        false
    }

    pub(crate) fn with_type_arguments(self, arguments: String) -> Self {
        Self {
            name: format!("{}[{arguments}]", self.name),
            bases: vec![self],
        }
    }

    pub fn base_exception() -> Self {
        Self {
            name: "BaseException".to_string(),
            bases: vec![],
        }
    }
}

pub(crate) fn canonical_exception_expression(db: &dyn Db, file: File, expression: &Expr) -> String {
    let source = source_text(db, file);
    remove_whitespace(&source[expression.range()])
}

pub(crate) fn canonical_exception_name(name: &str) -> String {
    if !name.contains('[') {
        return name.trim().to_string();
    }
    remove_whitespace(name)
}

fn remove_whitespace(name: &str) -> String {
    name.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::canonical_exception_name;

    #[test]
    fn canonicalizes_generic_exception_names() {
        assert_eq!(
            canonical_exception_name(" NotFoundError[ User, Group ] "),
            "NotFoundError[User,Group]"
        );
    }

    #[test]
    fn does_not_rewrite_unspecialized_exception_names() {
        assert_eq!(canonical_exception_name(" Value Error "), "Value Error");
    }
}
