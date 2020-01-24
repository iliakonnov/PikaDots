#![feature(proc_macro_hygiene, decl_macro)]
#[macro_use] extern crate rocket;
#[macro_use] extern crate clap;
#[macro_use] extern crate failure;
extern crate pikadots;
use clap::{App, SubCommand, Arg};
use std::env::args_os;
use std::ffi::{OsString, OsStr};
use std::fs::{File, read};
use pikadots::{Res};
use pikadots::progress::*;
use std::io::{BufReader, Cursor};
use {std::io, std::io::prelude::*};
use std::path::{Path, PathBuf};
use streaming_iterator::StreamingIterator;
use pikadots::data::{UserInfo, SimpleData};
use std::collections::HashMap;
use chrono::NaiveDateTime;
use pikadots::search::UserSelector;
use parking_lot::Mutex;

mod web;

enum FileOrStdin {
    File(File),
    Stdin(std::io::Stdin)
}

impl FileOrStdin {
    fn new(opt: Option<&OsStr>) -> Res<Self> {
        Ok(match opt {
            None => Self::Stdin(io::stdin()),
            Some(s) => Self::File(File::open(s)?)
        })
    }
}

enum FileOrStdout {
    File(File),
    Stdout(std::io::Stdout)
}

impl FileOrStdout {
    fn new(opt: Option<&OsStr>) -> Res<Self> {
        Ok(match opt {
            None => Self::Stdout(io::stdout()),
            Some(s) => Self::File(File::create(s)?)
        })
    }
}

fn do_parse(source: FileOrStdin, src_gz: bool, dest: FileOrStdout, dest_gz: bool) -> Res<()> {
    use pikadots::data::*;
    use pikadots::parser::parse_json;
    let mut data = Data::new(Cursor::new(Vec::new()));
    // FIXME: Too much boilerplate
    eprintln!("Reading...");
    match source {
        FileOrStdin::File(f) => {
            if src_gz {
                parse_json(&mut BufReader::new(flate2::read::GzDecoder::new(f)), &mut data)?;
            } else {
                parse_json(&mut BufReader::new(f), &mut data)?;
            }
        },
        FileOrStdin::Stdin(f) => {
            if src_gz {
                parse_json(&mut BufReader::new(flate2::read::GzDecoder::new(f)), &mut data)?;
            } else {
                parse_json(&mut BufReader::new(f), &mut data)?;
            }
        }
    }
    eprintln!("Writing...");
    match dest {
        FileOrStdout::File(f) => {
            if dest_gz {
                write_all(data, flate2::write::GzEncoder::new(f, flate2::Compression::new(3)))?;
            } else {
                write_all(data, f)?;
            }
        }
        FileOrStdout::Stdout(f) => {
            if dest_gz {
                write_all(data, flate2::write::GzEncoder::new(f, flate2::Compression::new(3)))?;
            } else {
                write_all(data, f)?;
            }
        }
    }
    Ok(())
}

