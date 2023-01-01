pub struct MockRand {
    results: Vec<u64>,
}

impl rand::RngCore for MockRand {
    fn next_u64(&mut self) -> u64 {
        self.results.remove(0)
    }

    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    fn fill_bytes(&mut self, _: &mut [u8]) {}

    fn try_fill_bytes(&mut self, _: &mut [u8]) -> Result<(), rand::Error> {
        Ok(())
    }
}

impl MockRand {
    pub fn new(results: Vec<u64>) -> Self {
        Self { results }
    }
}
