// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//
// Derived from the Microsoft Edit project.

use std::ops::Range;

use stdext::ReplaceRange as _;

/// An abstraction over reading from text containers.
pub trait ReadableDocument {
    fn read_forward(&self, off: usize) -> &[u8];
    fn read_backward(&self, off: usize) -> &[u8];
}

/// An abstraction over writing to text containers.
pub trait WriteableDocument: ReadableDocument {
    fn replace(&mut self, range: Range<usize>, replacement: &[u8]);
}

impl ReadableDocument for &[u8] {
    fn read_forward(&self, off: usize) -> &[u8] {
        let s = *self;
        &s[off.min(s.len())..]
    }

    fn read_backward(&self, off: usize) -> &[u8] {
        let s = *self;
        &s[..off.min(s.len())]
    }
}

impl ReadableDocument for String {
    fn read_forward(&self, off: usize) -> &[u8] {
        let s = self.as_bytes();
        &s[off.min(s.len())..]
    }

    fn read_backward(&self, off: usize) -> &[u8] {
        let s = self.as_bytes();
        &s[..off.min(s.len())]
    }
}

impl WriteableDocument for String {
    fn replace(&mut self, range: Range<usize>, replacement: &[u8]) {
        let utf8 = String::from_utf8_lossy(replacement);
        unsafe { self.as_mut_vec() }.replace_range(range, utf8.as_bytes());
    }
}
