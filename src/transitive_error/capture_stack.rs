#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ExceptionCaptureStack {
    inner: Vec<Vec<String>>,
    current_handler_exceptions: Vec<Vec<String>>,
}

impl ExceptionCaptureStack {
    pub(crate) fn new() -> Self {
        Self {
            inner: Vec::new(),
            current_handler_exceptions: Vec::new(),
        }
    }

    pub(crate) fn push(&self, captured_exceptions: Vec<String>) -> Self {
        let mut new_inner = self.inner.clone();
        new_inner.push(captured_exceptions);
        Self {
            inner: new_inner,
            current_handler_exceptions: self.current_handler_exceptions.clone(),
        }
    }

    pub(crate) fn pop(&self) -> Self {
        let mut new_inner = self.inner.clone();
        new_inner.pop();
        Self {
            inner: new_inner,
            current_handler_exceptions: self.current_handler_exceptions.clone(),
        }
    }

    pub(crate) fn is_captured(&self, error: &String) -> bool {
        self.inner
            .iter()
            .any(|es| es.iter().any(|e| e == "*ALL*" || e == error))
    }

    pub(crate) fn push_handler_exceptions(&self, exceptions: Vec<String>) -> Self {
        let mut new_handler_exceptions = self.current_handler_exceptions.clone();
        new_handler_exceptions.push(exceptions);
        Self {
            inner: self.inner.clone(),
            current_handler_exceptions: new_handler_exceptions,
        }
    }

    pub(crate) fn pop_handler_exceptions(&self) -> Self {
        let mut new_handler_exceptions = self.current_handler_exceptions.clone();
        new_handler_exceptions.pop();
        Self {
            inner: self.inner.clone(),
            current_handler_exceptions: new_handler_exceptions,
        }
    }

    pub(crate) fn get_current_handler_exceptions(&self) -> Option<Vec<String>> {
        self.current_handler_exceptions.last().cloned()
    }

    pub(crate) fn in_handler(&self) -> bool {
        !self.current_handler_exceptions.is_empty()
    }
}