use std::sync::Arc;
use parking_lot::Mutex;
use rocket::State;
use rocket::response::content::Html;
use crate::pikadots::search::find_seek;
use std::io::Cursor;
use image::DynamicImage;
use pikadots::search::{UserSelector, SearchSettings};
use pikadots::join_sorted;
use pikadots::data::UserInfo;

// Only plain data from file is available for WebState. No cache support, too
// FIXME: Too concrete type: Mutex<BufReader<File>>
pub type Data =
    crate::pikadots::data::Data<
        parking_lot::Mutex<
            std::io::BufReader<
                std::fs::File
            >
        >,
        crate::pikadots::data::SeekableRef
    >;

struct WebState {
    data: Arc<Mutex<Data>>,
    cache: bool,
}

#[derive(Responder)]
#[response(status = 200, content_type = "image/png")]
struct Png(Vec<u8>);

#[derive(Responder, Debug)]
enum Error {
    #[response(status = 500, content_type = "text/plain")]
    Inernal(String),
    #[response(status = 400, content_type = "text/plain")]
    InvalidRequest(String),
    #[response(status = 404, content_type = "text/plain")]
    NotFound(&'static str)
}

fn find_user(state: State<WebState>, query: String) -> Result<Vec<UserInfo>, Error> {
    let query: Result<Vec<_>, _> = query.split(',').map(UserSelector::new).collect();
    let query = query.map_err(|e| {
        Error::InvalidRequest(format!("Invalid selector: {}", e))
    })?;

    let mut res = {
        // Block for mutex

        let mut data = state.data.try_lock()
            .ok_or_else(|| {
                Error::Inernal("Unable to acquire mutex. Try restarting server ¯\\_(ツ)_/¯".to_string())
            })?;
        let query = vec![query];
        find_seek(&mut *data, query,  SearchSettings{
            use_cache: state.cache,
            limit: 100
        })
            .map_err(|e| {
                Error::Inernal(format!("Error searching this user: {:?}", e))
            })?
    };
    let user = res.pop();
    if let Some(x) = user {
        Ok(x)
    } else {
        Err(Error::NotFound("No such user"))
    }
}

#[get("/<query>/i.html")]
fn do_info(state: State<WebState>, query: String) -> Result<Html<String>, Error> {
    let users = find_user(state, query)?;
    let mut res = r#"<!DOCTYPE html>
    <html>
    <head>
        <meta charset="utf-8">
        <title>PikaDots</title>
    </head><body><table>
        <thead>
            <tr>
                <th>Name</th>
                <th>Id</th>
                <th>Sk</th>
                <th>Comment count</th>
            </tr>
        </thead>
        <tbody>
    "#.to_string();
    for u in users {
        res.push_str(&format!(r#"
            <tr>
                <td><a href="https://pikastat.d3d.info/user/pikabu_id=={pik}">{name}</a></td>
                <td>{pik}</td>
                <td>{sk}</td>
                <td align="right">{cnt}</td>
            </tr>"#,
            pik=u.pikabu_id, name=u.name, cnt=u.comments.len(),
            sk=u.seek.map(|x| x.to_string()).unwrap_or_default()
        ))
    }
    res.push_str("</tbody></table></body></html>");
    Ok(Html(res))
}

#[get("/<query>/i.png?<tz>")]
fn do_draw(state: State<WebState>, query: String, tz: Option<i8>) -> Result<Png, Error> {
    let tz = tz.unwrap_or(0);
    let users = find_user(state, query)?;

    let buf = Vec::new();
    let mut writer = Cursor::new(buf);
    let points = join_sorted(users.into_iter().map(|x| x.comments));
    let image = pikadots::draw::generate(&points[..]);
    let img = image.into_image(tz)
        .map_err(|e| {
            Error::Inernal(format!("Error saving image: {:?}", e))
        })?;
    DynamicImage::ImageRgb8(img).write_to(&mut writer, image::PNG)
        .map_err(|e| {
            Error::Inernal(format!("Error writing image: {:?}", e))
        })?;

    Ok(Png(writer.into_inner()))
}

#[get("/stats.txt")]
fn stats(state: State<WebState>) -> String {
    match state.data.try_lock() {
        None => {
            eprintln!("Failed try_lock!");
            "Unable to acquire mutex. Try restarting server ¯\\_(ツ)_/¯".to_string()
        },
        Some(data) => {
            format!(
                r#"Stats:
Cache: {} items
NameMap: {} items
IdMap: {} items"#,
                data.cached.len(), data.names.len(), data.ids.len()
            )
        }
    }
}

pub fn launch(data: Data, cache: bool, base: &str) {
    rocket::ignite()
        .mount(base, routes![do_info, do_draw, stats])
        .manage(WebState {
            data: Arc::new(Mutex::new(data)),
            cache
        })
        .launch();
}