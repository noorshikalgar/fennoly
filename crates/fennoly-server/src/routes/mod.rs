//! HTTP routes. Everything lives under `/api`.

mod account;
mod admin;
mod content;
mod nodes;
mod shares;
mod tools;
mod trash;
mod uploads;

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
        .route("/users", get(account::directory))
        .route("/admin/users", get(admin::list).post(admin::create))
        .route("/admin/users/{id}", patch(admin::update).delete(admin::remove))
        .route("/nodes/{id}", get(nodes::get_node).patch(nodes::update_node).delete(nodes::trash_node))
        .route("/nodes/{id}/children", get(nodes::children))
        .route("/nodes/{id}/folders", post(nodes::create_folder))
        .route("/nodes/{id}/star", put(nodes::star).delete(nodes::unstar))
        .route("/nodes/{id}/shares", get(shares::list).post(shares::create))
        .route("/shares/{id}", patch(shares::update).delete(shares::remove))
        .route("/shared", get(shares::shared_with_me))
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
        .route("/download/zip", get(content::zip));

    let uploads = Router::new()
        .route("/uploads", post(uploads::create).options(uploads::options))
        .route(
            "/uploads/{id}",
            patch(uploads::patch).head(uploads::head).delete(uploads::terminate),
        )
        .layer(DefaultBodyLimit::disable());

    Router::new().merge(json).merge(files).merge(uploads)
}
