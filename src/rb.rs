pub struct RingBuffer {
    buffer: Vec<f32>,
    read_index: usize,
    write_index: usize,
    size: usize,
}

impl RingBuffer {
    pub fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            read_index: 0,
            write_index: 0,
            size,
        }
    }

    pub fn push(&mut self, value: f32) {
        self.buffer[self.write_index] = value;
        self.write_index = (self.write_index + 1) % self.size;
    }

    pub fn pop(&mut self) -> Option<f32> {
        if self.read_index == self.write_index {
            return None;
        }

        let value = self.buffer[self.read_index];
        self.read_index = (self.read_index + 1) % self.size;
        Some(value)
    }

    pub fn as_mut_ptr(&mut self) -> *mut f32 {
        self.buffer.as_mut_ptr()
    }
}

// fn main() {
//     let mut ring_buffer = RingBuffer::new(1024);

//     // 获取 RingBuffer 类型的缓冲区可变指针
//     let buffer_ptr = ring_buffer.as_mut_ptr();

//     // 现在你可以操作底层缓冲区
// }
