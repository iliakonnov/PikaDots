use chrono::NaiveDateTime;
use byteorder::{LE, WriteBytesExt, ReadBytesExt};
use std::io::prelude::*;
use std::collections::HashMap;
use crate::Res;
use std::borrow::Cow;
use streaming_iterator::StreamingIterator;
use std::io::SeekFrom;
use parking_lot::Mutex;
use std::borrow::BorrowMut;

#[derive(Debug, Clone)]
pub struct UserInfo {
    pub name: String,
    pub pikabu_id: i64,
    pub seek: Option<usize>,
    pub comments: Vec<NaiveDateTime>,
}

pub fn read_chunk<R: Read>(mut reader: R, seek: Option<usize>) -> Res<Option<UserInfo>> {
    let pikabu_id = reader.read_i64::<LE>()?;
    let mut name = Vec::new();
    loop {
        let byte = reader.read_u8()?;
        if byte == 0 {
            break;
        }
        name.push(byte);
    }
    let name = String::from_utf8(name)?;
    let mut comments = Vec::new();
    loop {
        let ts = reader.read_i64::<LE>()?;
        if ts == std::i64::MIN {
            break;
        }
        let ts = NaiveDateTime::from_timestamp(ts, 0);
        comments.push(ts);
    }
    if pikabu_id == 0 && name.is_empty() && comments.is_empty() {
        Ok(None)
    } else {
        Ok(Some(UserInfo {
            pikabu_id,
            name,
            comments,
            seek,
        }))
    }
}

pub fn write_chunk<W: Write>(mut writer: W, info: Option<&UserInfo>) -> Res<()> {
    match info {
        Some(info) => {
            writer.write_i64::<LE>(info.pikabu_id)?;
            for i in info.name.bytes() {
                writer.write_u8(i)?;
            }
            writer.write_u8(0)?;
            for i in &info.comments {
                writer.write_i64::<LE>(i.timestamp())?
            }
            writer.write_i64::<LE>(std::i64::MIN)?;
            Ok(())
        }
        None => {
            writer.write_i64::<LE>(0)?;
            writer.write_u8(0)?;
            writer.write_i64::<LE>(std::i64::MIN)?;
            Ok(())
        }
    }
}

pub fn write_all<W: Write, D: SimpleData>(mut data: D, mut writer: W) -> Res<()> {
    let mut reader = data.get_reader(ReadConfig::None);
    while let Some(x) = reader.next() {
        write_chunk(&mut writer, Some(x))?;
    }
    write_chunk(writer, None)?;
    Ok(())
}

#[derive(Clone, Copy)]
pub struct CacheConfig {
    pub names: bool,
    pub ids: bool,
    pub offsets: bool,
    pub prefer_seek: bool
}

#[derive(Clone, Copy)]
pub enum ReadConfig {
    None,
    Cache(CacheConfig)
}

pub enum ReaderValue {
    Cached(usize),
    Owned(UserInfo),
    None
}

impl ReaderValue {
    pub fn into_cow<D: SimpleData>(self, data: &mut D) -> Option<Cow<UserInfo>> {
        match self {
            ReaderValue::Cached(idx) => {
                if let Some(x) = data.get_cached(idx) {
                    Some(Cow::Borrowed(x))
                } else {
                    None
                }
            },
            ReaderValue::Owned(ow) => Some(Cow::Owned(ow)),
            ReaderValue::None => None
        }
    }

    pub fn to_ref<'a, D: SimpleData>(&'a self, data: &'a mut D) -> Option<&'a UserInfo> {
        match self {
            ReaderValue::Cached(idx) => {
                if let Some(x) = data.get_cached(*idx) {
                    Some(x)
                } else {
                    None
                }
            },
            ReaderValue::Owned(ow) => Some(ow),
            ReaderValue::None => None
        }
    }
}

pub struct Reader<'a, T: SimpleData + Sized> {
    data: &'a mut T,
    config: ReadConfig,
    val: ReaderValue
}

impl<'a, T: SimpleData+Sized> StreamingIterator for Reader<'a, T> {
    type Item = UserInfo;

