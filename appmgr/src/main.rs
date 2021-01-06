use std::path::Path;

use patch_db::PatchDb;

#[tokio::main]
async fn main() {
    let _db = PatchDb::open(Path::new(appmgrlib::PERSISTENCE_DIR).join("appmgr.db"))
        .await
        .expect("opening database");
    // let
}
