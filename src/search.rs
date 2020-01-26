use crate::data::*;
use crate::Res;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use regex;
use streaming_iterator::StreamingIterator;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UserSelector {
    Name(String),
    Glob(String),
    Regexp(String),
    PikabuId(i64),
    Seek(u64)
}

impl UserSelector {
    pub fn new(s: &str) -> Res<Self> {
        // Since we are removing characters from beginning it is impossible to make it faster
        // than O(n) without too much hassle, so .to_owned is fine
        if s.starts_with("re:") {
            let s = &s[3..];
            Ok(UserSelector::Regexp(s.to_lowercase()))
        } else if s.starts_with("gl:") {
            let s = &s[3..];
            Ok(UserSelector::Glob(s.to_lowercase()))
        } else if s.starts_with("id:") {
            let s = &s[3..];
            let id = s.parse()?;
            Ok(UserSelector::PikabuId(id))
        } else if s.starts_with("sk:") {
            let s = &s[3..];
            let sk = s.parse()?;
            Ok(UserSelector::Seek(sk))
        } else {
            Ok(UserSelector::Name(s.to_lowercase()))
        }
    }

    pub fn human_readable(&self) -> String {
        match self {
            UserSelector::Name(n) => n.to_string(),
            UserSelector::Glob(gl) => format!("gl:'{}'", gl),
            UserSelector::Regexp(re) => format!("re:'{}'", re),
            UserSelector::PikabuId(id) => format!("id:{}", id),
            UserSelector::Seek(seek) => format!("sk:{}", seek),
        }
    }
}

#[derive(Clone, Debug)]
struct CompiledRegexp<'a> {
    original: &'a str,
    reg: regex::Regex
}

impl<'a> PartialEq for CompiledRegexp<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.original == other.original
    }
}

impl<'a> Eq for CompiledRegexp<'a> {}

