pub trait AudioProcessor: Send {
    fn process(&mut self, samples: &mut [f32], channels: u16, sample_rate: u32);
    fn reset(&mut self);
}
