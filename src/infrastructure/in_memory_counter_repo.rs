// src/infrastructure/in_memory_counter_repo.rs
use std::cell::RefCell;
use crate::application::ports::CounterRepository;
use crate::domain::counter::Counter;
pub struct InMemoryCounterRepo {
    counter: RefCell<Counter>,
}
impl InMemoryCounterRepo {
    pub fn new(initial: i32) -> Self {
        Self { counter: RefCell::new(Counter::new(initial)) }
    }
}
impl CounterRepository for InMemoryCounterRepo {
    fn load(&self) -> Counter {
        Counter::new(self.counter.borrow().value())
    }
    fn save(&self, counter: &Counter) {
        *self.counter.borrow_mut() = Counter::new(counter.value());
    }
}