impl<'a> Hash for CompiledRegexp<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.original.hash(state)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Compiled<'a> {
    Name(&'a str),
    Glob(glob::Pattern),
    Regexp(CompiledRegexp<'a>),
    PikabuId(i64),
    Seek(u64)
}

impl<'a> Compiled<'a> {
    fn compile(raw: &[UserSelector]) -> Res<Vec<Compiled>> {
        // FIXME: Cloning in .entry(...) can be avoided somehow. But IDK how.
        let mut regexps = HashMap::new();
        let mut globs = HashMap::new();
        raw.iter().map(|i| match i {
            UserSelector::PikabuId(x) => Ok(Compiled::PikabuId(*x)),
            UserSelector::Regexp(r) => regexps
                .entry(r.clone())
                .or_insert_with(|| regex::RegexBuilder::new(&r)
                    .case_insensitive(true)
                    .size_limit(5*1024*1024) // 5Mb
                    .nest_limit(5)
                    .dfa_size_limit(5*1024*1024)
                    .build()
                    .map(|x| Compiled::Regexp(CompiledRegexp {
                        original: r,
                        reg: x
                    }))
                )
                .clone().map_err(|e| e.into()),
            UserSelector::Name(name) => Ok(Compiled::Name(name)),
            UserSelector::Glob(gl) => globs
                .entry(gl.clone())
                .or_insert_with(|| glob::Pattern::new(&gl)
                    .map(Compiled::Glob)
                    // Because PatternError does not implement Clone.
                    // failure::Error does not implement too, so don't use format_err!(...)
                    .map_err(|e| format!("{}", e))
                )
                .clone().map_err(|e| format_err!("{}", e)),
            UserSelector::Seek(x) => Ok(Compiled::Seek(*x))
        }).collect()
    }
}

pub struct SearchSettings {
    pub use_cache: bool,
    pub limit: usize  // Returns error if limit is reached
}

pub fn find_seek<D: SimpleData+SeekableData>(data: &mut D, query: Vec<Vec<UserSelector>>, mut settings: SearchSettings) -> Res<Vec<Vec<UserInfo>>> {
    // Handle seeks only
    let mut seeks = Vec::new();
    let mut new_query = Vec::with_capacity(query.len());
    for i in query {
        let mut tmp = Vec::with_capacity(i.len());
        let mut tmp_sk = Vec::new();
        for j in i {
            if let UserSelector::Seek(sk) = j {
                let user = data.by_offset(sk as usize)?.into_cow(data)
                    .ok_or_else(|| format_err!("by_offset returned None"))?;
                tmp_sk.push(user.into_owned());
                settings.limit -= 1; if settings.limit == 0 {return Err(format_err!("Limit reached!"))}
            } else {
                tmp.push(j)
            }
        }
        new_query.push(tmp);
        seeks.push(tmp_sk);
    }
    let mut other = find(data, &new_query, settings)?;
    data.reset()?;
    for (i, v) in other.iter_mut().enumerate() {
        v.append(&mut seeks[i])
    }
    Ok(other)
}

pub fn find<D: SimpleData>(data: &mut D, query: &[Vec<UserSelector>], mut settings: SearchSettings) -> Res<Vec<Vec<UserInfo>>> {
    let compiled: Res<Vec<_>> = query.iter().map(|x| Compiled::compile(&x)).collect();
    let compiled = compiled?;

    let mut to_find = HashMap::new();
    let mut iter_all = false;
    for constraints in &compiled {
        for selector in constraints {
            if let Compiled::Glob(_) | Compiled::Regexp(_) = selector {
                iter_all = true;
            }
            to_find.entry(selector).or_insert_with(Vec::new);
        }
    }

    if iter_all || !settings.use_cache {
        let mut pending = if iter_all {
            std::usize::MAX  // This program won't work when there is too much users
        } else {
            to_find.len() // Only names and id's, so only one user for each selector
        };
        if !settings.use_cache {
            let mut reader = data.get_reader(ReadConfig::None)?;
            'l1: while let Some(user) = reader.next() {
                for (c, v) in &mut to_find {
                    let matches = match c {
                        Compiled::Name(n) => n == &user.name.to_lowercase(),
                        Compiled::Glob(gl) => gl.matches_with(&user.name, glob::MatchOptions {
                            case_sensitive: false,
                            require_literal_separator: false,
                            require_literal_leading_dot: false
                        }),
                        Compiled::Regexp(re) => re.reg.is_match(&user.name),
                        Compiled::PikabuId(id) => *id == user.pikabu_id,
                        Compiled::Seek(s) => Some(*s as usize) == user.seek,
                    };
                    if matches {
                        settings.limit -= 1; if settings.limit == 0 {return Err(format_err!("Limit reached!"))}
                        v.push(ReaderValue::Owned(user.clone()));
                        pending -= 1;
                        if pending == 0 {
                            break 'l1;
                        }
                    }
                }
            }
        } else {
            'l2: for (name, r) in data.iter_names() {
                for (c, v) in &mut to_find {
                    let matches = match c {
                        Compiled::Name(n) => name == n,
                        Compiled::Glob(gl) => gl.matches(name),
                        Compiled::Regexp(re) => re.reg.is_match(name),
                        Compiled::Seek(_) =>
                            return Err(format_err!("Seek search not available. Use find_seek(...) instead")),
                        _ => false
                    };
                    if matches {
                        settings.limit -= 1; if settings.limit == 0 {return Err(format_err!("Limit reached!"))}
                        v.push(data.get(*r)?);
                        pending -= 1;
                        if pending == 0 {
                            break 'l2;
                        }
                    }
                }
            }
            'l3: for (id, r) in data.iter_ids() {
                for (c, v) in &mut to_find {
                    match c {
                        Compiled::PikabuId(i) if i == id => {
                            settings.limit -= 1; if settings.limit == 0 {return Err(format_err!("Limit reached!"))}
                            v.push(data.get(*r)?);
                            pending -= 1;
                            if pending == 0 {
                                break 'l3;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    } else { // !iter_all && use_cache
        for (c, v) in &mut to_find {
            let val = match c {
                Compiled::Glob(_) | Compiled::Regexp(_) =>
                    return Err(format_err!("Internal searcher error. Invalid constraint in light path")),
                Compiled::Name(n) => data.by_name(&n),
                Compiled::PikabuId(id) => data.by_id(*id),
                Compiled::Seek(_) =>
                    return Err(format_err!("Seek search not available. Use find_seek(...) instead"))
            };
            let cow = val?.into_cow(data);
            if let Some(x) = cow {
                settings.limit -= 1; if settings.limit == 0 {return Err(format_err!("Limit reached!"))}
                v.push(ReaderValue::Owned(x.into_owned()))
            }
        }
    }

    let result = compiled.iter().map(|constraints| {
        constraints.iter()
            .map(|selector| {
                let rs = to_find.get(&selector).unwrap();
                rs.iter()
                    .map(|x| x.to_ref(data).unwrap().clone())
                    .collect::<Vec<_>>()
            })
            .flatten()
            // Dedup:
            .fold((HashSet::new(), Vec::new()), |(mut h, mut r), i| {
                if h.insert(i.pikabu_id) {
                    r.push(i)
                }
                (h, r)
            })
            .1
    }).collect();
    Ok(result)
}

pub fn selector_name(selectors: &[UserSelector]) -> String {
    if selectors.is_empty() {
        return "<none>".to_string()
    }
    let mut parts = String::new();
    let pre_last = selectors.len() - 1;
    for i in &selectors[..pre_last] {
        parts.push_str(&i.human_readable());
        parts.push_str("+");
    }
    parts.push_str(&selectors.last().unwrap().human_readable());
    parts
}
