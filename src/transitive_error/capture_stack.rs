#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ExceptionCaptureStack {
    try_except_capture_stack: Vec<Vec<String>>,
    current_handler_exceptions: Vec<Vec<String>>,
}

impl ExceptionCaptureStack {
    pub(crate) fn new() -> Self {
        Self {
            try_except_capture_stack: Vec::new(),
            current_handler_exceptions: Vec::new(),
        }
    }

    pub(crate) fn push(&self, captured_exceptions: Vec<String>) -> Self {
        let mut new_inner = self.try_except_capture_stack.clone();
        new_inner.push(captured_exceptions);
        Self {
            try_except_capture_stack: new_inner,
            current_handler_exceptions: self.current_handler_exceptions.clone(),
        }
    }

    pub(crate) fn pop(&self) -> Self {
        let mut new_inner = self.try_except_capture_stack.clone();
        new_inner.pop();
        Self {
            try_except_capture_stack: new_inner,
            current_handler_exceptions: self.current_handler_exceptions.clone(),
        }
    }

    pub(crate) fn is_captured(&self, error: &String) -> bool {
        self.try_except_capture_stack
            .iter()
            .any(|es| es.iter().any(|e| e == "*ALL*" || e == error))
    }

    pub(crate) fn push_handler_exceptions(&self, exceptions: Vec<String>) -> Self {
        let mut new_handler_exceptions = self.current_handler_exceptions.clone();
        new_handler_exceptions.push(exceptions);
        Self {
            try_except_capture_stack: self.try_except_capture_stack.clone(),
            current_handler_exceptions: new_handler_exceptions,
        }
    }

    pub(crate) fn pop_handler_exceptions(&self) -> Self {
        let mut new_handler_exceptions = self.current_handler_exceptions.clone();
        new_handler_exceptions.pop();
        Self {
            try_except_capture_stack: self.try_except_capture_stack.clone(),
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
