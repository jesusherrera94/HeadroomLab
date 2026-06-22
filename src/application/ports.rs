// src/application/ports.rs
use crate::domain::counter::Counter;
// Port: the application depends on this abstraction, not a concrete DB/file.
pub trait CounterRepository {
    fn load(&self) -> Counter;
    fn save(&self, counter: &Counter);
}