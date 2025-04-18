use std::sync::mpsc;

struct Router {
    id: u8,
    value: i32
}

impl Router {
    fn new() -> Self {
        Router {
            id: 0,
            value: 0
        }
    }

    fn add(&mut self, int: i32) {
        self.value  += int;
    }
}

pub fn start(
    input_stream: mpsc::Receiver<i32>
) {
    let mut r = Router::new();

    for integer in input_stream{
        r.add(integer);
        println!("{}", r.value);
    }
}