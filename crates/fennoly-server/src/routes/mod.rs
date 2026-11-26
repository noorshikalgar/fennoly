//! HTTP routes. Everything lives under `/api`.

mod account;
mod admin;
mod backups;
mod chunked;
mod content;
mod links;
pub(crate) mod nodes;
mod shares;
mod tools;
mod trash;
pub(crate) mod uploads;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, patch, post, put};
use tower_http::compression::CompressionLayer;

use crate::state::AppState;

pub use trash::purge_node;

pub fn api() -> Router<AppState> {
    // JSON routes: compressed.
    let json = Router::new()
        .route("/health", get(account::health))
        .route("/setup", get(account::setup_status).post(account::setup))
        .route("/auth/login", post(account::login))
        .route("/auth/logout", post(account::logout))
        .route("/me", get(account::me).patch(account::update_me))
        .route("/me/password", post(account::change_password))
        .route("/me/progress", get(account::progress))
        .route("/users", get(account::directory))
        .route("/admin/users", get(admin::list).post(admin::create))
        .route("/admin/users/{id}", patch(admin::update).delete(admin::remove))
        .route("/admin/backups", get(backups::list).post(backups::create))
        .route("/admin/backups/status", get(backups::status))
        .route("/admin/backups/{id}", get(backups::get_one).patch(backups::update).delete(backups::remove))
        .route("/admin/backups/{id}/test", post(backups::test))
        .route("/admin/backups/{id}/run", post(backups::run_now))
        .route("/admin/backups/{id}/runs", get(backups::runs))
        .route("/admin/backups/{id}/recovery-key", post(backups::recovery_key))
        .route("/admin/backups/{id}/snapshots", get(backups::snapshots))
        .route("/admin/backups/{id}/snapshots/{snap}/ls", get(backups::browse))
        .route("/admin/backups/{id}/snapshots/{snap}/restore", post(backups::restore))
        .route("/nodes/{id}", get(nodes::get_node).patch(nodes::update_node).delete(nodes::trash_node))
        .route("/nodes/{id}/children", get(nodes::children))
        .route("/nodes/{id}/folders", post(nodes::create_folder))
        .route("/nodes/{id}/star", put(nodes::star).delete(nodes::unstar))
        .route("/nodes/{id}/shares", get(shares::list).post(shares::create))
        .route("/shares/{id}", patch(shares::update).delete(shares::remove))
        .route("/shared", get(shares::shared_with_me))
        .route("/shared-by-me", get(shares::shared_by_me))
        .route("/nodes/{id}/sharing", delete(shares::stop_sharing))
        .route("/nodes/{id}/links", get(links::list).post(links::create))
        .route("/links/{id}", patch(links::update).delete(links::remove))
        .route("/public/{token}", get(links::info))
        .route("/public/{token}/unlock", post(links::unlock))
        .route("/public/{token}/folder/{id}", get(links::folder))
        .route("/recent", get(nodes::recent))
        .route("/starred", get(nodes::starred))
        .route("/search", get(nodes::search))
        .route("/trash", get(trash::list).delete(trash::empty))
        .route("/trash/{id}/restore", post(trash::restore))
        .route("/trash/{id}", delete(trash::purge))
        .route("/tools", get(tools::capabilities))
        .route("/tools/preview", post(tools::preview))
        .route("/tools/apply", post(tools::apply))
        .layer(CompressionLayer::new());

    // File bytes: no compression (range requests must stay exact).
    let files = Router::new()
        .route("/nodes/{id}/content", get(content::content))
        .route("/nodes/{id}/thumbnail", get(content::thumbnail))
        .route("/download/zip", get(content::zip))
        .route("/public/{token}/content/{id}", get(links::file_content))
        .route("/public/{token}/thumbnail/{id}", get(links::file_thumbnail))
        .route("/public/{token}/zip", get(links::zip));

    let uploads = Router::new()
        .route("/uploads", post(uploads::create).options(uploads::options))
        .route(
            "/uploads/{id}",
            patch(uploads::patch).head(uploads::head).delete(uploads::terminate),
        )
        .route("/chunked", post(chunked::plan))
        .route("/chunked/{id}", get(chunked::status).delete(chunked::cancel))
        .route("/chunked/{id}/chunks/{index}", put(chunked::put_chunk))
        .route("/chunked/{id}/finish", post(chunked::finish))
        .layer(DefaultBodyLimit::disable());

    Router::new()
        .merge(json)
        .merge(files)
        .merge(uploads)
        // Unknown API paths are a JSON 404, never the web app's index.html.
        .fallback(|| async { crate::error::AppError::NotFound })
}