    fn advance(&mut self) {
        let chunk = self.data.read_next();
        self.val = if let Some(x) = chunk {
            if let ReadConfig::Cache(cfg) = self.config {
                ReaderValue::Cached(self.data.put_cache(x, cfg))
            } else {
                ReaderValue::Owned(x)
            }
        } else {
            ReaderValue::None
        }
    }

    fn get(&self) -> Option<&Self::Item> {
        match &self.val {
            ReaderValue::Owned(x) => Some(x),
            ReaderValue::Cached(idx) => self.data.get_cached(*idx),
            ReaderValue::None => None
        }
    }
}

pub trait SimpleData: Sized {
    type Reader;
    type Reference: Copy;

    fn get(&self, r: Self::Reference) -> Res<ReaderValue>;

    fn get_cached(&self, idx: usize) -> Option<&UserInfo>;
    fn put_cache(&mut self, info: UserInfo, cfg: CacheConfig) -> usize;

    // FIXME: Too concrete type
    fn iter_names(&self) -> std::collections::hash_map::Iter<String, Self::Reference>;
    fn iter_ids(&self) -> std::collections::hash_map::Iter<i64, Self::Reference>;

    fn by_name(&mut self, name: &str) -> Res<ReaderValue>;
    fn by_id(&mut self, id: i64) -> Res<ReaderValue>;

    fn read_next(&mut self) -> Option<UserInfo>;

    fn get_reader(&mut self, config: ReadConfig) -> Reader<Self> {
        Reader {
            data: self,
            config,
            val: ReaderValue::None
        }
    }
}

pub trait SeekableData {
    fn read_at(&self, offset: usize) -> Res<Option<UserInfo>>;

    fn reset(&self) -> Res<()>;

    fn read_at_val(&self, offset: usize) -> Res<ReaderValue> {
        Ok(match self.read_at(offset)? {
            Some(x) => ReaderValue::Owned(x),
            None => ReaderValue::None
        })
    }

    fn by_offset(&self, offset: usize) -> Res<ReaderValue>;
}

impl<R, F> Data<R, F> {
    pub fn new(reader: R) -> Self {
        Data {
            reader,
            cached: Default::default(),
            names: Default::default(),
            ids: Default::default(),
            offsets: Default::default(),
        }
    }

    pub fn fill_cache(&mut self, names: HashMap<String, F>, ids: HashMap<i64, F>) {
        self.names = names;
        self.ids = ids;
    }
}

