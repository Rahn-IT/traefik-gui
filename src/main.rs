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

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_sync_db_pools;
#[macro_use]
extern crate diesel;

pub mod config;
mod http;
mod https;
mod schema;
mod tls;
mod traefik;

#[database("sqlite_database")]
pub struct DbConn(diesel::SqliteConnection);

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
                https::index,
                https::create,
                https::update,
                https::enable,
                https::delete,
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
        .attach(DbConn::fairing())
        .attach(AdHoc::on_ignite("Run Migrations", run_migrations))
        .attach(AdHoc::on_ignite(
            "Export Traefik Config",
            initialize_traefik_config,
        ))
        .manage(config::ConfigState::load().unwrap())
}

async fn run_migrations(rocket: Rocket<Build>) -> Rocket<Build> {
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

    DbConn::get_one(&rocket)
        .await
        .expect("database connection")
        .run(|conn| {
            conn.run_pending_migrations(MIGRATIONS)
                .expect("diesel migrations");
        })
        .await;

    rocket
}

#[derive(Serialize)]
struct Index {
    flash: Option<(String, String)>,
    http_count: i64,
    tls_count: i64,
    config: String,
}

#[get("/")]
async fn index(
    conn: DbConn,
    flash: Option<FlashMessage<'_>>,
    config: &State<ConfigState>,
) -> Template {
    let http_count = http::HttpRoute::count(&conn).await.unwrap_or(0);
    let tls_count = tls::TlsRoute::count(&conn).await.unwrap_or(0);
    let config = generate_traefik_config(&conn, &config.config()).await;
    Template::render(
        "index",
        &Index {
            flash: flash.map(FlashMessage::into_inner),
            http_count,
            tls_count,
            config,
        },
    )
}

#[post("/redeploy")]
async fn redeploy(conn: DbConn, config: &State<ConfigState>) -> Flash<Redirect> {
    export_traefik_config(&conn, &config.config()).await;

    Flash::success(Redirect::to("/"), "Traefik config updated")
}

async fn generate_traefik_config(conn: &DbConn, config: &Config) -> String {
    let mut traefik_config = tls::TlsRoute::generate_traefik_config(conn).await;
    let http = http::HttpRoute::generate_traefik_config(conn).await;
    let https = https::HttpsRoute::generate_traefik_config(conn, config).await;

    traefik_config.http.merge(http);
    traefik_config.http.merge(https);

    traefik_config.http.add_default_middlewares();

    let serialized = serde_yaml::to_string(&traefik_config).unwrap();

    serialized
}

pub async fn export_traefik_config(conn: &DbConn, config: &Config) {
    let config = generate_traefik_config(conn, config).await;

    std::fs::write("./traefik/gui.yml", config).unwrap();
}

async fn initialize_traefik_config(rocket: Rocket<Build>) -> Rocket<Build> {
    let conn = DbConn::get_one(&rocket).await.expect("database connection");

    let config = ConfigState::load().unwrap();

    export_traefik_config(&conn, &config.config()).await;

    rocket
}
