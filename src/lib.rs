#![feature(entry_insert)]
#[macro_use] extern crate failure;
use failure::Error;
use std::collections::HashMap;
use std::io::{Read, Seek};
use chrono::NaiveDateTime;

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