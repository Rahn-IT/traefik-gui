use config::{Config, ConfigState};
use rocket::{
    fairing::AdHoc,
    fs::FileServer,
    request::FlashMessage,
    response::{Flash, Redirect},
    serde::Serialize,
    Build, Rocket, State,
};
use rocket_dyn_templates::Template;
use serde::Deserialize;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

#[macro_use]
extern crate rocket;

pub mod config;
mod http;
mod https;
mod tls;
mod traefik;

pub type DbPool = SqlitePool;

const ACME_PATH: &str = "/.well-known/acme-challenge/";

#[derive(Deserialize)]
struct DbConfig {
    url: String,
}

#[launch]
async fn rocket() -> _ {
    rocket::build()
        .mount(
            "/",
            routes![
                index,
                redeploy,
                http::index,
                http::create,
                http::update,
                http::enable,
                http::delete,
                http::to_https,
                https::index,
                https::create,
                https::update,
                https::enable,
                https::delete,
                https::to_http,
                tls::index,
                tls::create,
                tls::update,
                tls::enable,
                tls::delete,
                config::index,
                config::update
            ],
        )
        .mount("/static", FileServer::from("templates/static"))
        .attach(Template::fairing())
        .attach(AdHoc::on_ignite("Init Database", init_database))
        .attach(AdHoc::on_ignite(
            "Export Traefik Config",
            initialize_traefik_config,
        ))
        .attach(AdHoc::on_shutdown("Close Database", |rocket| {
            Box::pin(async move {
                if let Some(db) = rocket.state::<DbPool>() {
                    db.close().await;
                }
            })
        }))
        .manage(config::ConfigState::load().unwrap())
}

async fn init_database(rocket: Rocket<Build>) -> Rocket<Build> {
    let db_config: DbConfig = rocket
        .figment()
        .extract_inner("databases.sqlite_database")
        .expect("sqlite_database config");

    let database_url = if db_config.url.starts_with("sqlite:") {
        db_config.url
    } else {
        format!("sqlite://{}", db_config.url)
    };

    let pool = SqlitePoolOptions::new()
        .connect(&database_url)
        .await
        .expect("database connection");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("sqlx migrations");

    rocket.manage(pool)
}

#[derive(Serialize)]
struct Index {
    flash: Option<(String, String)>,
    http_count: i64,
    https_count: i64,
    tls_count: i64,
    config: String,
}

#[get("/")]
async fn index(
    db: &State<DbPool>,
    flash: Option<FlashMessage<'_>>,
    config: &State<ConfigState>,
) -> Template {
    let http_count = http::HttpRoute::count(db.inner()).await.unwrap_or(0);
    let https_count = https::HttpsRoute::count(db.inner()).await.unwrap_or(0);
    let tls_count = tls::TlsRoute::count(db.inner()).await.unwrap_or(0);
    let config = generate_traefik_config(db.inner(), &config.config()).await;
    Template::render(
        "index",
        &Index {
            flash: flash.map(FlashMessage::into_inner),
            http_count,
            https_count,
            tls_count,
            config,
        },
    )
}

#[post("/redeploy")]
async fn redeploy(db: &State<DbPool>, config: &State<ConfigState>) -> Flash<Redirect> {
    export_traefik_config(db.inner(), &config.config()).await;

    Flash::success(Redirect::to("/"), "Traefik config updated")
}

async fn generate_traefik_config(conn: &DbPool, config: &Config) -> String {
    let mut traefik_config = tls::TlsRoute::generate_traefik_config(conn).await;
    let http = http::HttpRoute::generate_traefik_config(conn).await;
    let https = https::HttpsRoute::generate_traefik_config(conn, config).await;

    traefik_config.http.merge(http);
    traefik_config.http.merge(https);

    traefik_config.http.add_default_middlewares();

    serde_yaml::to_string(&traefik_config).unwrap()
}

pub async fn export_traefik_config(conn: &DbPool, config: &Config) {
    let config = generate_traefik_config(conn, config).await;

    std::fs::write("./traefik/gui.yml", config).unwrap();
}

async fn initialize_traefik_config(rocket: Rocket<Build>) -> Rocket<Build> {
    let db = rocket.state::<DbPool>().expect("database pool");

    let config = ConfigState::load().unwrap();

    export_traefik_config(db, &config.config()).await;

    rocket
}
