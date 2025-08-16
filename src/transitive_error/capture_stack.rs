use std::sync::RwLock;

#[derive(Debug)]
pub(crate) struct ExceptionCaptureStack {
    inner: RwLock<Vec<Vec<String>>>,
}

impl Eq for ExceptionCaptureStack {}

impl PartialEq for ExceptionCaptureStack {
    fn eq(&self, other: &Self) -> bool {
        *self.inner.read().unwrap() == *other.inner.read().unwrap()
    }
}

impl std::hash::Hash for ExceptionCaptureStack {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.read().unwrap().hash(state);
    }
}

impl ExceptionCaptureStack {
    pub(crate) fn new() -> Self {
        Self { inner: RwLock::new(Vec::new()) }
    }

    pub(crate) fn push(&self, captured_exceptions: Vec<String>) {
        self.inner.write().unwrap().push(captured_exceptions);
    }

    pub(crate) fn pop(&self) {
        self.inner.write().unwrap().pop();
    }
    pub(crate) fn is_captured(&self, error: &String) -> bool {
        self.inner.read().unwrap().iter().any(|es| es.iter().any(|e| e == error))
    }
}
