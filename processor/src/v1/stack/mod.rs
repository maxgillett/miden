use super::{BaseElement, ExecutionError, FieldElement, ProgramInputs, StackTrace, STACK_TOP_SIZE};
use core::{cmp, convert::TryInto};

// STACK
// ================================================================================================

/// TODO: add comments
pub struct Stack {
    step: usize,
    trace: StackTrace,
    overflow: Vec<BaseElement>,
    depth: usize,
}

impl Stack {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------
    /// TODO: add comments
    pub fn new(inputs: &ProgramInputs, init_trace_length: usize) -> Self {
        let init_values = inputs.stack_init();
        let mut trace: Vec<Vec<BaseElement>> = Vec::with_capacity(STACK_TOP_SIZE);
        for i in 0..STACK_TOP_SIZE {
            let mut column = vec![BaseElement::ZERO; init_trace_length];
            if i < init_values.len() {
                column[0] = init_values[i];
            }
            trace.push(column)
        }

        Self {
            step: 0,
            trace: trace.try_into().expect("failed to convert vector to array"),
            overflow: Vec::new(),
            depth: 0,
        }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns depth of the stack at the current step.
    #[allow(dead_code)]
    pub fn depth(&self) -> usize {
        self.depth
    }

    /// Returns the current step of the execution trace.
    #[allow(dead_code)]
    pub fn current_step(&self) -> usize {
        self.step
    }

    /// Returns execution trace length for this stack.
    #[allow(dead_code)]
    pub fn trace_length(&self) -> usize {
        self.trace[0].len()
    }

    /// Returns a copy of the item currently at the top of the stack.
    ///
    /// # Errors
    /// Returns an error if the stack is empty.
    pub fn peek(&self) -> Result<BaseElement, ExecutionError> {
        if self.depth == 0 {
            return Err(ExecutionError::StackUnderflow("peek", self.step));
        }

        Ok(self.trace[0][self.step])
    }

    /// Returns trace state at the current step.
    ///
    /// Trace state is always 16 elements long and contains the top 16 values of the stack. When
    /// the stack depth is less than 16, the un-used slots contain ZEROs.
    #[allow(dead_code)]
    pub fn trace_state(&self) -> [BaseElement; STACK_TOP_SIZE] {
        let mut result = [BaseElement::ZERO; STACK_TOP_SIZE];
        for (result, column) in result.iter_mut().zip(self.trace.iter()) {
            *result = column[self.step];
        }
        result
    }

    /// TODO: probably replace with into_trace()?
    pub fn trace(&self) -> &StackTrace {
        &self.trace
    }

    // TRACE ACCESSORS AND MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Returns the value located at the specified position on the stack at the current clock cycle.
    pub fn get(&self, pos: usize) -> BaseElement {
        debug_assert!(pos < self.depth, "stack underflow");
        self.trace[pos][self.step]
    }

    /// Sets the value at the specified position on the stack at the next clock cycle.
    pub fn set(&mut self, pos: usize, value: BaseElement) {
        debug_assert!(pos == 0 || pos < self.depth, "stack underflow");
        self.trace[pos][self.step + 1] = value;
    }

    /// Copies stack values starting at the specified position at the current clock cycle to the
    /// same position at the next clock cycle.
    pub fn copy_state(&mut self, start_pos: usize) {
        debug_assert!(
            start_pos < STACK_TOP_SIZE,
            "start cannot exceed stack top size"
        );
        debug_assert!(start_pos <= self.depth, "stack underflow");
        let end_pos = cmp::min(self.depth, STACK_TOP_SIZE);
        for i in start_pos..end_pos {
            self.trace[i][self.step + 1] = self.trace[i][self.step];
        }
    }

    /// Copies stack values starting at the specified position at the current clock cycle to
    /// position - 1 at the next clock cycle.
    ///
    /// If the stack depth is greater than 16, an item is moved from the overflow stack to the
    /// "in-memory" portion of the stack.
    ///
    /// # Panics
    /// Panics if the stack is empty.
    pub fn shift_left(&mut self, start_pos: usize) {
        debug_assert!(start_pos > 0, "start position must be greater than 0");
        debug_assert!(
            start_pos < STACK_TOP_SIZE,
            "start position cannot exceed stack top size"
        );
        debug_assert!(
            start_pos <= self.depth,
            "start position cannot exceed current depth"
        );

        match self.depth {
            0 => unreachable!("stack underflow"),
            1..=16 => {
                for i in start_pos..self.depth {
                    self.trace[i - 1][self.step + 1] = self.trace[i][self.step];
                }
            }
            _ => {
                for i in start_pos..STACK_TOP_SIZE {
                    self.trace[i - 1][self.step + 1] = self.trace[i][self.step];
                }
                let from_overflow = self.overflow.pop().expect("overflow stack is empty");
                self.trace[STACK_TOP_SIZE - 1][self.step + 1] = from_overflow;
            }
        }

        self.depth -= 1;
    }

    /// Copies stack values starting a the specified position at the current clock cycle to
    /// position + 1 at the next clock cycle
    ///
    /// If stack depth grows beyond 16 items, the additional item is pushed into the overflow
    /// stack.
    pub fn shift_right(&mut self, start_pos: usize) {
        debug_assert!(
            start_pos < STACK_TOP_SIZE,
            "start position cannot exceed stack top size"
        );
        debug_assert!(
            start_pos <= self.depth,
            "start position cannot exceed current depth"
        );

        const MAX_TOP_IDX: usize = STACK_TOP_SIZE - 1;
        match self.depth {
            0 => {} // if the stack is empty, do nothing
            1..=MAX_TOP_IDX => {
                for i in start_pos..self.depth {
                    self.trace[i + 1][self.step + 1] = self.trace[i][self.step];
                }
            }
            _ => {
                for i in start_pos..MAX_TOP_IDX {
                    self.trace[i + 1][self.step + 1] = self.trace[i][self.step];
                }
                let to_overflow = self.trace[MAX_TOP_IDX][self.step];
                self.overflow.push(to_overflow)
            }
        }

        self.depth += 1;
    }

    // Increments the clock cycle.
    pub fn advance_clock(&mut self) {
        self.step += 1;
    }

    pub fn finalize(&mut self) {
        for _ in self.step..self.trace_length() - 1 {
            self.copy_state(0);
            self.advance_clock();
        }
    }

    // UTILITY METHODS
    // --------------------------------------------------------------------------------------------

    /// Makes sure there is enough memory allocated for the trace to accommodate a new row.
    ///
    /// Trace length is doubled every time it needs to be increased.
    pub fn ensure_trace_capacity(&mut self) {
        if self.step + 1 >= self.trace_length() {
            let new_length = self.trace_length() * 2;
            for register in self.trace.iter_mut() {
                register.resize(new_length, BaseElement::ZERO);
            }
        }
    }

    /// Returns an error if the current stack depth is smaller than the specified required depth.
    ///
    /// The returned error includes the name of the operation (passed in as `op`) which triggered
    /// the check.
    pub fn check_depth(&self, req_depth: usize, op: &'static str) -> Result<(), ExecutionError> {
        if self.depth < req_depth {
            Err(ExecutionError::StackUnderflow(op, self.step))
        } else {
            Ok(())
        }
    }
}