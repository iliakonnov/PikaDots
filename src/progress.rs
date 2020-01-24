use indicatif::ProgressBar;
use std::io::prelude::*;
use std::io::{Error, SeekFrom};

pub struct ReaderWrapper<R> {
    pub reader: R,
    pub bar: ProgressBar,
    length: u64,
}

impl<R: Read> Read for ReaderWrapper<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let res = self.reader.read(buf);
        if let Ok(num) = res {
            self.bar.inc(num as u64);
        }
        res
    }
}

impl<R: Seek> Seek for ReaderWrapper<R> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Error> {
        match pos {
            SeekFrom::Start(x) => self.bar.set_position(x),
            SeekFrom::End(x) => {
                if x > self.length as i64 {
                    self.bar.set_position(0)
                } else {
                    self.bar.set_position((self.length as i64 - x) as u64)
                }
            }
            SeekFrom::Current(x) => {
                let mut new_pos = self.bar.position() as i64 + x;
                if new_pos < 0 {
                    new_pos = 0
                }
                self.bar.set_position(new_pos as u64)
            }
        }
        self.reader.seek(pos)
    }
}

impl<R> ReaderWrapper<R> {
    pub fn new(reader: R, length: u64) -> Self {
        let sty = if length == 0 {
            ProgressStyle::UnknownBytes
        } else {
            ProgressStyle::Bytes
        };
        let bar = sty.make(length);
        Self { reader, length, bar }
    }

    pub fn message(self, msg: &str) -> Self {
        self.bar.set_message(msg);
        self
    }
}

impl ReaderWrapper<std::fs::File> {
    pub fn from_file(f: std::fs::File) -> Self {
        let len = f.metadata().map(|x| x.len()).unwrap_or_default();
        Self::new(f, len)
    }
}

impl<R: Seek> ReaderWrapper<R> {
    pub fn auto(mut reader: R) -> Self {
        let length = reader.seek(SeekFrom::End(0)).unwrap_or_default();
        reader.seek(SeekFrom::Start(0));
        ReaderWrapper::new(reader, length)
    }
}

#[derive(Clone, Copy)]
pub enum ProgressStyle {
    UnknownBytes,
    Bytes,
    UnknownItems,
    Items,
}

impl ProgressStyle {
    pub fn msg(self, len: u64, message: &str) -> ProgressBar {
        let res = self.make(len);
        res.set_message(message);
        res
    }

    pub fn make(self, len: u64) -> ProgressBar {
        ProgressBar::new(len).with_style(self.into())
    }

    pub fn file(self, f: std::fs::File) -> ProgressBar {
        let len = f.metadata().map(|x| x.len()).unwrap_or_default();
        self.make(len)
    }

    pub fn file_msg(self, f: std::fs::File, msg: &str) -> ProgressBar {
        let res = self.file(f);
        res.set_message(msg);
        res
    }
}

impl Into<indicatif::ProgressStyle> for ProgressStyle {
    fn into(self) -> indicatif::ProgressStyle {
        use indicatif::ProgressStyle;
        match self {
            Self::UnknownBytes => {
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/??? ({bytes_per_sec}) {msg}")
                    .progress_chars("#>-")
            }
            Self::Bytes => {
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec} {eta}) {msg}")
                    .progress_chars("#>-")
            }
            Self::UnknownItems => {
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/??? ({per_sec}) {msg}")
                    .progress_chars("##-")
            }
            Self::Items => {
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} ({per_sec} {eta}) {msg}")
                    .progress_chars("##-")
            }
        }
    }
}