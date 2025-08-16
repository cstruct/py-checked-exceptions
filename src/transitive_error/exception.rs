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

    pub fn base_exception() -> Self {
        Self {
            name: "BaseException".to_string(),
            bases: vec![],
        }
    }
}