fn do_draw(gzip: bool, data: FileOrStdin, output: PathBuf, users: Vec<Vec<UserSelector>>, index: Option<File>) -> Res<()> {
    use pikadots::data::*;
    use pikadots::search::*;

    fn work<F>(mut searcher: F, output: PathBuf, users: Vec<Vec<UserSelector>>) -> Res<()>
        where F: FnMut(Vec<Vec<UserSelector>>) -> Res<Vec<Vec<UserInfo>>>
    {
        let names: Vec<String> = users.iter().map(|x| selector_name(&x[..])).collect();
        let found = searcher(users)?;
        for (i, group) in found.into_iter().enumerate() {
            let name = &names[i];
            let comments = group.into_iter().map(|x| x.comments);
            let sorted = pikadots::join_sorted(comments);
            let gen = pikadots::draw::generate(&sorted);
            let img = gen.into_image()?;
            let output = output.join(format!("{}.png", name));
            image::DynamicImage::ImageRgb8(img).save_with_format(output, image::PNG)?;
        }
        Ok(())
    }

    match data {
        FileOrStdin::File(f) => {
            // FIXME: Incorrect progress. Should be Wrapper<BufReader<File>> instead of BufReader<Wrapper<File>>
            // TODO: Make correct benchamarks of reading data. It looks really slow
            let reader = ReaderWrapper::from_file(f);
            let mut bar = reader.bar.clone();
            let reader = BufReader::new(reader);

            let res = if gzip {
                let mut data: Data<_, CacheRef> = Data::new(flate2::read::GzDecoder::new(reader));
                work(|x| find(&mut data, &x, SearchSettings {
                    use_cache: false,
                    limit: std::usize::MAX
                }), output, users)
            } else {
                let mut data: Data<_, SeekableRef> = Data::new(Mutex::new(reader));
                let use_cache = if let Some(idx) = index {
                    load_index(idx, &mut data)?;
                    true
                } else {
                    false
                };
                work(|x| find_seek(&mut data, x, SearchSettings {
                    use_cache,
                    limit: std::usize::MAX
                }), output, users)
            };
            bar.finish();
            res
        }
        FileOrStdin::Stdin(std) => {
            let reader = ReaderWrapper::new(std, 0);
            let mut bar = reader.bar.clone();
            let res = if gzip {
                let mut data: Data<_, CacheRef> = Data::new(flate2::read::GzDecoder::new(reader));
                work(|x| find(&mut data, &x, SearchSettings {
                    use_cache: false,
                    limit: std::usize::MAX
                }), output, users)
            } else {
                let mut data: Data<_, CacheRef> = Data::new(reader);
                work(|x| find(&mut data, &x, SearchSettings {
                    use_cache: false,
                    limit: std::usize::MAX
                }), output, users)
            };
            bar.finish();
            res
        }
    }
}

fn do_index(file: File, out: FileOrStdout) -> Res<()> {
    use pikadots::data::*;
    let reader = ReaderWrapper::from_file(file);
    let mut bar = reader.bar.clone();
    let reader = Mutex::new(BufReader::new(reader));

    let mut data: Data<_, SeekableRef> = Data::new(reader);
    let mut reader = data.get_reader(ReadConfig::None);
    let make_ln = |i: &UserInfo| {
        if let Some(s) = i.seek {
            format!("{},{},{}\n", i.pikabu_id, s, i.name)
        } else {
            format!("{},,{}\n", i.pikabu_id, i.name)
        }
    };
    match out {
        FileOrStdout::File(mut f) => {
            while let Some(i) = reader.next() {
                f.write(make_ln(i).as_bytes())?;
            }
        }
        FileOrStdout::Stdout(mut std) => {
            while let Some(i) = reader.next() {
                std.write(make_ln(i).as_bytes())?;
            }
        }
    }
    bar.finish();
    Ok(())
}

fn load_index<R>(idx: File, data: &mut pikadots::data::Data<R, pikadots::data::SeekableRef>) -> Res<()> {
    let mut reader = BufReader::new(idx);
    let mut names = HashMap::new();
    let mut ids = HashMap::new();
    for ln in reader.lines() {
        let ln = ln?;
        let splitted: Vec<&str> = ln.splitn(3, ',').collect();
        if splitted.len() != 3 {
            continue;
        }
        let (pik, sk, name) = (splitted[0], splitted[1], splitted[2]);
        let (pik, sk, name) = (pik.parse()?, sk.parse()?, name.to_lowercase());
        names.insert(name, pikadots::data::SeekableRef::Seek(sk));
        ids.insert(pik, pikadots::data::SeekableRef::Seek(sk));
    }
    data.fill_cache(names, ids);
    Ok(())
}

