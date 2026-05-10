use std::fmt::Display;
use std::io::{self, Read, Write};
use std::str::FromStr;

pub struct Scanner {
    input: Vec<u8>,
    index: usize,
}

impl Scanner {
    pub fn stdin() -> Self {
        let mut input = Vec::new();
        io::stdin().read_to_end(&mut input).unwrap();
        Self::new(input)
    }

    pub fn new(input: Vec<u8>) -> Self {
        Self { input, index: 0 }
    }

    pub fn read<T>(&mut self) -> T
    where
        T: FromStr,
        T::Err: std::fmt::Debug,
    {
        while self
            .input
            .get(self.index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            self.index += 1;
        }
        let start = self.index;
        while self
            .input
            .get(self.index)
            .is_some_and(|byte| !byte.is_ascii_whitespace())
        {
            self.index += 1;
        }
        std::str::from_utf8(&self.input[start..self.index])
            .unwrap()
            .parse()
            .unwrap()
    }
}

pub struct Output<W: Write = io::BufWriter<io::Stdout>> {
    writer: W,
}

impl Output<io::BufWriter<io::Stdout>> {
    pub fn stdout() -> Self {
        Self::new(io::BufWriter::new(io::stdout()))
    }
}

impl<W: Write> Output<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn print<T: Display>(&mut self, value: T) {
        write!(self.writer, "{value}").unwrap();
    }

    pub fn println<T: Display>(&mut self, value: T) {
        writeln!(self.writer, "{value}").unwrap();
    }

    pub fn bytes(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).unwrap();
    }

    pub fn flush(&mut self) {
        self.writer.flush().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_reads_whitespace_separated_tokens() {
        let mut scanner = Scanner::new(b"  3\n-5 hello ".to_vec());

        assert_eq!(scanner.read::<usize>(), 3);
        assert_eq!(scanner.read::<i32>(), -5);
        assert_eq!(scanner.read::<String>(), "hello");
    }

    #[test]
    fn output_writes_display_values_and_bytes() {
        let mut output = Output::new(Vec::new());

        output.print(12);
        output.bytes(b" ");
        output.println("ok");

        assert_eq!(output.writer, b"12 ok\n");
    }
}
