use std::sync::Arc;
use parking_lot::Mutex;
use rocket::State;
use rocket::response::content::Content;
use rocket::http::ContentType;
use rocket::http::Status;
use crate::pikadots::search::find_seek;
use crate::pikadots::Res;
use std::io::Cursor;
use image::DynamicImage;
use pikadots::search::{UserSelector, SearchSettings};
use pikadots::join_sorted;

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


#[get("/<query>/i.png")]
fn handle(state: State<WebState>, query: String) -> Result<Png, Status> {
    let query: Result<Vec<_>, _> = query.split(',').map(UserSelector::new).collect();
    let query = query.map_err(|e| {
        Status::new(400, "Invalid selector")
    })?;

    let mut res = {
        // Block for mutex

        let mut data = state.data.try_lock()
            .ok_or_else(|| {
                eprintln!("Failed try_lock!");
                Status::new(500, "Unable to acquire mutex. Try restarting server ¯\\_(ツ)_/¯")
            })?;
        let query = vec![query];
        find_seek(&mut *data, query,  SearchSettings{
            use_cache: state.cache,
            limit: 100
        })
            .map_err(|e| {
                Status::new(500, "Error searching this user")
            })?
    };
    let user = res.pop();
    let user = if let Some(x) = user {
        x
    } else {
        return Err(Status::NotFound)
    };

    let mut buf = Vec::new();
    let mut writer = Cursor::new(buf);
    let points = join_sorted(user.into_iter().map(|x| x.comments));
    let image = pikadots::draw::generate(&points[..]);
    let img = image.into_image()
        .map_err(|e| {
            Status::new(500, "Error saving image")
        })?;
    DynamicImage::ImageRgb8(img).write_to(&mut writer, image::PNG)
        .map_err(|e| {
            Status::new(500, "Error writing image")
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
        .mount(base, routes![handle, stats])
        .manage(WebState {
            data: Arc::new(Mutex::new(data)),
            cache
        })
        .launch();
}