pub struct Data<R, F> {
    reader: R,
    pub cached: Vec<UserInfo>,
    pub names: HashMap<String, F>,
    pub ids: HashMap<i64, F>,
    pub offsets: HashMap<usize, usize>,
}


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeekableRef {
    Seek(usize),
    Cached(usize)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheRef(pub usize);

impl<R: Read> SimpleData for Data<R, CacheRef> {
    type Reader = R;
    type Reference = CacheRef;

    fn get(&self, r: Self::Reference) -> Res<ReaderValue> {
        Ok(if r.0 < self.cached.len() {
            ReaderValue::Cached(r.0)
        } else {
            ReaderValue::None
        })
    }

    fn get_cached(&self, idx: usize) -> Option<&UserInfo> {
        self.cached.get(idx)
    }

    fn put_cache(&mut self, info: UserInfo, cfg: CacheConfig) -> usize {
        let idx = self.cached.len();

        if cfg.offsets {
            if let Some(x) = info.seek {
                self.offsets.insert(x, idx);
            }
        }

        if cfg.ids {
            self.ids.insert(info.pikabu_id, CacheRef(idx));
        }

        if cfg.names {
            self.names.insert(info.name.to_lowercase(), CacheRef(idx));
        }

        self.cached.push(info);
        idx
    }

    fn iter_names(&self) -> std::collections::hash_map::Iter<String, Self::Reference> {
        self.names.iter()
    }

    fn iter_ids(&self) -> std::collections::hash_map::Iter<i64, Self::Reference> {
        self.ids.iter()
    }

    fn by_name(&mut self, name: &str) -> Res<ReaderValue> {
        Ok(match self.names.get(&name.to_lowercase()) {
            Some(x) => ReaderValue::Cached(x.0),
            None => ReaderValue::None
        })
    }

    fn by_id(&mut self, id: i64) -> Res<ReaderValue> {
        Ok(match self.ids.get(&id) {
            Some(x) => ReaderValue::Cached(x.0),
            None => ReaderValue::None
        })
    }

    fn read_next(&mut self) -> Option<UserInfo> {
        if let Ok(Some(x)) = read_chunk(&mut self.reader, None) {
            Some(x)
        } else {
            None
        }
    }
}

impl<R: Read+Seek, F> SeekableData for Data<Mutex<R>, F> {
    fn read_at(&self, offset: usize) -> Res<Option<UserInfo>> {
        let mut reader = self.reader.lock();
        reader.seek(SeekFrom::Start(offset as u64))?;
        let r: &mut R = reader.borrow_mut();
        Ok(if let Ok(Some(x)) = read_chunk(r, Some(offset)) {
            Some(x)
        } else {
            None
        })
    }

    fn reset(&self) -> Res<()> {
        self.reader.lock().seek(SeekFrom::Start(0))?;
        Ok(())
    }

    fn by_offset(&self, offset: usize) -> Res<ReaderValue> {
        Ok(match self.offsets.get(&offset) {
            Some(x) => ReaderValue::Cached(*x),
            None => match self.read_at(offset)? {
                Some(x) => ReaderValue::Owned(x),
                None => ReaderValue::None
            }
        })
    }
}

impl<R: Read+Seek> SimpleData for Data<Mutex<R>, SeekableRef> where Self: SeekableData {
    type Reader = R;
    type Reference = SeekableRef;

    fn get(&self, r: Self::Reference) -> Res<ReaderValue> {
        match r {
            SeekableRef::Cached(idx) => Ok(ReaderValue::Cached(idx)),
            SeekableRef::Seek(s) => self.read_at_val(s)
        }
    }

    fn get_cached(&self, idx: usize) -> Option<&UserInfo> {
        self.cached.get(idx)
    }

    fn iter_names(&self) -> std::collections::hash_map::Iter<String, Self::Reference> {
        self.names.iter()
    }

    fn iter_ids(&self) -> std::collections::hash_map::Iter<i64, Self::Reference> {
        self.ids.iter()
    }

    fn put_cache(&mut self, info: UserInfo, cfg: CacheConfig) -> usize {
        let idx = self.cached.len();

        if cfg.offsets {
            if let Some(x) = info.seek {
                self.offsets.insert(x, idx);
            }
        }

        if cfg.ids {
            self.ids.insert(info.pikabu_id, SeekableRef::Cached(idx));
        }

        if cfg.names {
            self.names.insert(info.name.to_lowercase(), SeekableRef::Cached(idx));
        }

        self.cached.push(info);
        idx
    }

    fn by_name(&mut self, name: &str) -> Res<ReaderValue> {
        match self.names.get(&name.to_lowercase()) {
            Some(SeekableRef::Seek(s)) => self.read_at_val(*s),
            Some(SeekableRef::Cached(x)) => Ok(ReaderValue::Cached(*x)),
            None => Ok(ReaderValue::None)
        }
    }

    fn by_id(&mut self, id: i64) -> Res<ReaderValue> {
        match self.ids.get(&id) {
            Some(SeekableRef::Seek(s)) => self.read_at_val(*s),
            Some(SeekableRef::Cached(x)) => Ok(ReaderValue::Cached(*x)),
            None => Ok(ReaderValue::None)
        }
    }

    fn read_next(&mut self) -> Option<UserInfo> {
        let mut reader = self.reader.lock();
        let seek = reader.seek(SeekFrom::Current(0))
            .map(|x| x as usize)
            .ok();
        let r: &mut R = reader.borrow_mut();
        if let Ok(Some(x)) = read_chunk(r, seek) {
            Some(x)
        } else {
            None
        }
    }
}
