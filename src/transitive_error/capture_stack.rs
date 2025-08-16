use std::sync::RwLock;

#[derive(Debug)]
pub(crate) struct ExceptionCaptureStack {
    inner: RwLock<Vec<Vec<String>>>,
    current_handler_exceptions: RwLock<Vec<Vec<String>>>,
}

impl Eq for ExceptionCaptureStack {}

impl PartialEq for ExceptionCaptureStack {
    fn eq(&self, other: &Self) -> bool {
        *self.inner.read().unwrap() == *other.inner.read().unwrap()
            && *self.current_handler_exceptions.read().unwrap()
                == *other.current_handler_exceptions.read().unwrap()
    }
}

impl std::hash::Hash for ExceptionCaptureStack {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.read().unwrap().hash(state);
        self.current_handler_exceptions.read().unwrap().hash(state);
    }
}

impl ExceptionCaptureStack {
    pub(crate) fn new() -> Self {
        Self {
            inner: RwLock::new(Vec::new()),
            current_handler_exceptions: RwLock::new(Vec::new()),
        }
    }

    pub(crate) fn push(&self, captured_exceptions: Vec<String>) {
        self.inner.write().unwrap().push(captured_exceptions);
    }

    pub(crate) fn pop(&self) {
        self.inner.write().unwrap().pop();
    }

    pub(crate) fn is_captured(&self, error: &String) -> bool {
        self.inner
            .read()
            .unwrap()
            .iter()
            .any(|es| es.iter().any(|e| e == "*ALL*" || e == error))
    }

    pub(crate) fn push_handler_exceptions(&self, exceptions: Vec<String>) {
        self.current_handler_exceptions
            .write()
            .unwrap()
            .push(exceptions);
    }

    pub(crate) fn pop_handler_exceptions(&self) {
        self.current_handler_exceptions.write().unwrap().pop();
    }

    pub(crate) fn get_current_handler_exceptions(&self) -> Option<Vec<String>> {
        self.current_handler_exceptions
            .read()
            .unwrap()
            .last()
            .cloned()
    }

    pub(crate) fn in_handler(&self) -> bool {
        !self.current_handler_exceptions.read().unwrap().is_empty()
    }
}
