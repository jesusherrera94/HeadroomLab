// src/application/counter_service.rs
use crate::application::ports::CounterRepository;

pub struct CounterService<R: CounterRepository> {
    repo: R,
}

impl<R: CounterRepository> CounterService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
    pub fn increase(&self) -> i32 {
        let mut counter = self.repo.load();
        counter.increase();
        self.repo.save(&counter);
        counter.value()
    }
}
