#[derive(Debug, Clone)]
pub(crate) struct CallStack(Vec<(String, String)>);

impl CallStack {
    pub(crate) fn new() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn push(&self, func: (String, String)) -> CallStack {
        let mut new_stack = Vec::new();
        new_stack.extend(self.0.clone());
        new_stack.extend_one(func);
        Self(new_stack)
    }

    pub(crate) fn contains(&self, func: &(String, String)) -> bool {
        self.0.contains(func)
    }
}

impl std::hash::Hash for CallStack {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        1.hash(state);
    }
}

impl PartialEq for CallStack {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for CallStack {}