fn main() -> Res<()> {
    let app = clap_app!(PikaDots =>
        (version: "0.2")
        (author: "by Dino")
        (@subcommand draw =>
            (about: "Draw images")
            (@arg gzip: -z --gzip "Use gzip when reading data")
            (@arg data: -d --data +takes_value "Path to data. Omit to read from stdin")
            (@arg index: -i --index +takes_value "Load index from file")
            (@arg output: -o --output +takes_value * "Output path")
            (@arg users: -u --users ... +takes_value * "User selectors")
        )
        (@subcommand parse =>
            (about: "Parse json")
            (@arg source: -s --src +takes_value "Path to json. Omit to read from stdin")
            (@arg gzip_in: -g "Use gzip for input")
            (@arg dest: -d --dest +takes_value "Path to data. Omit to write to stdout")
            (@arg gzip_out: -z "Use gzip for output")
        )
        (@subcommand index =>
            (about: "Create indexes")
            (@arg data: -d --data * +takes_value "Path to data")
            (@arg output: -o --output +takes_value "Output file (csv). Omit to write to stdout")
        )
        (@subcommand serve =>
            (about: "Start webserver")
            (@arg data: -d --data +takes_value * "Path to data")
            (@arg index: -i --index +takes_value "Load index from file")
            (@arg mem: -m --memory "Load everything into memory")
            (@arg seeks: -s --seeks "Store seeks in memory instead of values")
            (@group map_name =>
                (@arg name: --name "Create only name hashtable (default)")
                (@arg no_name: --no_name "Do not create name hashtable")
            )
            (@group map_id =>
                (@arg id: --id "Create only pikabu_id hashtable (default)")
                (@arg no_id: --no_id "Do not create name hashtable")
            )
        )
    );

    let matches = app.get_matches();
    match matches.subcommand() {
        ("draw", sub) => {
            let sub = sub.unwrap();
            let gzip = sub.is_present("gzip");
            let data = FileOrStdin::new(sub.value_of_os("data"))?;
            let output = PathBuf::from(sub.value_of_os("output").unwrap());
            let index = sub.value_of_os("index").map(File::open);

            let index = if let Some(idx) = index {Some(idx?)} else {None};

            let vals = sub.values_of_os("users").unwrap();
            let mut users = Vec::with_capacity(vals.len());
            for i in vals {
                let s = i.to_str().ok_or(format_err!("Invalid OsStr: {:?}", i))?;
                let splitted: Res<Vec<_>> = s
                    .split(',')
                    .into_iter()
                    .map(UserSelector::new)
                    .collect();
                users.push(splitted?)
            }
            do_draw(gzip, data, output, users, index)
        },
        ("parse", sub) => {
            let sub = sub.unwrap();
            let source = FileOrStdin::new(sub.value_of_os("source"))?;
            let src_gz = sub.is_present("gzip_in");
            let dest = FileOrStdout::new(sub.value_of_os("dest"))?;
            let dest_gz = sub.is_present("gzip_out");
            do_parse(source, src_gz, dest, dest_gz)
        },
        ("index", sub) => {
            let sub = sub.unwrap();
            let data = File::open(sub.value_of_os("data").unwrap())?;
            let output = FileOrStdout::new(sub.value_of_os("output"))?;
            do_index(data, output)
        },
        ("serve", sub) => {
            let sub = sub.unwrap();
            let data = File::open(sub.value_of_os("data").unwrap())?;
            let index = sub.value_of_os("index").map(File::open);
            let mem = sub.is_present("mem");
            let seeks = sub.is_present("seeks");
            let name = !sub.is_present("no_name");
            let id = !sub.is_present("no_id");

            let data = Mutex::new(BufReader::new(data));
            let mut data = web::Data::new(data);
            let mut use_cache = false;
            if let Some(x) = index {
                load_index(x?, &mut data)?;
                use_cache = true;
            }
            if mem {
                // Read all
                let mut reader = data.get_reader(
                    pikadots::data::ReadConfig::Cache(pikadots::data::CacheConfig {
                        names: name,
                        ids: id,
                        offsets: false,
                        prefer_seek: seeks
                    })
                );
                while let Some(_) = reader.next() {
                    // Just reading all items
                }
                use_cache = true;
            }

            web::launch(data, use_cache, "/");
            Ok(())
        },
        _ => panic!("Unknown subcommand")
    }
}