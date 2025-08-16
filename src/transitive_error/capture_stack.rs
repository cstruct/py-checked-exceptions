pub(crate) struct ExceptionCaptureStack {
    inner: Vec<Vec<String>>,
}

impl ExceptionCaptureStack {
    pub(crate) fn new() -> Self {
        Self { inner: Vec::new() }
    }
    pub(crate) fn push(&mut self, captured_exceptions: Vec<String>) {
        self.inner.push(captured_exceptions);
    }
    pub(crate) fn pop(&mut self) {
        self.inner.pop();
    }
    pub(crate) fn is_captured(&self, error: &String) -> bool {
        self.inner.iter().any(|es| es.iter().any(|e| e == error))
    }
}
