use std::io::prelude::*;
use serde::Deserialize;
use crate::data::*;
use crate::Res;
use std::collections::hash_map::Entry;
use chrono::NaiveDateTime;
use indicatif::ProgressBar;

#[derive(Deserialize)]
struct Row {
    created_at_timestamp: i64,
    author_id: i64,
    author_username: String,
}

#[cfg(not(feature = "no_map"))]
pub fn parse_json<R: BufRead, A>(reader: &mut R, data: &mut Data<A, CacheRef>) -> Res<()>
    where Data<A, CacheRef>: SimpleData
{
    let cfg = CacheConfig {
        names: true,
        ids: true,
        offsets: false,
        prefer_seek: false
    };
    for ln in reader.lines() {
        let ln = ln?;
        if ln.is_empty() {
            break;
        }

        // Replace `\\` to `\`
        let ln = ln.replace(r#"\\"#, r#"\"#);
        let parsed: Row = serde_json::from_str(&ln).unwrap();
        let ts = NaiveDateTime::from_timestamp(parsed.created_at_timestamp, 0);

        match data.ids.entry(parsed.author_id) {
            Entry::Occupied(occ) => {
                let idx = occ.get().0;
                data.cached[idx].comments.push(ts);
            },
            Entry::Vacant(vac) => {
                data.put_cache(UserInfo {
                    name: parsed.author_username,
                    pikabu_id: parsed.author_id,
                    comments: vec![ts],
                    seek: None,
                }, &cfg);
            }
        }
    }

    for i in &mut data.cached {
        i.comments.sort();
    }

    Ok(())
}
