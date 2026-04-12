// Stub clipboard module needed by tui.rs.

#[derive(Default)]
pub struct Clipboard {
    data: Vec<u8>,
    synchronized: bool,
}

impl Clipboard {
    pub fn write(&mut self, data: Vec<u8>) {
        self.data = data;
    }

    pub fn mark_as_synchronized(&mut self) {
        self.synchronized = true;
    }
}
