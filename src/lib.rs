#![feature(entry_insert)]
#![allow(clippy::blacklisted_name)]  // bar is good identifier
#[macro_use] extern crate failure;
use failure::Error;

pub type Res<T> = Result<T, Error>;

pub mod data;
pub mod parser;
pub mod draw;
pub mod search;

pub mod progress;

pub fn join_sorted<T: Ord, I1: IntoIterator<Item=I2>, I2: IntoIterator<Item=T>>(arrays: I1) -> Vec<T> {
    // FIXME
    let mut res: Vec<T> = arrays.into_iter().flatten().collect();
    res.sort();
    res
}